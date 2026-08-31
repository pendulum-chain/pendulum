# PEN Migration — Local Test Plan

A runbook for validating the full migration stack on a laptop, with no public
testnet involved. Work through the phases in order; each has explicit pass
criteria. Phases 1–2 are independent and can be done in either order; phase 3
needs both.

**Tooling, and what each one actually proves**

| Tool | Stands in for | Proves |
|---|---|---|
| **Anvil** (Foundry) | Base | Contract behaviour, deploy script, attestor/monitor wiring |
| **Chopsticks** | Pendulum mainnet | Runtime upgrade + pallet against **real** balances, locks, vesting, treasury |
| **Zombienet** | Relay + parachain | The **relay-chain finality** path — attestors only act on finalized blocks, which Chopsticks cannot faithfully reproduce (phase 4) |
| **Base Sepolia** | Base | Real gas estimation, block times and RPC behaviour, against a public chain (phase 5) |

Chopsticks gives realistic *state*; Zombienet gives realistic *finality*. You
need both, for different reasons. Neither requires Paseo or Foucoco.

> Foucoco is **not** used: that chain is no longer live. Validation is this
> local stack plus a public run on Base Sepolia for the contracts. The
> discussion post's reference to Foucoco should be corrected when the formal
> proposal is published.

---

## Phase 0 — Prerequisites

```bash
# Build the runtime wasm that will be tested as the upgrade
cargo build --release -p pendulum-runtime
# -> target/release/wbuild/pendulum-runtime/pendulum_runtime.compact.compressed.wasm

# Contracts + services
cd contracts && forge build
cd ../attestor && npm install && npm run build
cd ../monitor && npm install && npm run build
```

Confirm the baseline is green before starting:

```bash
cargo test -p token-migration --features runtime-benchmarks
cd contracts && forge test
cd ../attestor && npm test
cd ../monitor && npm test
```

**Pass:** all suites green.

---

## Phase 1 — Base side on Anvil

Goal: the contracts behave as specified against a real EVM, and the deploy
script works with realistic parameters.

```bash
anvil --port 8545          # terminal 1
cd contracts
cp .env.example .env       # fill in: 4 attestor addrs, Safes, caps, MAX_ISSUANCE, EARLIEST_SWEEP_TS
forge script script/Deploy.s.sol --rpc-url http://localhost:8545 --broadcast
```

Automated by `testing/src/phase1-base.mjs` (`node testing/src/phase1-base.mjs`),
which deploys with the real script and then asserts:

1. `PEN.totalSupply()` == `MAX_ISSUANCE`, and `PEN.balanceOf(vault)` == the same.
2. `vault.threshold()` == 3, `vault.attestorCount()` == 4.
3. `vault.paused()` == false; `vault.token()` == the PEN address.
4. Approve one migration from 3 attestor keys → recipient receives
   `palletAmount × 1e6`; `nonceConsumed(nonce)` == true.
5. Approve from only 2 → nothing released.
6. **Cap deferral:** approve an amount above `perReleaseCap` from 3 attestors →
   no release, `pendingApprovedAmount` increases, `ReleasePending` emitted.
   Then `setCaps` higher and call `release(...)` → succeeds.
7. **Rolling cap:** consume the full `dailyCap`, confirm
   `availableDailyAllowance()` == 0, warp 12h (`evm_increaseTime`), confirm it
   has refilled by half.
8. **Guardian pause:** pause from the guardian key → `release` reverts;
   unpause is rejected from the guardian and accepted from admin.
9. `sweepRemainder` reverts before `earliestSweepTimestamp`.

**Pass:** 11/11 from the script. These mirror the Foundry suite, but run
against the deployed bytecode and the real deploy script — that is the point.

Note the daily cap is a rolling bucket and is **shared state across checks**,
refilling proportionally to the *current* `dailyCap`. Always `setCaps` first
and warp afterwards; warping first and lowering the cap leaves the earlier
consumption largely undecayed.

---

## Phase 2 — Pendulum side on Chopsticks (real mainnet state)

Goal: the runtime upgrade applies cleanly, ships **paused**, and the pallet
behaves correctly against genuine holder state — locked, vesting, staked and
whale accounts as they exist today.

`chopsticks.yml`:

```yaml
endpoint: wss://rpc-pendulum.prd.pendulumchain.tech
mock-signature-host: true
db: ./chopsticks-db.sqlite
port: 8000
```

```bash
npx @acala-network/chopsticks@latest --config chopsticks.yml \
  --wasm-override target/release/wbuild/pendulum-runtime/pendulum_runtime.compact.compressed.wasm
```

Produce a block (`dev_newBlock`) so the upgrade takes effect, then check:

1. **Ships paused (the critical one).** `tokenMigration.paused()` == `true`
   immediately after the upgrade, with no storage written. Any `migrate` call
   fails `MigrationsPaused`.
2. `tokenMigration.nextNonce()` == 0, `totalMigrated()` == 0,
   `treasuryDestination()` == None.
3. Unpause via sudo/root (`setPaused(false)`), then run the cases below.
4. **Happy path:** fund a dev account via `dev_setStorage`, `migrate(amount,
   0x…)` → balance drops, total issuance drops by the same amount,
   `MigrationInitiated` carries `{nonce, who, base_address, amount}` in that
   field order (the attestor decodes positionally — this is the check that
   catches event drift).
5. **Encumbered balances, against real accounts.** Pick a genuinely staked
   account and a genuinely vesting account from mainnet state and confirm
   `migrate` of the locked portion fails; the transferable portion succeeds.
6. **Dust/ED rule:** migrating all-but-a-sliver fails `WouldLeaveDust`;
   migrating the entire free balance succeeds.
7. **Zero address:** `migrate(amount, 0x000…0)` fails `InvalidBaseAddress`.
8. **Treasury path:** `setTreasuryDestination` then `migrateTreasury` from
   root — burns from the real `py/trsry` account, keeps it alive, and emits an
   event identical in shape to a user migration.
9. **Nonce continuity:** several migrations across both paths share one
   monotonic nonce sequence with no gaps or reuse.

**Pass:** 1–9 all hold. Item 1 is the launch-safety property; do not proceed if
it fails.

---

## Phase 3 — the whole pipeline together (Chopsticks + Anvil)

Goal: a burn on the Substrate side reaches Base with no manual step, driven by
four attestor processes, the monitor and the releaser running as a system.
This is the only place the components race each other, and it is where the
remaining risk lives — both production bugs found during this work (attestor
gas under-estimation, releaser Multicall) were invisible to unit tests and
appeared here.

Automated by `testing/src/phase3-e2e.mjs`. Chopsticks stands in for Pendulum:
it reports finalized heads, which is what the attestors subscribe to, and it
carries real state. Use `testing/chopsticks-e2e.yml`, which caches to a `db` —
unlike phase 2 it makes no claim about a just-upgraded chain, and the cache
keeps a long run alive when the upstream public RPC drops the connection.

Genuine relay-chain finality is covered separately in phase 4.

The script starts the four attestors, the monitor and the releaser itself,
each with its own key and checkpoint file. To run them by hand instead:

```bash
PENDULUM_WS=ws://127.0.0.1:9944 BASE_RPC_URL=http://localhost:8545 \
VAULT_ADDRESS=0x… ATTESTOR_PRIVATE_KEY=0x… CHECKPOINT_FILE=./cp1.json \
npm start   # repeat for attestors 2–4 with distinct keys/checkpoints
```

The releaser needs only a gas-funded key and the Base endpoint — no Pendulum
connection, and no privileges over the vault:

```bash
BASE_RPC_URL=http://localhost:8545 VAULT_ADDRESS=0x… \
RELEASER_PRIVATE_KEY=0x… START_BLOCK=0 npm start
```

Checks:

1. **Full path:** unpause the pallet, `migrate` from a funded account → within
   a block or two of finality, 3 approvals land and the recipient's PEN
   balance on Anvil equals `amount × 1e6`.
2. **The race is benign.** All four attestors see the same event; the two that
   lose the race log a skip and **stay running**. No crash-loop, no fatal
   alert. (This is the failure mode that took three review rounds to get
   right — verify it explicitly.)
3. **Restart safety:** kill an attestor mid-run, restart it → it resumes from
   its checkpoint, re-derives nothing twice, no duplicate release.
4. **Outage tolerance:** stop one attestor → migrations still release (3 of 4
   remain). Stop a second → releases stop cleanly, nothing is lost, and the
   monitor raises a liveness alert. Restart both → the backlog drains.
5. **Deferred releases drain by themselves.** Set a low `dailyCap`, migrate
   enough to exhaust it, and confirm the excess is marked pending
   (`ReleasePending`) rather than reverting — then that the releaser picks it
   up and completes it as the bucket refills, with **no manual intervention**.
   Restart the releaser mid-backlog and confirm it resumes from its state file.
6. **Monitor invariants:** the monitor logs `ok` with
   `balance + released + swept == totalSupply` holding continuously.
7. **Conservation alarm:** manually transfer PEN out of the vault on Anvil to
   create a deficit → the monitor alerts and (if `GUARDIAN_PRIVATE_KEY` is
   set) auto-pauses the vault. **Then verify the reverse:** send PEN *into*
   the vault → surplus is tolerated, no false alert.
8. **Portal:** run the portal against the local chain with
   `VITE_MIGRATION_VAULT_ADDRESS` set to the Anvil vault; migrate through the
   UI and watch the status card go 0/3 → 3/3 → released.

**Pass:** 7/7 from the script.

Three traps cost real debugging time and are worth knowing before you run it:
a wasm built with `--features runtime-benchmarks` cannot be used as a
Chopsticks override (building the node for benchmarks silently overwrites the
runtime wasm); the attestors' `START_BLOCK` must be the chain head, not 0, or
each daemon walks ~7.6M historical blocks; and strays from an aborted run keep
rewriting the checkpoint files a fresh run just cleared.

---

## Phase 4 — real relay-chain finality (Zombienet)

Goal: prove the finality gate is real. Chopsticks finalises every block it
authors, so an attestor reading finalized heads there is indistinguishable from
one reading best heads — the safety property is untested by construction. This
phase runs a genuine relay chain, where finalized lags best.

```bash
node testing/src/make-zombienet-spec.mjs     # generates testing/.zombienet-pendulum-raw.json
./zombienet-macos-arm64 spawn testing/zombienet.toml --provider native
node testing/src/phase4-zombienet.mjs        # auto-discovers the collator RPC
```

Checks:

1. The collator serves the **Pendulum** runtime (not the relay — the collator
   exposes an embedded relay client too, and a fixed port is as likely to hit
   Rococo).
2. `tokenMigration` is in the runtime metadata with all four extrinsics.
3. **Ships paused** on a chain that never wrote the storage — the same
   fail-safe default phase 2 checks against forked mainnet state, here on a
   chain built from genesis.
4. The parachain is authoring blocks.
5. **Finalized advances and lags best.** This is the phase: it proves both that
   the relay is finalising parachain blocks at all, and that finality is not
   instant.
6. The lag is strictly positive at least once — i.e. genuinely relay-driven.
7. `subscribeFinalizedHeads` delivers monotonically increasing heads, which is
   the exact subscription the attestor uses.

**Pass:** 7/7. Observed: a steady ~2-block parachain finality lag (best #9 /
finalized #7) behind a relay running its own ~3-block lag — against mainnet the
measured figure is ~2 blocks / ~47s, comfortably inside the monitor's
`GRACE_SECONDS` default of 1800.

Three traps here cost real debugging time:

- **A benchmarking build breaks the relay, not the node.** `cargo build
  --features runtime-benchmarks` rewrites the runtime wasm embedded in the node
  binary, `build-spec` propagates it into the chain spec, and the result
  decompresses past the relay's `VALIDATION_CODE_BOMB_LIMIT`
  (`MAX_CODE_SIZE * 4` = 12 MiB). The relay then rejects every candidate with
  `PossibleBomb` and the parachain stalls at its own block #1 while the relay
  looks perfectly healthy. `make-zombienet-spec.mjs` swaps in the shipped
  artifact to avoid this.
- **The collator must be named `alice`.** Zombienet only derives `//Alice` for a
  node with that name, and genesis pins the authority to that key. A collator
  renamed by a collision (`alice-1`) silently gets a key that is not in genesis
  and never authors. Hence the relay validators being `validator01`/`02`.
- **`build-spec` cannot read back its own output**, so Zombienet cannot build
  this spec itself; see `fix-chainspec.mjs`.

An older relay is fine. This ran against polkadot **0.9.40** driving a
**1.6.0** collator; the version gap does not affect finality behaviour, and the
only incompatibility encountered was the code-size limit above, which is our
artifact's problem rather than the relay's.

---

## Phase 5 — full-stack rehearsal (Zombienet + Base Sepolia)

Goal: run the whole system against real infrastructure on both sides at once,
with no real value at stake. Phases 1–4 each hold one half still — Anvil is
instant and single-node, Chopsticks fakes finality. This is the only phase
where genuine relay finality and a public EVM meet, which is where both
production bugs found during this work actually lived.

```bash
cp testing/.env.rehearsal.example testing/.env.rehearsal   # fill in throwaway keys
node testing/src/rehearsal.mjs --preflight                 # lists what needs funding
# claim Base Sepolia ETH once into the deployer address, then:
node testing/src/rehearsal.mjs --fund                      # fans gas out to the other seven
node testing/src/rehearsal.mjs
```

Gas is sized against measured cost: a whole run — two deployments plus ~20
approvals — is about **0.00005 ETH** on Base Sepolia, so the ~0.014 ETH the
roles hold between them covers many runs. Faucets are rate-limited per address,
which is why `--fund` exists: claim once into the deployer rather than eight
times.

| Flag | Effect |
|---|---|
| `--preflight` | Check prerequisites and role funding, deploy nothing |
| `--fund` | Top up any underfunded role from the deployer; idempotent |
| `--keep` | Leave the network and fleet running for manual poking |
| `--attach` | Use an already-running Zombienet instead of spawning one |
| `--skip-slow` | Skip the wall-clock cap-refill scenario |

It brings up Zombienet, waits for genuine parachain finality, unpauses the
pallet **through the technical-committee origin** (this chain has no sudo, so
unlike phase 2 there is no storage poke — the rehearsal drives the same origin
that will unpause mainnet), deploys the contracts to Base Sepolia with the real
`Deploy.s.sol`, starts four attestors plus the monitor and releaser, and runs
the scenarios. Every run writes `testing/.rehearsal/<timestamp>/` containing a
manifest (addresses, ports, block heights, commit) and each daemon's log, so a
failed run stays diagnosable after teardown.

Four design decisions worth knowing before you change it:

- **Contracts are redeployed every run, deliberately.** The Zombienet chain is
  ephemeral and restarts its nonce sequence at zero on each spawn, while the
  vault's `nonceConsumed` mapping is permanent. Reusing a vault means the second
  run re-emits nonce 0, every attestor's pre-check returns "already handled",
  and the pipeline logs skips while testing nothing. A guard asserts this
  explicitly rather than trusting the convention. Redeploying also exercises the
  deploy script on every cycle.
- **Caps are sized for wall clock, not for production.** There is no
  `evm_increaseTime` on a public chain, so the rolling bucket has to refill in
  real minutes: at `DAILY_CAP` = 28,800 PEN it returns 100 PEN (the on-chain
  minimum migration) every ~5 minutes. The production cap values remain
  validated only in phase 1, where time can be warped — the two phases are
  complementary and neither is sufficient alone.
- **It refuses to run anywhere that could cost money.** Base mainnet (chain
  8453) is rejected outright, any chain other than Sepolia needs an explicit
  override, and the Substrate endpoint must self-report as the local chain.
  Checked before anything is deployed or signed.
- **Teardown is part of the contract.** Stray daemons from an aborted run
  rewrite the checkpoint files a fresh run just cleared, so processes are killed
  by reading the process table rather than `pkill -f` — a shell running
  `pkill -f <pattern>` matches its own command line, which is exactly how an
  earlier session produced three waiter shells that could never terminate.

**Pass:** all scenarios green. Keys are throwaway and testnet-only;
`testing/.env.rehearsal` is gitignored and must never hold a key that will see
mainnet.

---

## Phase 6 — Failure drills (the runbooks)

Rehearse each runbook against a real stack, so the first time anyone runs them
is not during an incident. Automated:

```bash
node testing/src/drills.mjs        # RB-1, RB-3 (surplus), RB-4, RB-6, RB-7 on the Sepolia stack
# RB-5 needs Chopsticks (the one place a runtime can be swapped) and a
# spec-bumped wasm; see the header of drill-rb5-upgrade.mjs:
UPGRADE_WASM=/path/to/spec+1.wasm node testing/src/drill-rb5-upgrade.mjs
```

| Runbook | Where it is drilled |
|---|---|
| RB-1 key compromise | `drills.mjs`: attestor removed with its approval recorded on a paused, exactly-at-threshold payload — the vote stops counting and nobody can complete the release below quorum |
| RB-2 outage | Phase 5 rehearsal (one down still releases; two down stops cleanly) |
| RB-3 invariant breach | Deficit + auto-pause: phase 3 on Anvil (a deficit **cannot be created** on Sepolia — nothing but the vault can move its tokens, which is the security property). Surplus tolerance: `drills.mjs` |
| RB-4 pause/unpause | `drills.mjs`: pallet then vault, resumed in reverse, pipeline recovers |
| RB-5 runtime upgrade | `drill-rb5-upgrade.mjs` on Chopsticks: fork pre-upgrade mainnet (no pallet), enact the real spec-26 upgrade by writing `:code`, confirm frame-system records it, and require every attestor's checkpoint to advance past the upgrade block — their decode path ran on post-upgrade blocks, and a decode failure exits by design — including across a full node restart. A post-upgrade `migrate` cannot be *submitted* through Chopsticks (it serves pre-fork metadata even across `--resume`, so the real runtime rejects the extrinsic as `badProof`); decoding real events under the post-upgrade runtime is what phases 2–3 do wholesale, and the shape-change half is the attestor's unit-tested 4-field assertion |
| RB-6 attestor rotation | `drills.mjs`: re-add bumps the generation so the old vote stays dead; the documented recovery (rewind checkpoint, restart, re-approve) completes the quorum; a 5th attestor adds and removes cleanly |
| RB-7 window close | `drills.mjs`: pallet paused, `pendingApprovedAmount` drained, conservation reconciled exactly, sweep executed, monitor quiet. Deploys with a ~90s sweep floor since Sepolia time cannot be warped; the threshold is never reduced, so the 7-day settling gate stays unarmed |

---

## Phase 7 — Exit criteria before mainnet

- [ ] Phases 1–6 pass end to end.
- [ ] The upgrade ships paused, verified on a Chopsticks fork of **live**
      mainnet state (not a fresh chain).
- [ ] A cap-deferred release recovers correctly without manual contract
      surgery.
- [ ] All four attestors survive a full run without a fatal exit.
- [ ] Finality gating confirmed against a real relay: finalized lags best, and
      the attestors act only on the finalized stream.
- [ ] The monitor alerts on a real injected deficit and tolerates a surplus.
- [ ] Benchmarks re-run on reference hardware and the generated weights
      replace the manual estimates.
- [ ] A dry run of the deploy script with the **final** production parameters,
      reviewed by someone other than whoever wrote the `.env`.
- [ ] The phase 5 rehearsal green on Base Sepolia against the shipped revision.
