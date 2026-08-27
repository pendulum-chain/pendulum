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

## Still to add

Phase 1 (contracts on Anvil) and phase 3 (end-to-end with Zombienet, four
attestors, the monitor and the releaser) are still manual — see the test plan.
