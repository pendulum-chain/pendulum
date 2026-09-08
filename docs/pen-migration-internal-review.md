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

## Post-drills review pass (2026-08-31)

Scope: the fund-release-path changes made since the rehearsal findings (the
race-recheck backoff and the transient-RPC classification), per the standing
practice, plus the drill scripts themselves.

### L1(drills). LOW — Status codes matched as bare substrings

The transient-error classifiers matched `502|503|504` anywhere in the error
text, and error messages embed transaction parameters — an amount containing
`502` could misclassify a genuine failure as transient. The consequence was a
noisy stall (alert every head, checkpoint frozen, liveness alert eventually)
rather than anything silent, but it is now anchored on word boundaries, and
`429` was added for endpoints that return only the numeric code. **Fixed.**

### Verified, no change needed

- The monitor and releaser were checked for the attestor's die-on-transient
  defect and do not have it: both wrap their poll/cycle bodies in a
  loop-level catch that alerts and continues, and only startup failures exit.
- The recheck backoff cannot convert a permanent failure into a silent skip:
  it returns true only on positive on-chain confirmation, and errors inside
  the recheck retry rather than resolve.

### Drill outcomes (phase 6)

All seven runbooks are now rehearsed: `drills.mjs` 13/13 on the Sepolia stack
(RB-1, RB-3 surplus, RB-4, RB-6, RB-7; RB-2 in phase 5; RB-3 deficit in
phase 3), and `drill-rb5-upgrade.mjs` 5/5 — the actual spec-26 upgrade enacted
on a fork of pre-upgrade mainnet under a live fleet, which also proved the
referendum's version bump compiles. The RB-5 script's header records the
Chopsticks limitations that shaped it; the significant one is that a
post-upgrade extrinsic cannot be submitted through Chopsticks at all, because
it serves pre-fork metadata even across `--resume` while executing the new
runtime, which rejects the stale-metadata signature as `badProof`.

## Round 8 (2026-08-31, independent reviewer over the round-7+ commits and the uncommitted durable-state/finality work)

Scope per the round-8 handover: the six attestor/releaser commits since round
7, the uncommitted working-tree changes (durable state in all three daemons,
the monitor's per-tuple reconciliation, Base-finality gating, the pallet's
one-time treasury anchor), governance wiring, runtime wiring, and the portal.
No fund-loss bug was found; the on-chain invariants of rounds 1–7 held under
re-derivation. The significant findings were all in the newest, uncommitted
daemon code — and notably in code that had **never run under real Base
safe-lag**: Anvil serves `safe == latest`, so the phase-3 harness exercises
the finality gating as a no-op, and the 2026-08-28 Sepolia rehearsal predates
it.

### H1(r8). HIGH — Finality gating made every lost race an alert storm and serialized fleet throughput
The uncommitted change pointed the benign-race recheck (`alreadyHandled`/
`alreadyHandledSettled`) at the Base `safe` block and put a
`waitForBaseFinality` inline in `approve()`. Consequences: (a) the guaranteed
per-migration race loser saw "not handled" at `safe` for the whole safe-lag
(minutes), fell through to a synthetic "pending finality" transient error, and
re-alerted on every finalized head until `safe` caught up — an alert storm on
the *normal* k-of-n race, the fifth instance of the round-1 class; (b) each
winner blocked serially on its own transaction reaching `safe` before touching
the next event, collapsing per-attestor throughput to roughly one migration
per Base batch interval — hours of backlog under launch-day load, with the
monitor's liveness grace then paging for everything queued.

**Resolution (fixed, restructured):** processing and durability are decoupled.
Blocks are processed strictly in order against **latest** Base state (lost
races conclude benign from latest, as the rehearsal fix intended), while the
durable checkpoint trails separately: it advances past a block only once every
releasable event in it reads as handled at the `safe`/`finalized` boundary
(`advanceCheckpoint` in `attestor/src/main.ts`). Event-less blocks checkpoint
with zero Base reads. A block whose approvals refuse to settle (reorged away
with nothing replacing them) is re-approved idempotently after
`BASE_FINALITY_TIMEOUT_MS` with a single alert — which also makes reorg
recovery automatic where the previous design needed a restart. Crash recovery
re-processes exactly the non-durable suffix. Transient alerts are throttled to
one per minute (everything still logged).

### M1(r8). MEDIUM — Unmatched-event grace could false-pause on a stalled source node, and hid a real attack for its duration
Two failure modes of one mechanism: (a) a monitor whose own Pendulum node
served a live-but-stalled finalized view for longer than
`UNMATCHED_EVENT_GRACE_SECONDS` (default 600s) — a stuck peer set or a slow
restart-sync — turned every fresh legitimate approval into `MIGRATION TUPLE
VIOLATION` and auto-paused the vault (post-handover unpause: quorum + ≥48h
timelock); (b) during the grace the monitor only *logged*, and `awaitingSource`
returned before the M2a aggregate check, so a genuinely fabricated tuple
(compromised quorum, high nonce) got up to 600s of cap-bounded releases with
zero paging — a ~10× dwell regression versus the pre-rewrite monitor.

**Resolution (fixed):** three changes in `monitor/src/`. (1) The first sighting
of an unmatched event **pages immediately** (`UNVERIFIED BASE EVENT`,
deduplicated through the persisted first-seen map), so operators get the whole
grace window. (2) A **fabrication proof** short-circuits the grace: a burn is
relay-finalized strictly before any attestor approves, and Substrate
timestamps are monotone — so once the finalized source view's timestamp passes
the Base event's own block timestamp (plus `SOURCE_CLOCK_SKEW_MARGIN_SECONDS`,
default 120s) and the nonce still does not exist, the event is provably
fabricated and pauses at once (`provablyUnsourced` in `checks.ts`, unit
tested). A merely-lagging source can never satisfy the proof, so node lag
cannot fast-path a false pause. (3) A finalized source view older than
`SOURCE_STALE_ALERT_SECONDS` (default 300s) pages `PENDULUM SOURCE VIEW
STALLED` on its own — before any grace deadline can force a cannot-verify
pause. The grace-expiry pause itself is retained deliberately: a monitor blind
past its grace with unverified releases flowing is a pause-worthy state, and it
now arrives announced.

### M2(r8). MEDIUM — Full-supply quorum denominator could deadlock governance into a permanent pause
`PENGovernor` quorum was a fraction of *total* past supply — including the
vault's unmigrated ~150M — while only migrated-and-delegated PEN can vote.
Post-handover, a pause (guardian key compromise, monitor false positive, or a
legitimate incident) with quorum unreachable would be **permanent**: unpause,
`setCaps`, `clearStalePending` and `sweepRemainder` are all admin actions
behind the timelock, the paused vault freezes releases, so circulating voting
supply can never grow to quorum — a self-locking deadlock with no alternate
admin path. (Mitigating: the handover-acceptance proposal itself proves quorum
once; the deadlock needed participation to decay afterwards.)

**Resolution (fixed, design change — needs explicit sign-off):** quorum now
tracks **circulating** supply. `MigrationVault.setToken` one-time-delegates the
vault's balance to a constant dead-address vote sink
(`MigrationVault.VOTE_SINK` == `PENGovernor.QUORUM_SINK`, lockstep asserted in
tests), which checkpoints the unmigrated supply in the token's vote history;
`PENGovernor.quorum()` subtracts the sink's past votes from the denominator and
applies an absolute `quorumFloor` (new constructor/deploy parameter,
`QUORUM_FLOOR`) so early proposals are not trivially cheap. Third parties
delegating to the sink only forfeit their own voting power (strictly dominated
by voting), so the mechanism is not abusable to raise or unfairly drop quorum
below the floor. Release gas grows ~10k for the sink checkpoint update.
Covered by `test_QuorumTracksCirculatingSupplyWithFloor`,
`test_QuorumSinkMatchesVaultVoteSink`, `test_SetTokenParksVaultVotesInSink`.
Handover should still be gated on measured delegated voting power
comfortably exceeding `max(floor, fraction × circulating)`.

### M3(r8). MEDIUM — Daemon catch-up requires archive state; a >51-minute outage wedged them on default-pruned nodes
Monitor and attestor catch up block by block via `api.at()` +
`system.events()`, which needs per-block state; a default Substrate node prunes
state to 256 blocks (~51 min). Any daemon outage past the horizon wedged the
restart loudly but indefinitely, and the (correct) "never edit the state file"
rule left no sanctioned recovery. **Resolution (documented):** both READMEs now
require `--state-pruning archive` (or `archive-canonical`) and name the
recovery path — point `PENDULUM_WS` at an archive node, never edit state.

### L1(r8). LOW — Load-balanced `eth_getLogs` could silently truncate a scan range
A pool node behind the `safe` head can serve a truncated log range without
erroring on some providers, letting the monitor/releaser cursor advance past
events never seen (stale pending entries, lost per-tuple evidence; aggregates
unaffected). **Resolution (fixed):** both daemons probe each range's end block
(`getBlock({blockNumber: end})`) before scanning — a lagging node now fails the
cycle loudly and the range replays — and both READMEs recommend a single
dedicated Base endpoint.

### L2(r8). LOW — Releaser re-paged governance-blocked releases every poll
`ExceedsPerReleaseCap`/`InsufficientVaultBalance` need a ≥48h timelocked action
to clear but alerted every 60s (~2,900 identical pages per timelock period).
**Resolution (fixed):** per-nonce throttling (`BLOCKED_ALERT_INTERVAL_MS`,
default 6h), cleared when the nonce resolves.

### L3(r8). LOW — Bare numeric status codes in the transient classifier collided with nonce labels
The drills-round word-boundary fix did not cover `nonce=429` (and 502/503/504)
embedded in error labels: a genuine failure at those nonces classified as
transient — an infinite noisy retry instead of the PRD-A5 exit. **Resolution
(fixed):** classification is structural first — `HttpRequestError.status`,
`TimeoutError`, socket error codes, walked through the error `cause` chain —
with a word-only text fallback (bare digit patterns removed entirely, WebSocket
disconnect phrasing added). Extracted to `attestor/src/checks.ts` and unit
tested, including the `nonce=429` regression. The releaser's synthetic
"PendingFinality" message channel was removed along with the inline waits.

### L4(r8). LOW — Config footguns
`BASE_FINALITY_TAG=finalized` with the 15-minute default timeout guaranteed at
least one spurious timeout per approval; unvalidated numeric envs became NaN
silently (turning backoffs into busy-loops or disabling graces). **Resolution
(fixed):** all numeric envs are validated at startup across the three daemons,
and finality-related defaults scale with the chosen tag (45 min for
`finalized`). The releaser's inline finality wait was removed outright: a
successful `release()` now leaves the durable pending set only when
`pruneConsumed` sees the nonce consumed at the boundary, which is both durable
against reorgs and stall-free.

### Noted, not fixed (INFO)
Portal: a daily-cap-deferred release shows "waiting for approvals (3/3) …
normally a few minutes" indefinitely (detect `approvals ≥ threshold &&
!released` and say it is queued behind the cap); migrate-entire-balance can
fail post-fee (`InsufficientBalance`/`WouldLeaveDust`) because the max button
uses the display float and fees are withdrawn first. Repo hygiene: ~420MB of
untracked local artifacts at the repo root are one `git add .` from landing in
the PR. `spec_version` is still 25 at HEAD; the bump to 26 stays on the
release checklist.

### Round 8 explicitly verified as not vulnerable
The one-time treasury-destination anchor (origin-gated, zero-address-checked,
tested; root can still reset it via `killStorage`, an accepted bar-raise from
council-majority to root); treasury migrations share the `MigrationInitiated`
event and nonce sequence, so the monitor's contiguous-nonce cursor handles
them; pallet burn atomicity, lock/vesting enforcement, dust/ED, KeepAlive and
the paused-by-default storage default (the Executive migration tuple touches
only xcmp-queue/identity); the `isUnreleasable` ⇄ `approve()` input-revert
lockstep (unchanged, three conditions); generation accounting cannot
double-add `pendingApprovedAmount` across remove/re-add/re-approve; the leaky
bucket under `setCaps` changes; ABI/event parity across vault, attestor,
monitor, releaser and portal (indexing verified field by field); payload-hash
parity incl. the portal's manual `abi.encode` mirror; monitor draft/persist
crash consistency and M2a read ordering; deploy-script role wiring; the vault's
PEN cannot vote (and after r8 is provably parked at the sink).

### Round 8 fix verification
All local suites pass post-fix: pallet 22, Foundry 40 (3 new), attestor 14
(6 new classifier tests), monitor 14 (1 new), releaser 11. The attestor/
monitor/releaser changes are on the fund-release path: per the standing
practice they need a fresh adversarial pass, and phases 3/5/6 must be re-run —
phase 3 cannot exercise the finality trailing (Anvil's `safe == latest`), so
the Sepolia rehearsal is the first environment where the new checkpoint
behavior actually runs under real safe-lag. The M2(r8) governance change
(circulating-supply quorum + floor) alters deployed-parameter semantics and
needs an explicit team decision on `QUORUM_FRACTION`/`QUORUM_FLOOR` values
before phase 5b re-runs.

## Round 9 (2026-09-05, multi-agent audit over the round-8 tree; fresh angles)

**Method.** Ten independent finder agents, one lens each — an adversarial pass
over the round-8 fixes, daemon concurrency, fresh-eyes Solidity, and seven
angles no earlier round had taken (economic/MEV, substrate-runtime
interactions, supply chain, secrets/ops hygiene, portal web security,
cross-component drift, test gaps) — followed by dedup against this log, two
adversarial refuters per finding (code-truth and exploitability), a
completeness critic that added four lenses (source-chain inflation, release
artifact drift, governance-capture economics, runbook drift), and a second
targeted round. 37 agents, 45 raw findings; the funnel's own triage was then
re-judged by hand, because it had dropped several material items. Three
findings are defects in round-8 fixes, which is exactly what the standing
practice predicted.

### H1(r9). HIGH — Nobody could stop a passed hostile proposal during the timelock delay
`DeployGovernance.s.sol` granted `CANCELLER_ROLE` only to the Governor, and
OZ Governor lets only the proposer cancel, only before voting starts. So once
a proposal with quorum-clearing stake passed, the 48h delay was not a
reaction window: one executor transaction could batch `unpause`,
`addAttestor`×3, an unbounded `setCaps`, and three fabricated approvals — the
guardian's pause (the PRD's stated mitigation for this threat) is undone
inside the same batch. **Resolution (fixed, decision pending):** the script
takes an optional `TIMELOCK_CANCELLER` (intended: the guardian or council
Safe) and grants it `CANCELLER_ROLE`; `test_CancellerCanVetoAQueuedProposalDuringTheDelay`
proves the veto against a queued proposal. **The team must set that address
at phase 5** — the env example now says so.

### H2(r9). HIGH — Governance capture is cheap by construction; no quorum floor prices it out
Quorum stake is bought, not burned, and the prize is the unmigrated vault.
Round 8's circulating-supply quorum was necessary (a full-supply denominator
deadlocks unpause) but makes early capture cheaper still; no floor value
reconciles "honestly reachable soon after launch" with "unprofitable to
capture". No production `QUORUM_FLOOR` was decided anywhere, the env example
still described a total-supply quorum, and the only in-repo value was the
rehearsal's 0. **Resolution:** capture resistance is re-assigned explicitly to
H1's canceller, the guardian pause and the vault caps — not to the floor —
and `contracts/.env.example` now records both parameters as decisions to make
before phase 5. Also noted: the round-8 handover gate ("measured delegated
voting power exceeds quorum") is satisfiable by an attacker's own stake —
measure *diverse* participation.

### H3(r9). HIGH — Minted-then-burned PEN drains the vault with every check satisfied
A passed Pendulum referendum (1 PEN deposit, `EnsureSigned` submission, root
enactment — the exact vector of the real July 2026 proposal #2) or an
unexpected teleport-in mints PEN; migrating it is a genuine finalized burn,
honestly attested, tuple-matched, and within M2a/M2b — while the immutable
150M vault drains ahead of late honest migrators. No component read Pendulum
issuance; round 7 had cleared only staking inflation. **Resolution (fixed,
alert-only):** the monitor anchors `totalIssuance + TotalMigrated` on first
observation (persisted; that sum cannot grow under any legitimate flow, since
teleport-in can only restore what teleport-out burned) and pages
`SOURCE SUPPLY GREW` on growth beyond `ISSUANCE_TOLERANCE`; RB-3 gained the
response (pause the *pallet*). Pausing is deliberately a human decision. The
governance-side mitigation — council/technical-committee vigilance on opaque
preimage proposals for the window's duration — is a Pendulum governance
matter recorded here as an open decision.

### M1(r9). MEDIUM — Quorum-approved-but-unreleased migrations were silent everywhere
Three causes, one symptom. (a) Nothing enforced `perReleaseCap ≤ dailyCap`, so
an amount in between passes the per-release check but can never fit the daily
allowance — the rehearsal config itself had that shape (50,000 > 28,800).
(b) The leaky bucket is first-come-first-served with no reservation, so
sustained small self-migrations can starve a large deferred release at
near-zero net cost. (c) A threshold cut can make a payload releasable without
a `ReleasePending`, invisible to the releaser. In all three the releaser
retried quietly, the monitor's liveness loop skipped anything at quorum, and
the portal said "a few minutes". **Resolution (fixed):** the vault enforces
`perReleaseCap ≤ dailyCap` (`CapsInverted`) in the constructor and `setCaps`;
the monitor pages `LIVENESS: quorum reached but not released` after the grace;
the releaser classifies an amount above `dailyCap` itself as blocked
(`ExceedsDailyCapPermanently`) rather than a refill wait; RB-6 step 4 no
longer claims a listing that did not exist. Residual: (b) is now *visible*,
not prevented — a reservation scheme is a vault design change, recorded as a
decision.

### M2(r9). MEDIUM — Round 8's fabrication proof was unsound under Base timestamp lag
The proof anchored on the Base block timestamp of the unmatched event. OP-stack
L2 timestamps trail real time by the length of a sequencer outage while it
catches up, so after such an outage a merely-lagging monitor node could
"prove" a legitimate event fabricated and pause without grace. **Resolution
(fixed):** `provablyUnsourced` anchors on the monitor's own first-observation
time (the burn is finalized before the approval can be observed at all), and a
future-dated source view (collator clock ahead) proves nothing. Regression
tests cover both; the Base-timestamp fetch is gone.

### M3(r9). MEDIUM — Monitor catch-up was all-or-nothing per cycle
After a long outage the whole replay ran inside one `check()` and persisted
only at the end; one transient RPC error discarded hours of progress, and a
flaky endpoint could keep it blind indefinitely. **Resolution (fixed):** the
Pendulum-side cursor (self-consistent on its own; the Base cursor only ever
trails it) is checkpointed every 200 blocks during ingest.

### M4(r9). MEDIUM — `setCaps(type(uint256).max, …)` would brick approve() and release()
`_decayedConsumed` multiplies elapsed seconds by `dailyCap` under checked
arithmetic; an "uncap" overflowed it, reverting every threshold-crossing
`approve()` (a deterministic revert outside `isUnreleasable`, i.e. the
fleet-halt class) until a second timelocked `setCaps`. **Resolution (fixed):**
`MAX_DAILY_CAP = 2^128` enforced with the caps invariant; tested at the bound.

### Lower-severity, all fixed
- Releaser: a `NonceAlreadyConsumed` decoded from a *latest*-state simulation
  could delete a pending entry before consumption was confirmed at the
  boundary (round-8 regression) — now classified as pending finality; a mined
  revert (no revert data) no longer pages as "unexpected" but is re-evaluated
  next cycle.
- Monitor: `securityViolation` awaited the alert webhook *before* pausing, with
  no timeout — now pauses first; every daemon's webhook call has a 10s
  timeout and redacts URLs (viem error texts quote the RPC endpoint, which
  commonly embeds an API key).
- Attestor: a push-driven loop with no watchdog idled silently when the node
  stopped finalizing — `HEAD_STALL_ALERT_MS` watchdog added; state reads at
  the `safe` block outside a node's retained window (`missing trie node`
  while the batcher lags) are classified transient instead of fatal.
- Monitor: a burn to the zero/vault address kept the pending count non-zero
  forever (paging liveness every grace period and making RB-7's precondition
  unsatisfiable) — paged once as `CRITICAL: unreleasable migration burned`
  and excluded from reconciliation; RB-7 step 3 accounts for them.
- Vault constructor rejects an `earliestSweepTimestamp` in the past.
- The RB-6 drill rewound a checkpoint by writing the legacy shape the round-8
  loader rejects — fixed; the sanctioned rewind procedure is now in RB-6.
- Runbooks re-aligned with post-round-8 behaviour: alert vocabulary table,
  RB-1 trigger, RB-2 recovery (archive node, never edit the checkpoint,
  `START_BLOCK` applies only without a checkpoint), RB-3 step 6, RB-6 steps 3–4.

### Noted, not fixed — decisions and other repos
- **Spam economics (MEDIUM):** the 100 PEN minimum is retained capital, not a
  cost — a self-migration returns it on Base — so spam is bounded only by the
  spammer's Pendulum holdings and the daily cap, which the spam then consumes
  (feeding M1(b)). Options: a burned-only fee on `migrate`, a per-account rate
  limit, or accepting it with the monitor's new paging. Decision.
- **Portal (separate repo):** `NumericInput` paste strips commas as thousands
  separators, so a pasted `500,5` migrates 5005 PEN (to the user's own Base
  address — not lost, but 10× and irreversible; the same paste path predates
  the numora switch); the smart-contract and above-cap warnings fail open
  while their RPC reads are loading or failed; confirmation checkboxes do not
  reset when the address or amount changes; the headline copy says "3-of-5"
  where the deployed set is 3-of-4.
- **Process:** no CI job runs the Foundry or daemon suites; testing scripts
  pass the deployer key on the forge argv (throwaway keys — use `--account`/a
  keystore for production deploys); phase 4's header claims to prove attestor
  finality gating but starts no attestor; the phase-3 fabricated-approval
  drill passes through the grace-expiry path (grace 0), never the proof path.

### Round 9 explicitly refuted
The live PEN teleport channel does not invalidate `MAX_ISSUANCE` (teleports are
supply-conserving; 150M was rounded up from ~149.93M live issuance, and
AssetHub's PEN can only originate from Pendulum burns). A compromised quorum
cannot permanently brick `sweepRemainder`: the fabricated nonce is consumable
by the rotated honest quorum, after which `clearStalePending` clears it (its
documented purpose, tested).

### Round 9 fix verification
Local suites after the fixes: pallet 22, Foundry 44 (4 new), attestor 14,
monitor 16 (2 new), releaser 12 (1 new). The same standing practice applies
as after round 8 — these changes touch the vault (caps invariants, sweep
guard), the governance deployment, and all three daemons, so they need a
fresh adversarial pass and the Sepolia re-run, with `REHEARSAL_PER_RELEASE_CAP_PEN`
now equal to the daily cap and `TIMELOCK_CANCELLER`/`QUORUM_FLOOR` set.

### Phase 3 re-run on the round-9 revision (2026-09-06)

Phase 1 passed 11/11 on the first attempt. Phase 3 (Chopsticks mainnet fork
with the runtime wasm override + Anvil; four attestors, monitor, releaser)
reached 9/9 after three findings that only a live fleet could produce:

- **Every attestor exited on its first checkpoint** — silently, because the
  harness kept daemon output only in memory. Cause: the durability read at
  the `safe` block hit a block that predates the vault. Anvil resolves
  `safe`/`finalized` to genesis until the chain is 32 blocks deep (the round-8
  claim "Anvil's safe equals latest" was wrong — it holds only with
  `--slots-in-an-epoch 0`, which the harness now requires and documents). The
  attestor now treats a zero-data read at the boundary as "not yet durable"
  rather than fatal, which is also the right behaviour for a fresh testnet
  deployment whose safe head still trails the vault. Daemon output is teed to
  `testing/.logs/`.
- **`setCaps(perRelease > daily)` in the cap-deferral drill** now reverts
  `CapsInverted`, as intended; the drill uses equal caps.
- **An exact gas estimate ran out of gas inside the ERC20Votes checkpoint
  write.** The vault's vote-sink delegation (round 8) makes every transfer
  touching the vault write a checkpoint keyed by block timestamp; an estimate
  computed in one second (overwrite) undershoots execution in the next (new
  entry). The harness passes an explicit gas limit, and the releaser now
  doubles its `release()` estimate like the attestor does for `approve()`.
  On Base the estimate runs at the pending block's timestamp and is
  conservative, but the buffer costs nothing.

Note for later drills: `--slots-in-an-epoch 1` makes Anvil trail `safe` by one
block, which would exercise the checkpoint trailing locally; the drills would
then need to mine an extra block after each action.

### Spec-26 runtime: phase 2 and RB-5 (2026-09-08)

The runtime was built from the committed tree with `spec_version` 26
(`transaction_version` unchanged at 11: no existing extrinsic encoding
changes). Local artifact `pendulum_runtime.compact.compressed.wasm`, 2,206,797
bytes, sha256 `8e82e2e8f0e17eb2174aa114cef2b999e52b905bb6e87b51a0aae8e72100c782`
— the reproducible build from the release pipeline is the artifact to submit;
compare hashes. Against a fresh Chopsticks fork of live mainnet with that wasm
as override, the fork reports spec 26 and **phase 2 passed 14/14**, including
ships-paused with no storage written. **RB-5 passed 5/5**: the wasm written to
`:code` under a running four-attestor fleet on a pristine spec-25 fork; every
attestor decoded post-upgrade blocks without a restart and rode a node
restart. Dev-machine benchmark weights were accepted as production weights.

## Residual risks and standing practices (no external audit — risk accepted)
- Every change to the fund-release path (vault release/approve/sweep logic,
  pallet burn path) gets a fresh independent adversarial review round before
  deployment — rounds 5–7 re-derived the round-4 fixes and the full outsider
  surface, and this practice replaces the external-audit backstop.
- The attestor's positional event decode is shape-checked but still assumes
  field order; re-verify against metadata after any runtime upgrade (RB-5).
