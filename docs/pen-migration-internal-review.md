# PEN Migration — Internal Security Review (running log)

**Date:** 2026-07-07
**Scope:** MigrationVault.sol, PEN.sol, PENGovernor.sol, deploy scripts,
token-migration pallet, attestor daemon, invariant monitor.
**Method:** adversarial review by an independent reviewer agent against the
PRD requirements (P1–P9, V1–V9, A1–A5, M1–M4), cross-checked against the test
suite. This log is the project's security-assurance record (PRD §9): **no
external audit is commissioned** — the residual risk is consciously accepted
and carried by the threat-model mitigations (PRD §8) and the review rounds
recorded here.

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

## Round 6 (2026-07-09, review focused on outsider bricking of the off-chain fleet)

The on-chain fund path (rounds 1–5) held up. The headline finding is the
round-5 fix re-opening the round-2 DoS class one layer out, in the attestor.

### C1(r6). CRITICAL — A 1-PEN migration to the vault address crash-loops the whole attestor fleet
Round 5 made `MigrationVault.approve` revert `RecipientIsVault` on a
vault-recipient tuple (closing the monitor surplus-wedge). But the attestor's
`isUnreleasable` gate — which skips permanently-reverting tuples so the fleet
never crash-loops on them — was not updated in lockstep: it flagged only the
zero address and zero amount. The pallet cannot reject the vault address (it has
no knowledge of Base state), so `migrate(1 PEN, <vault address>)` reaches every
attestor as a well-formed event, its `approve` reverts deterministically, the
daemon treats the revert as fatal and exits, and — the checkpoint never having
advanced past the block — reprocesses the same block on restart, forever. All
five attestors hit the same finalized block and crash-loop together, halting
every migration behind it for the price of one 1-PEN transaction. Same class as
round-2 C1 (zero-address DoS).

**Resolution (fixed):** `isUnreleasable` now also flags `recipient ==
vaultAddress` (case-insensitively), so a vault-recipient event is skipped with a
distinct CRITICAL alert instead of crash-looping — the vault-aware attestor is
the *only* component that can defend against this, since the pallet can't see
the Base address and the portal check is bypassable by calling the extrinsic
directly. The predicate was extracted to `attestor/src/checks.ts` and
unit-tested (`attestor/src/checks.test.ts`, mirroring the monitor's `checks.ts`);
the attestor previously had no unit tests, which is why the coupling gap slipped
through. **Architectural note for all future review rounds:** the set of deterministic
`approve` reverts and the attestor's `isUnreleasable` set must stay in exact
lockstep — this is the third round a vault-side "reject bad tuple" change
reopened a fleet-crash hole. A shared, tested enumeration of the reverting
preconditions would end this recurrence.

### M1(r6). MEDIUM — Minimum migration amount was below the fleet's per-migration gas cost
`MinimumMigrationAmount` was 1 PEN (~$0.0086 at $0.00858/PEN), below the ~3 Base
`approve` txs (~$0.01–$1 depending on Base gas) the fleet spends per migration,
making dust-spam a cheap asymmetric gas-drain grief on all five operators.
**Fixed:** raised to 100 PEN (~$0.86), which dominates fleet gas cost across
normal Base conditions while staying negligible for real holders. Tunable via
runtime upgrade; revisit if the PEN price or Base gas regime shifts materially.

### L1(r6). LOW — Monitor re-fired liveness alerts every poll for a stalled or unreleasable nonce
The M4 liveness check paged for every past-grace unconsumed nonce on every poll,
so a pause backlog — or a burn to a structurally-unreleasable address (zero or
the vault, which never consumes) — produced an unbounded alert storm, training
on-call to ignore the highest-severity page. **Fixed:** liveness re-alerts are
throttled to at most once per grace period per nonce (cleared on consumption),
preserving outage visibility without the storm.

### Round 6 explicitly verified as not vulnerable
Monitor auto-pause cannot be weaponised by an outsider (M2b fires only on a
deficit, unreachable without stealing from the vault; M2a can't false-positive
given the Base-first read ordering); the vault's opportunistic-release path
defers rather than reverts on pause/caps/insufficient-balance, so only a
vault-side *input* revert (now fully enumerated in `isUnreleasable`) can reach
the attestor's fatal path; `pendingApprovedAmount` is not inflatable by an
outsider (pending entries require three real attestations of real burns);
`migrate` correctly refuses locked/staked/vesting balance and the dust/ED
remainder check is sound.

## Round 7 (2026-07-09, review focused on outsider exploit/brick across the full stack)

The on-chain fund path (rounds 1–6) held up under an independent re-derivation.
The core invariant — a release threshold can only ever be crossed inside
`approve()` (the sole exception, a `setThreshold` *decrease*, is governance-gated
and settling-period-guarded) — was re-confirmed, so `pendingApprovedAmount` is a
complete reservation against `sweepRemainder` and no outsider can strand or
double-release. The `isUnreleasable` ⇄ `approve()` revert lockstep is currently
consistent (zero recipient / vault recipient / zero amount; the nonce-consumed
and already-approved reverts are absorbed by the daemon's `alreadyHandled`
re-check). The one novel finding is off-chain, in the monitor.

### M1(r7). MEDIUM — Monitor liveness scan re-read every nonce ever created, every poll
The round-5 fix batched the per-nonce `nonceConsumed` reads through Multicall3 to
stop the liveness scan from outrunning the poll interval. But the scan still
rebuilt its working set from scratch each poll: `for (nonce = 0; nonce <
nextNonce) if (!nonceFirstSeen.has(nonce)) set(nonce, now)` re-added *every* nonce
not currently in the map — including nonces already consumed and pruned. So each
poll re-inserted and re-read all consumed nonces via Multicall, making the scan
O(all migrations ever created) rather than O(pending backlog) and growing without
bound for the whole migration window. At high migration volume this lengthens each
check cycle (so the M2a/M2b conservation checks, which run first, fire less often)
and eventually risks the liveness Multicall failing outright. It is not a
fund-loss or correctness bug — genuinely-pending nonces keep their original
first-seen time, so no missed or false liveness alerts — but it silently negated a
fix the team believed was in place, worst exactly as the migration succeeds.

**Resolution (fixed):** the monitor tracks a high-water mark
(`nextNonceIncorporated`) and stamps only nonces in `[nextNonceIncorporated,
nextNonce)` each poll (`newNonces` in `monitor/src/checks.ts`), so a
consumed-and-pruned nonce is never re-added and the scan is O(pending backlog) as
round 5 intended. Covered by a new `checks.test.ts` regression asserting a
consumed nonce does not reappear on the following poll.

### C1(r7). LOW — A migration above the per-release cap looks stuck to the user
A single migration whose released amount exceeds `perReleaseCap` is burned on
Pendulum and reaches quorum, but its release defers (marked pending) until
governance raises the cap — recoverable, yet potentially a long, opaque wait. The
pallet cannot bound this (it has no knowledge of the Base-side cap).
**Resolution (fixed):** the portal migration page reads the vault's live
`perReleaseCap` and, before submission, warns that an above-cap amount will be
held until governance raises the cap, requiring an explicit confirmation and
recommending the user split into sub-cap migrations (`getPerReleaseCap` in
`src/helpers/ethereum.ts`; warning + confirmation checkbox in the migration page).

### Round 7 — deployment / ops notes (no code change)
- **Guardian slot vs. monitor auto-pause.** The vault has a single `guardian`
  address and `pause()` accepts only `guardian`/`admin`. PRD G5 wants the guardian
  to be a fast human Safe; PRD M3 wants the monitor to hold a guardian key for
  auto-pause. One slot cannot be both (a Safe can't be driven by the monitor's
  single EOA). Decide consciously at the key ceremony (D4). Post-handover
  consequence: a monitor-EOA guardian whose key leaks can pause, and `unpause` is
  then a ≥48h-timelocked governance action — a bounded but real griefing halt (the
  documented "guardian can at worst halt" trade-off).
- **Max issuance vs. staking inflation (PRD D3).** `MAX_ISSUANCE` is minted once
  and is immutable; if Pendulum total issuance ever grew past it (e.g. via staking
  inflation) during the window, late migrants could burn against a drained vault.
  Confirmed **non-applicable**: staking rewards/inflation are set to zero on-chain
  (`set_inflation`), so issuance is static — the genesis `InflationInfo` in
  `node/src/chain_spec.rs` is historical. Because the parameter is immutable,
  re-confirm at deploy time that `MAX_ISSUANCE` covers live issuance and that
  rewards remain zero for the window's duration.
- **Two-step admin handover window.** `Deploy.s.sol` calls
  `transferAdmin(adminSafe)`, but the deployer stays admin until the Safe calls
  `acceptAdmin()`. Correct (two-step prevents a wrong-address handover), yet the
  deployer key is a live admin during the gap — treat it as sensitive and complete
  `acceptAdmin` promptly.

### Round 7 explicitly verified as not vulnerable
Replay/double-release, conflicting-tuple non-merging, reentrancy (CEI + hook-free
token), attestor generations (no threshold crossing outside `approve()`),
`_approvers` bounded even under repeated rotation (`_inApprovers` dedup),
`palletAmount * conversionFactor` cannot overflow uint256, `clearStalePending`
underflow-safe and consumed-nonce-restricted; pallet burn atomicity, nonce
monotonicity/overflow guard, dust/ED check against the `withdraw(TRANSFER)` lock
enforcement, and the new `migrate_treasury`/`set_treasury_destination` pair
(origin-gated, `KeepAlive`, shared nonce space, zero-address rejection); governor
deploy-script role wiring (deployer admin renounced, executor = anyone,
self-administered timelock) and PEN↔Governor clock-mode consistency; monitor
auto-pause not weaponisable by an outsider (unchanged from round 6).

## Full-stack rehearsal on Base Sepolia (2026-08-28)

Two defects that no prior round could reach. Both are failures of an assumption
the local harness cannot violate: Anvil is a single node with instant inclusion,
so it always reads its own writes and never throttles. A public, load-balanced
endpoint does neither.

### C1(rehearsal). CRITICAL — An attestor exited on the ordinary k-of-n race

Losing the approval race is the most common event in this system: with four
attestors watching the same migration, three win and one loses every time. The
loser's transaction reverts, and the daemon re-reads the vault to confirm the
revert was benign before deciding what to do.

That confirming read hit a node that had not yet imported the block, reported
"not handled", and the benign race was escalated to a fatal exit. Verified
rather than inferred: replaying the reverted transaction one block earlier
succeeds, and it consumed 26k of 500k gas — an early custom-error revert, not
OutOfGas.

This is the same failure class as C1(r6) and the gas under-estimation before it,
reached by a third route. **Fixed:** the confirmation now re-checks with backoff
before concluding anything is wrong.

### C2(rehearsal). CRITICAL — Any transient RPC failure killed an attestor

The endpoint rate-limited the fleet and the attestor exited, because every
transport-level failure was handled identically to a decode failure.

The distinction matters. PRD A5 requires dying rather than silently skipping an
event, and a decode failure still does exactly that. But a rate limit or a
dropped socket carries no information about the event, and the checkpoint is
only advanced once a block is fully handled — so leaving the block unprocessed
is safe, and the next finalized head re-processes it. In production the previous
behaviour meant any momentary endpoint problem cost an attestor, and two such
blips put the fleet below quorum with releases stalling silently.

**Fixed:** transport failures are retried; decode failures remain fatal.

### Operational consequence (no code change)

Any procedure that writes and then immediately reads carries the same hazard
against a public RPC — RB-4 and RB-6 both do. Confirm state by re-reading until
it settles, not once. Each attestor should also run against its own node rather
than a shared public endpoint; the rehearsal reproduced the rate limit precisely
because six daemons shared one.

## Residual risks and standing practices (no external audit — risk accepted)
- Every change to the fund-release path (vault release/approve/sweep logic,
  pallet burn path) gets a fresh independent adversarial review round before
  deployment — rounds 5–7 re-derived the round-4 fixes and the full outsider
  surface, and this practice replaces the external-audit backstop.
- The attestor's positional event decode is shape-checked but still assumes
  field order; re-verify against metadata after any runtime upgrade (RB-5).
