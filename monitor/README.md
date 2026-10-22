# PEN Migration Invariant Monitor

Independent watchdog for the PEN migration (PRD §6.5). **Must run on
infrastructure separate from every attestor** — its whole value is being an
independent pair of eyes on both chains.

Checks every poll:

| Check | Meaning | Reaction |
|---|---|---|
| M2 tuple match | Every safe Base `Approved` and `Released` event must match one finalized Pendulum `MigrationInitiated` nonce, recipient, and pallet amount; `Released.tokenAmount` must equal `palletAmount × conversionFactor`. After a violation the Base cursor stays on any block range whose events the source view may still reveal, so a pause caused by a lagging source does not lose those releases. | Alert + auto-pause on any mismatching `Released`, and on approvals of one nonce that disagree with the finalized Pendulum view from two or more distinct attestors, also across polls and restarts. A single attestor's disagreeing approval pages `CRITICAL: MISMATCHED APPROVAL` instead: one approval can never release. A mismatch against a *known* nonce counts immediately. An *unknown* nonce first pages (`UNVERIFIED BASE EVENT`, without delaying the pause decision), then counts as a mismatch as soon as it is provable — the finalized source view passed the event's own timestamp and the nonce still does not exist — or after `UNMATCHED_EVENT_GRACE_SECONDS` if the source view stays behind or cannot be read (a stalled or unreadable source also pages on its own, see below) |
| M2a aggregate | `totalReleased <= TotalMigrated × conversionFactor + ACKNOWLEDGED_RELEASE_EXCESS`. This remains independent defence-in-depth around the event reconciliation. After a real, quantified loss, operators set `ACKNOWLEDGED_RELEASE_EXCESS` to it (RB-3) so the vault can be unpaused; any further excess still fires. | Alert + auto-pause |
| M2b: `balanceOf(vault) + totalReleased + totalSwept >= totalSupply` | Vault-internal conservation. Only a **deficit** alerts: a surplus is a harmless inbound transfer (donation, or a migration whose recipient is the vault) and is ignored, so it cannot false-trigger a pause. It reads Base only and runs first in every poll, so a slow or failing Pendulum source or Base log scan can neither delay nor skip it. | Alert + auto-pause on a deficit |
| M4: every old migration reaches active approval quorum — and then releases | Liveness of the attestor fleet (`below approval quorum`), and of everything after it (`quorum reached but not released`: a release starved of daily allowance, paused, or made releasable by a threshold cut without a `ReleasePending`, which the releaser cannot see). | One summary page per kind and poll (count and the first nonces; the log lists every nonce), each nonce at most once per `GRACE_SECONDS` |
| Source supply | `totalIssuance + TotalMigrated` on Pendulum must never grow past its first-seen anchor. | Alert (`SOURCE SUPPLY GREW`) |
| Unreleasable burns | A burn to the zero or vault address can never be approved or released; it is paged once and excluded from the pending count so it cannot page liveness forever or block RB-7. | Alert once |
| Pendulum source | The finalized Pendulum view must stay fresh and readable. While it cannot be read (an RPC error, a nonce discontinuity, an event-shape change), the Base-side checks — vault balance and its auto-pause, Base view, auto-pause readiness, vault admin events, governance queue — still run, and Base events are reconciled against the last view read in full: an unreadable source counts as a stalled one, so a mismatch on a known nonce counts at once and an unknown nonce after `UNMATCHED_EVENT_GRACE_SECONDS`. M2a and liveness wait for the source. | Alert, throttled: `PENDULUM SOURCE VIEW STALLED` once the view is older than `SOURCE_STALE_ALERT_SECONDS`, `PENDULUM SOURCE UNREADABLE` on a failure |
| Base log completeness | In every scanned block range the `Released` amounts must add up to the growth of `totalReleased`, and the `RemainderSwept` amounts to that of `totalSwept`, between the block before the range and its last block. A range whose logs do not (a backend serving truncated logs) is read once more, then held: the Base cursor stays on it and M2a and liveness wait until a read accounts for the totals. | Alert (`BASE LOG SCAN INCOMPLETE`), throttled |
| Base view | The safe/finalized Base block must not be older than `BASE_STALE_ALERT_SECONDS`: a view that stops advancing hides every later vault event while every check still passes at the old block. | Alert (`BASE VIEW STALLED`), throttled |
| Auto-pause readiness | At startup and every poll: the Base RPC serves `BASE_CHAIN_ID` and, with a guardian key configured, the key parses, is `vault.guardian()` and holds at least `GUARDIAN_MIN_BALANCE_WEI`. A read that fails pages as well and never stops the poll's other checks. | Alert (`AUTO-PAUSE NOT READY`), throttled; `AUTO-PAUSE DISABLED` once at startup without a key |
| Pendulum governance | Every pending public proposal, the council's external proposal, every ongoing referendum and every open council and technical-committee motion, with the call decoded. A call that mints, moves or unlocks balances outside normal transfers, or changes the runtime or the migration pallet, pages as critical: minted PEN burned through the migration drains the fixed-supply vault, and only Pendulum governance can stop the call before enactment. A call the monitor cannot read pages as critical too: one over 64 KiB is never fetched, and one that does not decode or nests more than 256 calls or 16 levels deep is unreadable; one whose preimage is not noted yet is re-checked every poll and paged again once it can be read. The watch runs after the poll's conservation checks, so it cannot stop or delay them, and it decodes within fixed bounds, so its cost to the next poll is bounded. | Alert once per item (`PENDULUM GOVERNANCE ITEM`, `CRITICAL: Pendulum governance item …`); a motion proposed again with the same call (a new motion index), or an external proposal made again after it left the queue or became a referendum, pages again; a restart re-pages the items still pending |
| Migration pallet admin events | Every finalized `MigrationPauseSet` and `TreasuryDestinationSet`, however fast the motion behind it passed. | Alert once per event (`MIGRATION PALLET ADMIN EVENT`) |
| Vault admin events | Every admin event of the vault (token, pause, unpause, attestors, threshold, caps, guardian, admin transfer, sweep, stale pending cleared). Before the governance handover the admin acts without a delay, so these are the only trace of such a change. This scan (shared with the governance queue) keeps its own persisted cursor: while it fails, every other check still runs, and it resumes where it stopped, also after a restart. | Alert once per event (`VAULT ADMIN EVENT`); the first run also pages the deployment's own events; `BASE ADMIN SCAN FAILED`, throttled, on a failure |
| Governance queue | Every operation the governance Timelock schedules (`CallScheduled`), with its calls decoded and the time it becomes executable. Once the delay has passed anyone may execute a ready operation, so the canceller has exactly the delay to review and veto it. | Alert once per operation and scheduling block (`GOVERNANCE OPERATION SCHEDULED`), so an operation cancelled and scheduled again with the same id pages again |

## Configuration (environment)

| Variable | Meaning |
|---|---|
| `PENDULUM_WS` | WebSocket of the monitor's own Pendulum node, run with `--state-pruning archive` (see below) |
| `BASE_RPC_URL` | Base JSON-RPC endpoint — a **single dedicated node**, and a different provider than the attestors use. Load-balanced pools can serve `eth_getLogs` from a lagging replica; each scan range is probed for its end block first and its logs must account for the vault's running totals, which turns a behind-node into a loud replay instead of a silent gap, but a dedicated node removes the hazard entirely |
| `VAULT_ADDRESS` | MigrationVault address on Base |
| `POLL_INTERVAL_MS` | Poll cadence (default 60s) |
| `GRACE_SECONDS` | Liveness alert threshold (default 30 min) |
| `UNMATCHED_EVENT_GRACE_SECONDS` | Time a lagging or unreadable Pendulum source view gets to reveal the burn behind a safe Base event before a cannot-verify pause (default 10 min). The first sighting of an unmatched event pages immediately, so this is the window operators have to repair a stalled or failing node. Size it to your node-ops response time |
| `SOURCE_CLOCK_SKEW_MARGIN_SECONDS` | Clock-skew allowance for the fabrication proof (default 120s): once the finalized Pendulum view has passed the moment the monitor first observed an unmatched Base event by this margin and the nonce still does not exist, the event is provably fabricated and pauses without waiting out the grace. The anchor is the monitor's own clock, not the Base block timestamp (which trails real time after a sequencer outage); a future-dated source view proves nothing |
| `SOURCE_STALE_ALERT_SECONDS` | Age of the finalized Pendulum view (vs. wall clock) past which the monitor pages that it is going blind (default 300s) |
| `BASE_STALE_ALERT_SECONDS` | Age of the safe/finalized Base block (vs. wall clock) past which `BASE VIEW STALLED` pages (default 600s for `safe`, 1800s for `finalized`; Base's safe head normally trails by about a minute, its finalized head by about 20) |
| `ISSUANCE_TOLERANCE` | Growth of Pendulum `totalIssuance + TotalMigrated` over its first-seen anchor (12-decimal pallet units) tolerated before `SOURCE SUPPLY GREW` pages (default 0) |
| `ACKNOWLEDGED_RELEASE_EXCESS` | Excess of released over migrated, in token units (18 decimals), that operators quantified and accepted after a real loss; M2a fires only beyond it (default 0). Set it at the RB-3 sign-off and restart; the startup log shows it |
| `TIMELOCK_ADDRESS` | Optional: the governance TimelockController. Set it as soon as governance is deployed; the queue is scanned from the monitor's Base cursor onwards, so operations scheduled before the monitor knew the address are not paged |
| `ALERT_WEBHOOK_URL` | Slack incoming webhook receiving the pages (see [Pages](#pages)). A page counts as delivered only on a 2xx answer within 10s: once-only pages (governance items, admin events, scheduled operations, first sightings) are retried every poll until delivered, and failed deliveries are logged. Without it the log line is the page |
| `HEARTBEAT_URL` | Optional: a dead-man's switch URL (Healthchecks.io style), requested with a `GET` (10s timeout) after every poll that ran all checks; a failed request is logged and never stops the monitor. Have the service page when the requests stop: the monitor is down, stuck, or cannot complete its checks (for example while the Pendulum source cannot be read) |
| `GUARDIAN_PRIVATE_KEY` | Optional: the key of `vault.guardian()`, enabling automatic `pause()` on conservation violations; its readiness is checked at startup and every poll, and any other key pages `AUTO-PAUSE NOT READY` (the vault admin may pause too, but its key does not belong on the monitor) |
| `GUARDIAN_MIN_BALANCE_WEI` | Guardian ETH balance below which `AUTO-PAUSE NOT READY` pages (default 10^15 wei = 0.001 ETH) |
| `BASE_CHAIN_ID` | Default 8453; the Base RPC must serve it |
| `PENDULUM_START_BLOCK` | **Required on first run:** first block to scan, normally the pallet activation block or the next finalized block before unpausing |
| `PENDULUM_START_NONCE` | Nonce expected at that block (default 0; set explicitly when starting after any migrations) |
| `BASE_START_BLOCK` | **Required on first run:** vault deployment block or an earlier block |
| `STATE_FILE` | Durable cursors (Pendulum, Base, Base admin scan), pending tuples, and liveness timestamps (default `./monitor-state.json`) |
| `BASE_FINALITY_TAG` | `safe` (default) or `finalized`; only events inside this boundary are trusted |
| `BASE_MAX_BLOCK_RANGE` | Maximum `eth_getLogs` range (default 10,000 blocks) |

## Pages

Each page is a Slack incoming-webhook message, a JSON `POST` of `{ "text": … }`
to `ALERT_WEBHOOK_URL`. The text is `[CRITICAL] pen-monitor: SUBJECT` or
`[WARNING] pen-monitor: SUBJECT`, then the detail on the next line. Slack's
control characters `&`, `<` and `>` are escaped, so no detail can mention
anyone or hide a link, and a detail longer than 3,000 characters is cut short
(the monitor's log line keeps the full text). `CRITICAL` marks the conservation
violations, `AUTO-PAUSE FAILED`, `AUTO-PAUSE UNAVAILABLE` and every subject that
starts with `CRITICAL`; everything else is a `WARNING`.

| Subjects | Meaning |
|---|---|
| `MIGRATION TUPLE VIOLATION`, `VAULT BALANCE DEFICIT`, `CONSERVATION VIOLATION` | A conservation check failed; the auto-pause follows with `vault auto-pause confirmed`, `vault pause confirmed` (already paused), `AUTO-PAUSE PENDING FINALITY`, `AUTO-PAUSE FAILED` or `AUTO-PAUSE UNAVAILABLE` |
| `UNVERIFIED BASE EVENT` | First sighting of a Base event whose nonce the source view does not know yet |
| `CRITICAL: MISMATCHED APPROVAL` | One attestor approved a tuple that disagrees with the finalized Pendulum view; check its key and its Pendulum node. A second attestor doing so for the same nonce pauses the vault |
| `LIVENESS: migration below approval quorum`, `LIVENESS: quorum reached but not released` | Stalled migrations, one summary per kind and poll |
| `PENDULUM SOURCE VIEW STALLED`, `PENDULUM SOURCE UNREADABLE`, `BASE VIEW STALLED`, `BASE ADMIN SCAN FAILED`, `BASE LOG SCAN INCOMPLETE` | The monitor is going blind on one chain |
| `AUTO-PAUSE NOT READY`, `AUTO-PAUSE DISABLED` | The next violation would not pause the vault automatically |
| `SOURCE SUPPLY GREW`, `CRITICAL: unreleasable migration burned on Pendulum` | Supply problems at the source |
| `PENDULUM GOVERNANCE ITEM`, `CRITICAL: Pendulum governance item mints, moves or unlocks balances, or changes the runtime`, `CRITICAL: Pendulum governance item cannot be read`, `MIGRATION PALLET ADMIN EVENT` | Pendulum governance |
| `GOVERNANCE OPERATION SCHEDULED`, `VAULT ADMIN EVENT` | Base governance and vault administration |
| `monitor check failed`, `pendulum governance watch failed`, `multicall unavailable, …`, `multicall batch failed, …`, `monitor startup failed` | The monitor itself |

## Run

```sh
npm install
npm run build
npm start
```

Run it under a process manager and treat "monitor down" itself as a paging
condition (`HEARTBEAT_URL`): an unwatched migration is the risk model failing
silently. The
two-chain cursors, unresolved canonical tuples, their original finalized block
timestamps, and alert throttles are atomically persisted. A malformed or
Pendulum-genesis/Base-chain/vault-mismatched state file is fatal rather than
silently resetting the security history.

**Node requirements.** The Pendulum node must run with `--state-pruning
archive` (or `archive-canonical`): the monitor catches up block by block
through historical state, and a default-pruned node (256 blocks ≈ 51 min)
cannot serve that after any monitor outage longer than the pruning horizon —
the monitor then pages `PENDULUM SOURCE UNREADABLE` on restart. Recovery is
pointing `PENDULUM_WS` at an archive node, never editing the state file. A
monitor whose *running* source view stalls pages `PENDULUM SOURCE VIEW
STALLED` well before the unmatched-event grace can force a cannot-verify
pause; treat that page as "restore the node now". The Base-side checks keep
running while the source cannot be read, and an unreadable source counts as
a stalled one: once a Base event it cannot verify appears, the monitor pages
`UNVERIFIED BASE EVENT` and pauses when the grace runs out, so treat
`PENDULUM SOURCE UNREADABLE` the same way.

An automatic pause is reported as successful only after the transaction is
canonical inside the configured Base confirmation boundary and `paused()` is
true there. A merely-mined pause is reported as pending finality.
