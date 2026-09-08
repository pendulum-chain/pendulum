# PEN Migration — Operational Runbooks

Runbooks required by PRD acceptance criterion 10. Each must be drill-tested
before mainnet launch. Contact points, Safe addresses and paging channels are
filled in during the attestor onboarding ceremony (decision D4).

**Shared prerequisites:** access to the alerting channel; read access to a
Pendulum node and a Base RPC; the on-call sheet mapping attestor index →
operator → contact. Escalation path everywhere: on-call engineer → migration
tech lead → guardian Safe signers → admin Safe signers.

---

## Alert vocabulary (post round 8/9) → runbook

| Alert | Source | Meaning | Go to |
|---|---|---|---|
| `MIGRATION TUPLE VIOLATION` | monitor | A safe Base `Approved`/`Released` contradicts (or provably lacks) a finalized Pendulum burn. Auto-pause fires with it. | RB-1 / RB-3 |
| `UNVERIFIED BASE EVENT` | monitor | A safe Base event whose nonce the monitor's Pendulum view does not know *yet*. Either the monitor's node lags (see `PENDULUM SOURCE VIEW STALLED`) or the event is fabricated; escalates to a violation once provable or after `UNMATCHED_EVENT_GRACE_SECONDS`. | check the monitor's node first; RB-1 if it escalates |
| `PENDULUM SOURCE VIEW STALLED` | monitor | The monitor's finalized Pendulum view is stale — it is going blind. Restore the node before the grace forces a cannot-verify pause. | RB-2 (monitor's node) |
| `CONSERVATION VIOLATION` / `VAULT BALANCE DEFICIT` | monitor | Aggregate loss on Base. Auto-pause fires with it. | RB-3 |
| `SOURCE SUPPLY GREW` | monitor | Pendulum `totalIssuance + TotalMigrated` grew: PEN was minted at the source (a referendum, an unexpected teleport-in) and can be burned against the fixed-supply vault. Not a vault fault; a human decision. | RB-3 step 6 |
| `LIVENESS: migration below approval quorum` | monitor | Attestors are not approving. | RB-2 |
| `LIVENESS: quorum reached but not released` | monitor | Approved but unreleased: cap-deferred and starved, paused, or made releasable by a threshold cut with no `ReleasePending` (the releaser cannot see those). | RB-6 step 4 / RB-4 |
| `CRITICAL: unreleasable migration burned` | monitor / attestor | A burn to the zero or vault address; can never release; excluded from the monitor's pending count. | note for RB-7; nothing to do |
| `release blocked and needs operator action` | releaser | Above the per-release cap, above the daily cap itself, or under-funded vault: needs a governance action. | RB-4 / RB-7 |
| `approvals not durable at the Base finality boundary` | attestor | A block's approvals were reorged away or Base finality is stalled; re-submitted automatically. | watch; RB-2 if persistent |
| `no finalized Pendulum head received` | attestor | The attestor's node stopped finalizing or its subscription died. | RB-2 |

> **Confirming state against a public RPC.** Public endpoints are load-balanced
> and give no read-after-write guarantee: a read issued straight after a
> confirmed transaction can land on a node that has not imported that block yet,
> and report the old value. Every verification step below means "re-read until
> it settles", not "read once" — a single read showing the old value is not
> evidence the transaction failed. This bit the rehearsal twice, on a correctly
> executed admin handover and a correctly executed pause.

## RB-1: Suspected attestor key compromise

**Trigger:** an `Approved` event from an attestor for a tuple that does not
match any finalized Pendulum `MigrationInitiated` event (monitor
`MIGRATION TUPLE VIOLATION`, or manual observation), or an operator reports
infrastructure compromise. An `UNVERIFIED BASE EVENT` page on its own is not
yet evidence: it also fires when the monitor's Pendulum node merely lags —
check for `PENDULUM SOURCE VIEW STALLED` first. If the node is healthy the
monitor escalates to the violation (and pauses) within minutes.

1. **Pause first, investigate second.** Any guardian signer (or the monitor's
   auto-pause) calls `vault.pause()`. Releases stop; approvals keep recording.
2. Confirm the mismatch: compare the suspicious `Approved(nonce, recipient,
   palletAmount, attestor)` event against the pallet's `MigrationInitiated`
   events at the finalized head (`tokenMigration` section).
3. If confirmed compromised: admin Safe executes `vault.removeAttestor(x)`.
   Removal retroactively invalidates all of that attestor's recorded
   approvals — no released funds can result from them afterwards.
4. Operator rotates infrastructure and generates a **new** key; admin Safe
   executes `vault.addAttestor(newKey)`. Never re-add a possibly-leaked key.
5. Reconcile: verify `totalReleased <= TotalMigrated × conversionFactor` and
   that every consumed nonce maps 1:1 to a pallet event. If value was lost,
   follow the incident-disclosure policy before unpausing.
6. Admin Safe executes `vault.unpause()`. Deferred releases can be executed by
   anyone via `vault.release(nonce, recipient, palletAmount)`.

**Rollback:** none needed; pausing is side-effect-free.

## RB-2: Attestor outage (no key compromise)

**Trigger:** monitor M4 liveness alert (nonce unreleased past the grace
period) or an attestor's own restart-loop/low-gas alerts.

1. Determine how many attestors are down. With 1 of 4 down, releases
   continue — treat as routine ops. With 2+ down, migrations queue up
   harmlessly (approvals missing, nothing to roll back) — escalate to the
   affected operators.
2. Common causes, in order of frequency: Base gas wallet empty (fund it; the
   daemon logs the address), Pendulum node not synced/finalizing, the node no
   longer holding the historical state the daemon needs to catch up (an
   outage longer than the node's pruning horizon — the daemon wedges loudly
   on restart: point `PENDULUM_WS` at an archive node; **never edit the
   checkpoint**, and note `START_BLOCK` only applies when no checkpoint file
   exists), Base RPC unable to serve state at the `safe` block while the
   batcher lags (`missing trie node` — the daemon waits it out; if it
   persists, use an endpoint with deeper state history), daemon restart-loop
   after a runtime upgrade (see RB-5).
3. After recovery the daemon catches up from its checkpoint automatically;
   duplicate approvals are impossible (pre-checks + contract dedup).
4. Verify recovery: the queued nonces release as approvals arrive; monitor
   goes back to `ok`.

## RB-3: Conservation invariant violation

**Trigger:** monitor alert `CONSERVATION VIOLATION` or `VAULT BALANCE
DEFICIT`. This is the highest-severity alert the system can produce.

**Note:** the monitor only alerts on a *deficit* (tokens missing), never on a
surplus. A plain inbound transfer of PEN into the vault (a donation, or a
migration whose recipient is the vault) raises the balance above the
conservation identity but is harmless and is deliberately ignored — it can no
longer false-trigger an auto-pause (round-5 fix). The vault also rejects the
vault address as a release recipient at `approve`.

1. Auto-pause should already have fired; **verify `vault.paused() == true`**
   and pause manually if not. Do not unpause until step 5.
2. Rule out monitor error: recompute both sides by hand from independent RPC
   endpoints (`TotalMigrated` at the finalized head; `totalReleased`,
   `balanceOf(vault)`, `totalSupply` on Base).
3. If real: identify the offending `Released` events (those whose nonce has no
   matching pallet event) and the attestors who approved them → continue with
   RB-1 steps 3–5 for every implicated attestor. Assume quorum compromise:
   rotate **all** keys unless positively excluded.
4. Quantify the loss (sum of unmatched releases) and follow the disclosure
   policy. Consider whether caps need lowering before resumption.
5. Unpause only with sign-off from the admin Safe quorum and a written
   incident report.
6. **`SOURCE SUPPLY GREW`** is the source-chain variant: no vault invariant is
   broken, but Pendulum now holds more PEN than the vault was sized for, and
   every burn of it is a genuine burn the attestors will honestly release.
   Identify the mint (a referendum enacting `setBalance`/`update_balance`, a
   teleport-in) from the Pendulum explorer. If it is not a legitimate
   teleport round-trip: pause the **pallet** (`setPaused(true)`, RB-4) to stop
   further burns while governance decides, and treat any migration of the
   minted PEN as loss for disclosure purposes. The monitor cannot pause the
   pallet; only governance can.

## RB-4: Pause / unpause (routine procedure)

**Pause** (guardian Safe, any authorized signer, or admin):
`vault.pause()` — instant; releases stop, approvals keep recording, `migrate`
on Pendulum is unaffected (pause that separately if needed, see below).

**Pendulum-side pause** (for pallet-level incidents or coordinated stops):
`tokenMigration.setPaused(true)` via root/half-council, or 2/3 technical
committee for fast response. This stops new burns at the source.

**Unpause:** admin Safe executes `vault.unpause()` (or `setPaused(false)` on
the pallet via governance). After a vault unpause, deferred releases are
retried permissionlessly via `vault.release(...)` — the attestor fleet does
not need to do anything.

**Order in a coordinated stop:** pause the pallet first (stop new burns), then
the vault. Resume in reverse order.

## RB-5: Pendulum runtime upgrade

**Risk:** a runtime upgrade can change event encoding or the pallet's index,
which would make attestor daemons exit on decode failure (by design, PRD A5).

Before the upgrade is enacted:
1. Check the diff for changes to `pallets/token-migration`, its event type or
   its `construct_runtime` index. No changes → notify operators, no action.
2. If the event shape changed: update and release a new daemon version;
   operators deploy it **before** the upgrade block.
3. Nonce monotonicity across upgrades is a pallet invariant (P2) — any
   migration touching `NextNonce` storage must preserve it; reject one that
   doesn't.

After enactment:
4. Watch the fleet: all four daemons progressing past the upgrade block, test
   migration of a small amount end-to-end, monitor `ok` lines resuming.
5. If daemons exit on the upgrade block: they hold position (checkpoint stays
   put) — fix decoding, redeploy, they resume without loss.

## RB-7: Window close and remainder sweep

**Trigger:** the migration window is closing (per decision D5) and governance
wants to sweep the unmigrated remainder to its designated destination.

**Why this needs care:** `sweepRemainder` reserves only *threshold-approved*
pending releases (`pendingApprovedAmount`). A migration that was burned on
Pendulum but is still gathering attestor approvals is **not** reserved — sweep
it and, while the attestor fleet no longer crash-loops (the release simply
defers and stays recoverable), that user's tokens must be restored by a
governance refund before they can be released. Avoid this by reconciling
first.

1. **Stop new burns:** pause the pallet — `tokenMigration.setPaused(true)` via
   governance/technical committee (RB-4). No new migrations can start.
2. **Wait a finality + processing buffer** (at least the relay finality window
   plus a generous attestor-processing margin; hours, not minutes) so every
   already-finalized migration reaches the vault and is released.
3. **Reconcile via the monitor:** confirm `TotalMigrated × conversionFactor ==
   totalReleased + pendingApprovedAmount + unreleasable` and that the monitor
   reports **zero** outstanding/unreleased nonces for a sustained window.
   `unreleasable` is the sum of burns to the zero or vault address (each paged
   once as `CRITICAL: unreleasable migration burned`); the monitor excludes
   them from its pending count because they can never release. Resolve any
   pending or deferred releases (raise caps / unpause / `release`) before
   proceeding.
4. **Compute the sweep amount** off-chain: `balance − pendingApprovedAmount`,
   and sanity-check it against expected unmigrated supply. Do not sweep more.
5. **Sweep:** admin (timelock) calls `sweepRemainder(destination, amount)`.
   The call reverts (`ExceedsSweepable`) if the amount exceeds
   `balance − pendingApprovedAmount`, as a last-line guard.
6. **Verify:** `totalSwept` increased by `amount`; the monitor's conservation
   check (`balance + totalReleased + totalSwept >= totalSupply`, alerting only
   on a deficit) still holds and does **not** alert (it accounts for
   `totalSwept`).

**If a still-in-flight migration was swept anyway:** its release defers with
`InsufficientVaultBalance` and is marked pending. To make the user whole,
governance transfers the owed token amount back to the vault, then anyone
calls `release(nonce, recipient, palletAmount)`.

## RB-6: Attestor set / threshold change (planned)

1. Admin Safe (timelocked post-handover: expect the configured delay between
   scheduling and execution) calls `addAttestor` / `removeAttestor` /
   `setThreshold`. Invariants enforced on-chain: threshold ≥ 2 and ≤ attestor
   count.
2. Sequence for replacing an operator: `addAttestor(new)` first, wait for
   their daemon to be live and approving, then `removeAttestor(old)`.
3. Removed attestors' recorded approvals stop counting immediately; pending
   migrations that relied on them simply need approvals from the remaining
   set. A **new** key with no checkpoint file backfills from its
   `START_BLOCK` — set it to a block before the oldest unreleased migration.
   A **re-added** existing daemon must re-sign under its new generation:
   stop it, lower `lastProcessedBlock` in its checkpoint file to a block
   before the oldest unreleased migration **keeping every other field** (the
   identity fields are required; a bare `{lastProcessedBlock}` is rejected),
   and restart. Rewinding backwards is the one sanctioned checkpoint edit —
   re-processing is idempotent.
4. **Lowering the threshold** (`setThreshold` to a smaller value) can make a
   payload that was one approval short suddenly releasable, *without* routing
   through `approve()` — so its owed amount is not registered in
   `pendingApprovedAmount` and no `ReleasePending` was ever emitted, which
   means the **releaser cannot see it**. The contract guards the sweep:
   `sweepRemainder` is blocked for `SWEEP_SETTLING_PERIOD` (7 days) after any
   threshold decrease. During that window, call `release(...)` on every
   migration that the new, lower threshold now satisfies — the monitor pages
   `LIVENESS: quorum reached but not released` for each once it is older than
   `GRACE_SECONDS` — so each is properly released or re-registered before the
   next sweep. Never sweep right after cutting the threshold.
