# PEN Migration — Internal Security Review (pre-audit)

**Date:** 2026-07-07
**Scope:** MigrationVault.sol, PEN.sol, PENGovernor.sol, deploy scripts,
token-migration pallet, attestor daemon, invariant monitor.
**Method:** adversarial review by an independent reviewer agent against the
PRD requirements (P1–P9, V1–V9, A1–A5, M1–M4), cross-checked against the test
suite. This is an *internal* pass — it precedes and does not replace the
external audits (PRD §9).

## Findings and resolutions

### 1. HIGH — Attestor daemon treated the normal 3-of-5 race as fatal
With five independent attestors racing to approve the same event, the two
whose transactions land after the third matching approval revert with
`NonceAlreadyConsumed`. The daemon treated any revert as fatal (alert +
exit), meaning two attestors would crash-loop on nearly every migration —
alert fatigue that could mask real incidents.

**Resolution (fixed):** on any submission failure the daemon re-checks
`nonceConsumed`/`hasApproved`; if the migration is resolved on-chain the race
is logged as benign and processing continues. Unexplained failures still
alert and exit (PRD A5 preserved).

### 2. HIGH — Monitor reads were not pinned to one block
`totalReleased` and `balanceOf(vault)`/`totalSupply` were read in separate
batches without a block tag. A release landing between the batches would
produce a false `VAULT BALANCE MISMATCH` and — with a guardian key configured
— an unjustified auto-pause (48h+ to undo post-handover, since unpause is
timelocked).

**Resolution (fixed):** all Base-side reads in a check cycle are pinned to a
single `blockNumber`. The Pendulum-then-Base read ordering of the M2a check
was confirmed safe by construction (burns finalize strictly before releases).

### 3. MEDIUM — `sweepRemainder` could strand approved-but-deferred releases
A payload can reach quorum while its release is deferred (pause or caps); the
owed tokens still sit in the vault balance. Sweeping the full balance at
window close would leave such a release permanently unexecutable — the user
already burned on Pendulum.

**Resolution (fixed):** the vault now tracks `pendingApprovedAmount`
(payloads that crossed the threshold without releasing; cleared on release).
`sweepRemainder` transfers `balance − pendingApprovedAmount`. A timelocked
`clearStalePending` exists for pending entries of *consumed* nonces only
(conflicting tuples that lost the race); unconsumed pending entries are owed
to their migrator and can never be cleared. Covered by three new tests.

### Minor (fixed in the same pass)
- Attestor event decoding now asserts the 4-field event shape explicitly, so
  a runtime upgrade that changes the event fails loudly instead of decoding
  positionally into garbage.
- The attestor's periodic gas-balance check no longer swallows RPC errors.

## Explicitly verified as not vulnerable
- **Replay/double-release:** `nonceConsumed` gates both `approve` and
  `release` and is set before the transfer; conflicting tuples never merge.
- **Reentrancy:** checks-effects-interactions ordering in `_release`; the
  token is hook-free OZ code.
- **Admin takeover / role wiring:** two-step admin transfer; deploy scripts
  leave no dangling deployer privileges; timelock self-administered.

## Follow-ups for the external audit
- The cap-accounting window (`currentDay` bucketing) and attestor-rotation
  edge cases around `pendingRelease` marking (threshold crossed via
  `addAttestor` re-adding a prior approver is not marked pending) deserve
  focused auditor attention.
- The attestor's positional event decode is shape-checked but still assumes
  field order; re-verify against metadata after any runtime upgrade (RB-5).
