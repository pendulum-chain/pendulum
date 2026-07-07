# PEN Migration — Operational Runbooks

Runbooks required by PRD acceptance criterion 10. Each must be drill-tested
before mainnet launch. Contact points, Safe addresses and paging channels are
filled in during the attestor onboarding ceremony (decision D4).

**Shared prerequisites:** access to the alerting channel; read access to a
Pendulum node and a Base RPC; the on-call sheet mapping attestor index →
operator → contact. Escalation path everywhere: on-call engineer → migration
tech lead → guardian Safe signers → admin Safe signers.

---

## RB-1: Suspected attestor key compromise

**Trigger:** an `Approved` event from an attestor for a tuple that does not
match any finalized Pendulum `MigrationInitiated` event (monitor M2a alert, or
manual observation), or an operator reports infrastructure compromise.

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

1. Determine how many attestors are down. With ≤ 2 of 5 down, releases
   continue — treat as routine ops. With 3+ down, migrations queue up
   harmlessly (approvals missing, nothing to roll back) — escalate to the
   affected operators.
2. Common causes, in order of frequency: Base gas wallet empty (fund it; the
   daemon logs the address), Pendulum node not synced/finalizing, checkpoint
   file pointing at a pruned block (re-point `START_BLOCK` at a block the node
   still has, never past unprocessed migrations), daemon restart-loop after a
   runtime upgrade (see RB-5).
3. After recovery the daemon catches up from its checkpoint automatically;
   duplicate approvals are impossible (pre-checks + contract dedup).
4. Verify recovery: the queued nonces release as approvals arrive; monitor
   goes back to `ok`.

## RB-3: Conservation invariant violation

**Trigger:** monitor alert `CONSERVATION VIOLATION` or `VAULT BALANCE
MISMATCH`. This is the highest-severity alert the system can produce.

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
4. Watch the fleet: all five daemons progressing past the upgrade block, test
   migration of a small amount end-to-end, monitor `ok` lines resuming.
5. If daemons exit on the upgrade block: they hold position (checkpoint stays
   put) — fix decoding, redeploy, they resume without loss.

## RB-6: Attestor set / threshold change (planned)

1. Admin Safe (timelocked post-handover: expect the configured delay between
   scheduling and execution) calls `addAttestor` / `removeAttestor` /
   `setThreshold`. Invariants enforced on-chain: threshold ≥ 2 and ≤ attestor
   count.
2. Sequence for replacing an operator: `addAttestor(new)` first, wait for
   their daemon to be live and approving, then `removeAttestor(old)`.
3. Removed attestors' recorded approvals stop counting immediately; pending
   migrations that relied on them simply need approvals from the remaining
   set (the new attestor's daemon backfills from its `START_BLOCK` — set it
   to a block before the oldest unreleased migration).
