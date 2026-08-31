# PEN → Base Migration — Review Handover (round 8)

A briefing for a fresh reviewer. Goal: find the bugs the previous rounds
missed. Everything below exists to make you effective fast — but treat every
claim in this document, and in every other doc, as **unverified**: the docs
were written by the same process that wrote the code. Verify against the code
at HEAD.

## What you are reviewing

One-way migration of the PEN token from the Pendulum parachain to Base.
Holders call `tokenMigration.migrate(amount, base_address)` on Pendulum; the
amount is burned and an event with a unique nonce is emitted. Four attestor
daemons watch relay-**finalized** blocks and submit
`approve(nonce, recipient, amount)` to a vault on Base; the 3rd matching
approval releases pre-minted fixed-supply ERC-20 PEN (12→18 decimals, ×1e6,
converted in exactly one place). Rate caps bound worst-case loss; an
independent monitor holds a conservation invariant and can auto-pause; a
permissionless releaser drains cap-deferred releases. Governance: OZ Governor
+ Timelock on Base, with the vault admin handed to the timelock by proposal.

| Where | What |
|---|---|
| `pallets/token-migration/` + `runtime/pendulum/src/lib.rs` | Burn-and-emit pallet, runtime wiring (index 102, pause origin, BaseFilter) |
| `contracts/src/` | `PEN.sol`, `MigrationVault.sol`, `PENGovernor.sol` |
| `contracts/script/` | `Deploy.s.sol`, `DeployGovernance.s.sol` |
| `attestor/`, `monitor/`, `releaser/` | The three daemons (TypeScript, viem + polkadot-js) |
| `testing/` | Phases 1–6: Anvil, Chopsticks, Zombienet, Base Sepolia rehearsals + failure drills |
| Portal repo, branch `feat/pen-base-migration` (PR #655) | Migration UI (`src/pages/migration/`, `src/hooks/migration/`, `src/helpers/ethereum.ts`) |

Branch: `feat/pen-to-base-migration` (PR #559), 38 commits over `main`.

## Prior review history — read it first

[pen-migration-internal-review.md](pen-migration-internal-review.md) is the
running log: seven adversarial rounds (2026-07-07…09), the Base Sepolia
rehearsal findings (2026-08-28), and a post-drills pass (2026-08-31). Per
round it records findings, fixes, and — important for you — the explicit
**"verified as not vulnerable"** lists. Rounds 3 and the rehearsal both found
bugs in *earlier rounds' fixes*, so do not treat a prior "fixed" as settled:
the fix commits are part of your attack surface.

What each round already hammered (highest marginal value lies elsewhere):
replay/race/rotation on `approve()` (r1–r3), sweep-vs-pending accounting
(r1, r3, r4), monitor conservation and its false-positive/bricking modes
(r1, r5, r6), outsider griefing of the fleet (r6, r7), zero-address and
vault-as-recipient poison events (r2, r6), threshold-decrease retroactivity
(r4), rolling-cap semantics (r4).

## The four bug classes that kept recurring

Every real bug found post-unit-tests belongs to one of these. The most likely
"last remaining bug" is another instance of one of them:

1. **A benign event misread as fatal takes the fleet down.** Four variants
   found so far: the k-of-n race as crash (r1), zero-address decode (r2),
   OutOfGas from bimodal gas estimation (`approve` records cheaply or
   releases expensively depending on arrival order), and a lost race
   "confirmed" against a lagging RPC node. Hunt for a fifth: any path where
   `alert + exit` fires on something an adversary or ordinary timing can
   trigger cheaply and repeatedly.
2. **Read-after-write assumptions against a load-balanced RPC.** No public
   endpoint guarantees a read sees your confirmed write. Anything that
   writes then reads once — daemons, runbooks, portal polling — is suspect.
3. **Bimodal/underestimated gas.** Any transaction whose execution path
   depends on state that can change between estimation and inclusion.
4. **Execution/reporting splits in tooling.** Chopsticks executes one runtime
   while reporting another; a benchmarks build silently swaps the embedded
   wasm. Where else could the thing tested differ from the thing shipped?

## Invariants to attack

Try to falsify these directly — each is load-bearing:

- `vault.balanceOf + totalReleased + totalSwept == PEN.totalSupply`, with
  surplus tolerated and only deficit alarming.
- A nonce releases at most once, ever, across user AND treasury migrations
  (one shared sequence); no tuple `(nonce, recipient, amount)` can be
  released with different arguments than were approved.
- `activeApprovals` counts only current-generation, current-member approvals;
  `hasApproved` must agree with it exactly (they disagreed once — r3/M1).
- `pendingApprovedAmount` reserves every quorum-approved-but-deferred release
  against `sweepRemainder`, and can never underflow or leak permanently
  (check `clearStalePending`'s restrictions).
- The pallet ships **paused** via a storage default with no migration writes;
  nothing else in the runtime upgrade can flip it.
- Guardian can pause, never unpause; the two-step admin can never be skipped;
  after handover the timelock delay gates every admin action.
- `earliestSweepTimestamp` is immutable; a threshold *decrease* arms a 7-day
  sweep settling gate.
- Burn-side: total issuance decreases by exactly the migrated amount; ED/dust
  rules can't strand or destroy funds; locked/vesting/staked balances cannot
  migrate; minimum 100 PEN holds on both paths.
- Decimal conversion ×1e6 happens exactly once (vault, at release). Look for
  any second place amounts are scaled — portal display, monitor math,
  releaser, tests.

## Where review soak is thinnest — prioritize these

1. **The seven attestor/releaser commits since round 7** (`7c1b91c`,
   `dcf5401`, `844f731`, `c7f4f0c`, `7037f6f`, `f0f64e4` and the transient
   classifier): newest fund-release-path code, reviewed once, by the author.
   Specifically: can `isTransientRpcError` misclassify anything fatal as
   transient (silent-stall) or vice versa (fleet death)? Can the
   `alreadyHandledSettled` backoff interact badly with checkpointing or the
   serialized block-processing promise chain?
2. **The portal UI** — one full-diff pass (r4) only. Amount parsing and
   decimal display, EIP-55 handling in `src/helpers/ethereum.ts`, the
   payload-hash mirror of the vault's `abi.encode`, and what the status card
   does on RPC lag or a deferred release.
3. **`migrate_treasury` / `set_treasury_destination`** — the governance-only
   burn path; less exercised than user `migrate`. Check origin gating, the
   fixed-destination logic, KeepAlive semantics against the real treasury
   account.
4. **Monitor liveness (M4)** — `nonceFirstSeen` map growth, alert
   deduplication, GRACE_SECONDS interaction with finality lag; r5/r7 touched
   it twice, which historically predicts a third issue.
5. **Governance wiring** — `PENGovernor.sol` composition and
   `DeployGovernance.s.sol` ran on-chain for the first time on 2026-08-31.
   Quorum counts For+Abstain against **full** `totalSupply` (the unreleased
   vault balance counts toward the denominator — PRD G1); check whether any
   quorum/threshold interaction surprises at production numbers (150M supply,
   2% quorum, vault holding most of it early on).
6. **Runtime wiring** — pallet index 102, `BaseFilter` whitelist additions,
   pause origin (root / half-council / 2/3 technical committee), locally
   generated benchmark weights (are the weight/fee margins abusable?).

## What is deliberately out of scope / accepted

- No external audit is commissioned; residual risk is carried by caps,
  monitoring + auto-pause, guardian, timelock, and the soft launch (see the
  review log's final section).
- Attestors are team-operated (3-of-4) at launch.
- Quorum *sizing* and production governance timings are unit-tested, not
  rehearsed; Chopsticks cannot submit post-upgrade extrinsics (see
  `drill-rb5-upgrade.mjs` header) — reviewer beware when judging RB-5 claims.

## Running things

```bash
cargo test -p token-migration            # 21 (22 with --features runtime-benchmarks)
cd contracts && forge test               # 37
cd attestor  && npm test                 # 6   (monitor: 7, releaser: 7)
node testing/src/phase1-base.mjs         # needs: anvil --port 8545
node testing/src/phase2-pendulum.mjs     # needs: chopsticks per docs/pen-migration-local-test-plan.md
```

Phases 5/5b/6 (Sepolia) need the funded, gitignored `testing/.env.rehearsal`
(throwaway keys; present on this machine). The full map with pass criteria is
[pen-migration-local-test-plan.md](pen-migration-local-test-plan.md); phase
scripts themselves are fair review targets — check that assertions actually
assert what their names claim.

## Rules of engagement

- Report findings as `file:line`, one sentence of defect, one concrete
  failure scenario (inputs/state → wrong outcome). Severity by worst-case
  fund impact first, fleet availability second.
- Adversarially verify your own findings before reporting — prior rounds'
  false positives cost real time.
- The standing practice applies to you too: if your findings change the
  fund-release path, that change itself needs a fresh pass.
