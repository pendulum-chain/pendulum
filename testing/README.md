# PEN Migration — local validation harness

Automates the checks in [`docs/pen-migration-local-test-plan.md`](../docs/pen-migration-local-test-plan.md)
so they run as a command with pass/fail output, instead of a manual checklist.
Exits non-zero on any failure, so it can gate a deployment step.

## Phase 2 — Pendulum on Chopsticks

Exercises what unit tests cannot: that the runtime upgrade applies to **real
mainnet storage**, that it ships **paused**, and that `migrate` behaves against
genuine holder state — whales, vesting schedules, staking locks and the real
`py/trsry` treasury account.

```bash
# 1. build the runtime that will be proposed as the upgrade
cargo build --release -p pendulum-runtime

# 2. fork mainnet with that runtime applied
npx @acala-network/chopsticks@latest --config testing/chopsticks.yml \
  --wasm-override target/release/wbuild/pendulum-runtime/pendulum_runtime.compact.compressed.wasm

# 3. run the checks
cd testing && npm install
node src/phase2-pendulum.mjs
```

### Two things that will bite you

**Chopsticks must be fresh for every run.** The phase asserts the state of a
*just-upgraded* chain, so a chain that has already run the harness will report
`nextNonce != 0` and the script refuses to continue. The config deliberately
sets no `db:` — a persisted database carries forward the blocks the harness
itself produced, which would make the ships-paused check pass or fail for the
wrong reason. Restart Chopsticks between runs.

**Storage overrides land after extrinsics in the same block.** `dev_setStorage`
followed immediately by a transaction means the transaction sees the *old*
state. Every helper here produces a block after writing storage. Related: the
human-readable object form treats a falsy value as a *deletion*, so setting
`Paused: false` deletes the key — and because `Paused` defaults to `true`, it
reads back paused. The harness writes that key as a raw `0x00` instead.

There is no sudo pallet on Pendulum, so root-only calls (`migrate_treasury`,
`set_paused`) cannot be dispatched here. The harness simulates their effect via
`dev_setStorage`; the extrinsics themselves are covered by the pallet's unit
tests.

## Phase 1 — contracts on Anvil

Deploys with the **real** `script/Deploy.s.sol`, including its two-step admin
handover, then asserts against the deployed bytecode. Unit tests exercise the
contract; this exercises the thing we ship and the script that ships it.

```bash
anvil --port 8545
node testing/src/phase1-base.mjs
```

The daily cap is a **rolling leaky bucket and is shared state across checks**.
It refills proportionally to the *current* `dailyCap`, so lowering the cap
slows the decay of consumption already recorded — always `setCaps` first and
warp afterwards, never the reverse.

## Phase 3 — the whole pipeline together

Runs four attestors, the monitor and the releaser against Chopsticks and Anvil,
and asserts that a burn on the Substrate side arrives on Base with no manual
step, that the fleet tolerates outages, and that a cap-deferred release drains
itself.

```bash
anvil --port 8545
npx @acala-network/chopsticks@latest --config testing/chopsticks-e2e.yml \
  --wasm-override target/release/wbuild/pendulum-runtime/pendulum_runtime.compact.compressed.wasm
# build the services once: (cd attestor && npm run build), same for monitor/ and releaser/
node testing/src/phase3-e2e.mjs
```

Chopsticks stands in for Pendulum rather than Zombienet: it reports finalized
heads, which is what the attestors subscribe to, and it carries real state.
What it does **not** reproduce is genuine relay-chain finality timing — lag,
and the possibility of a fork before finality. That remains a manual Zombienet
exercise.

Phase 3 uses `chopsticks-e2e.yml`, which *does* cache to a `db`: unlike phase 2
it makes no claims about a just-upgraded chain, and the cache keeps a long run
alive when the upstream public RPC drops the connection, which it does.

### Things that cost real debugging time here

- **Never reuse a wasm built with `--features runtime-benchmarks`.** It
  references host functions Chopsticks does not provide and fails with
  `Unresolved function ext_benchmarking_add_to_whitelist_version_1`. Building
  the node with that feature silently overwrites the runtime wasm, so rebuild
  with `cargo build --release -p pendulum-runtime` afterwards.
- **`START_BLOCK` for the attestors must be the chain head**, not 0. A fork
  sits at ~7.6M blocks and a daemon starting from 0 walks every one of them.
  The same applies in production: it is the block the pallet went live at.
- **Kill stray daemons between runs.** An aborted run leaves processes that
  keep writing the same checkpoint files, so a fresh attestor loads a stale
  checkpoint moments after the harness cleared it. `killStrays()` handles this,
  and `stopAndWait` is used wherever a test depends on a daemon really being
  down — signalling alone lets it land one more transaction.
