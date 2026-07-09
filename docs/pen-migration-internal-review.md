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

## Round 2 (2026-07-07, second independent reviewer over the full diff of both repos)

### C1. CRITICAL — Zero-address migration deadlocked the entire attestor fleet
`migrate(amount, H160::zero())` was accepted by the pallet and the portal, but
the vault deterministically rejects a zero recipient. Every attestor would hit
the same permanent revert at the same block, alert, exit, and — because the
checkpoint only advances after a block fully processes — crash-loop forever.
One 1-PEN transaction could halt every migration behind it for all five
operators simultaneously.

**Resolution (fixed, defense in depth):** the pallet rejects
`H160::zero()` (`InvalidBaseAddress`, with test); the portal validator rejects
the zero address; the attestor statically detects vault-unreleasable tuples
(zero recipient/amount), raises a distinct CRITICAL alert, and skips past the
event instead of crash-looping — such an event can now only mean a
pallet/vault validation mismatch.

### H1. HIGH — Re-adding a removed attestor could cross a threshold outside `approve()`
`activeApprovals` counted historical approvals against the current attestor
set, so `addAttestor` re-adding an address with stale recorded approvals could
push a payload over the threshold without running the pending-release
accounting in `approve()` — re-opening the sweep-stranding hole of round-1
finding 3 through a rotation side door.

**Resolution (fixed structurally):** attestor **generations**. Every
`addAttestor` bumps the address's generation and approvals only count while
their recorded generation matches — a re-added attestor must approve again, so
the threshold can only ever be crossed inside `approve()`. The public
`hasApproved` view now means "holds a currently-valid approval" (same ABI, so
the daemon keeps working and correctly re-approves after a re-add). Covered by
a regression test.

### Round 2 explicitly verified as not vulnerable
Portal EIP-55 implementation and keccak string semantics; portal/attestor
payload-hash construction exactly mirroring the vault's `abi.encode`;
pallet↔attestor↔vault↔portal event-field alignment; replay/reentrancy/
conflicting-tuple logic (re-confirmed); monitor block-pinning fix;
`clearStalePending` restrictions; deploy-script role wiring.

## Round 3 (2026-07-07, third independent reviewer, focused on the round-1/2 fixes)

All three findings trace to the round-1 `sweepRemainder`/`pendingApprovedAmount`
mechanism never being re-verified against *sub-threshold* in-flight migrations
or against the monitor's conservation formula.

### C1(r3). CRITICAL — `sweepRemainder` could strand an in-flight migration and crash-loop the fleet
`pendingApprovedAmount` reserves only payloads that have already crossed the
threshold. A migration with 1–2 approvals at sweep time reserved nothing, so
`sweepRemainder` (which swept `balance − pendingApprovedAmount`) could remove
its tokens. When the remaining attestors then crossed the threshold, the
inline release in `approve()` reverted on insufficient balance — rolling back
the approval, and, because every attestor hit it identically, permanently
crash-looping 3-of-5 daemons and halting all future migrations.

**Resolution (fixed):**
- `approve()` now includes vault balance in its `releasable` check, so an
  under-funded release **defers** (marks pending) instead of reverting — the
  fleet can never crash-loop on it, and the debt stays tracked and recoverable
  after a governance refund. (`release()` gained a matching
  `InsufficientVaultBalance` guard.)
- `sweepRemainder(to, amount)` now takes an explicit amount bounded by
  `balance − pendingApprovedAmount` (saturating, so a prior over-sweep can't
  cause an underflow revert), forcing conscious reconciliation.
- New runbook **RB-7** (window close) mandates pausing the pallet and
  confirming zero outstanding nonces via the monitor before sweeping.
- Tests: `test_OverSweptInFlightMigrationDefersAndRecovers` proves the fleet
  stays up and the migration recovers; sweep tests updated to the new
  signature.

### H1(r3). HIGH — Monitor's M2b check ignored `sweepRemainder`
`sweepRemainder` moved tokens out without touching `totalReleased`, so the
monitor's `balance + totalReleased == totalSupply` check would fire a
guaranteed false `VAULT BALANCE MISMATCH` — and auto-pause — on the first
legitimate window-close sweep.

**Resolution (fixed):** the vault now tracks `totalSwept` (incremented in
`sweepRemainder`); the monitor checks `balance + totalReleased + totalSwept ==
totalSupply`. RB-7 also notes a mismatch coinciding with a `RemainderSwept`
event is expected, not a compromise signal. The fuzz invariant test now
includes `totalSwept`.

### M1(r3). MEDIUM — `hasApproved` disagreed with `activeApprovals`
`hasApproved` checked only the generation, not `isAttestor`, so it reported
`true` for an attestor removed and never re-added (the standard RB-1 outcome),
while that approval counts 0 toward releases. Low live impact (only the
attestor self-check reads it) but wrong on a public view meant for
auditability.

**Resolution (fixed):** `hasApproved` now requires `isAttestor` too, mirroring
`activeApprovals`. Covered by `test_HasApprovedFalseForRemovedAttestor`.

### Round 3 explicitly verified as not vulnerable
The H1 generation mechanism itself (no double-count, no stale re-match, no
double-increment of pending); C1's `isUnreleasable` completeness for the
`approve` revert set; reentrancy; governance cannot bypass the attestor quorum
or move the immutable sweep timestamp; deploy-script role wiring against the
actual vendored OZ v5.4.0 `TimelockController`; unbounded-`_approvers`
gas-griefing (not practically reachable).

## Round 4 (2026-07-08, resumed first reviewer, full-diff pass incl. the portal UI)

Verified all eight round-1/2/3 fixes are correctly implemented and cleared the
portal migration UI (EIP-55, payload-hash parity, finality-gated submission).
Two novel findings, both in the same class as prior rounds — a threshold
crossed outside `approve()`, and the cap backstop:

### H1(r4). HIGH — `setThreshold` decrease could retroactively strand a payload
Lowering the threshold can make a sub-threshold payload releasable without
routing through `approve()`, so its amount is never added to
`pendingApprovedAmount`; a later `sweepRemainder` could then sweep it, and its
`release()` reverts `InsufficientVaultBalance` until governance refunds.

**Resolution (fixed):** `setThreshold` records the time of any decrease;
`sweepRemainder` is blocked for `SWEEP_SETTLING_PERIOD` (7 days) afterwards,
giving the monitor and a permissionless `release()` time to settle any
newly-qualifying payload first. Runbook RB-6 updated. Recoverable and
detectable even absent the guard (RB-7 reconciliation shows the shortfall).
Covered by `test_ThresholdCutBlocksSweepDuringSettling`.

### H2(r4). HIGH — Daily cap was a fixed calendar-day bucket, not a rolling window
The `currentDay` bucket reset to zero at the UTC boundary, letting a
compromised quorum release `dailyCap` at 23:59 and again at 00:00 — 2× the
intended blast-radius bound (PRD V4 specifies a *rolling* 24h maximum).

**Resolution (fixed):** replaced with a leaky-bucket rolling limiter —
`dailyCap` capacity refilling linearly at `dailyCap`/day
(`availableDailyAllowance()`), so a burst is capped at `dailyCap` and a second
burst must wait ~24h for the bucket to refill. No instant reset at any
boundary. Covered by `test_DailyCapRefillsGraduallyOverRollingWindow` and
`test_DailyCapHasNoInstantResetAtBoundary`. Residual: over a *rolling* 24h a
full bucket plus full refill still totals up to ~2× `dailyCap`, but spread over
24h rather than instantaneous — size `dailyCap` accordingly.

## Round 5 (2026-07-09, in-depth review focused on bricking the pipeline)

Adversarial pass over the whole stack asking specifically where an outsider
could brick releases. The on-chain fund path (rounds 1–4) held up; the finding
was in the off-chain monitor.

### H1(r5). HIGH — A dust PEN transfer to the vault permanently tripped the monitor's M2b check
The M2b conservation check used strict equality
(`balance + totalReleased + totalSwept != totalSupply`). Under all legitimate
contract logic that sum is *exactly* `totalSupply`, so equality could only ever
break *upward* — via tokens arriving in the vault outside the release path.
Anyone could do that permissionlessly: `PEN.transfer(vault, 1 wei)`, or a
`migrate(_, <vault address>)` whose 3rd approval self-transfers into the vault.
The break is permanent (the surplus can only leave via the post-window
`sweepRemainder`), so every poll re-fired the highest-severity alert and —
with `GUARDIAN_PRIVATE_KEY` set — re-paused the vault every cycle, wedging all
releases for the rest of the window while burns kept accruing on Pendulum.

**Resolution (fixed):**
- M2b now alerts only on a **deficit** (`balance + released + swept <
  totalSupply`); a surplus is ignored. A deficit is the only direction that can
  signal real loss (a genuine unauthorized release keeps the sum equal and is
  caught by M2a). Alert renamed `VAULT BALANCE DEFICIT`.
- Defense in depth on-chain: `MigrationVault.approve` rejects `recipient ==
  address(this)` (`RecipientIsVault`), closing the self-migration variant at the
  single point approvals are recorded.
- The conservation/liveness predicates were extracted to `monitor/src/checks.ts`
  and unit-tested (`checks.test.ts`): surplus-does-not-fire, deficit-fires,
  exact-holds. Vault side covered by `test_ApproveRejectsVaultRecipient` and
  `test_VaultRecipientNeverReleasesAndPreservesInvariant`.

### M1(r5). MEDIUM — Monitor liveness scan could starve the conservation checks
M4 read `nonceConsumed` one nonce at a time, sequentially, every poll. During a
pause or cap-deferral every migration stays unconsumed, so the scan grew with
the backlog and could push a cycle past the poll interval — starving the M2a/M2b
checks exactly when they matter most. **Fixed:** per-nonce reads are batched
through Multicall3.

### L1(r5). LOW — M2a read ordering made safe by construction, not just by latency
The monitor read Pendulum `totalMigrated` before the Base totals. A burn
finalizing between the two reads and released before the Base read could momentarily
show `released > migrated`. It was unreachable in practice (release latency ≫ the
read gap) but is now removed outright: Base is read first, then the
monotonically-growing `totalMigrated` at a strictly-later snapshot, so M2a cannot
false-positive on an in-flight burn.

## Follow-ups for the external audit
- These round-4 fixes touch the fund-release path and have **not** had a
  subsequent internal round; they are the first thing the external audit should
  re-derive. Four internal rounds have each found an issue (twice in a prior
  round's own fix) — continued internal iteration shows diminishing returns
  against real external review.
- The attestor's positional event decode is shape-checked but still assumes
  field order; re-verify against metadata after any runtime upgrade (RB-5).
