# PEN Migration Invariant Monitor

Independent watchdog for the PEN migration (PRD §6.5). **Must run on
infrastructure separate from every attestor** — its whole value is being an
independent pair of eyes on both chains.

Checks every poll:

| Check | Meaning | Reaction |
|---|---|---|
| M2 tuple match | Every safe Base `Approved` and `Released` event must match one finalized Pendulum `MigrationInitiated` nonce, recipient, and pallet amount; `Released.tokenAmount` must equal `palletAmount × conversionFactor`. | Alert + auto-pause. A mismatch against a *known* nonce pauses immediately. An *unknown* nonce first pages (`UNVERIFIED BASE EVENT`), then pauses as soon as it is provable — the finalized source view passed the event's own timestamp and the nonce still does not exist — or after `UNMATCHED_EVENT_GRACE_SECONDS` if the source view stays behind (a stalled source also pages on its own, see below) |
| M2a aggregate | `totalReleased <= TotalMigrated × conversionFactor`. This remains independent defence-in-depth around the event reconciliation. | Alert + auto-pause |
| M2b: `balanceOf(vault) + totalReleased + totalSwept >= totalSupply` | Vault-internal conservation. Only a **deficit** alerts: a surplus is a harmless inbound transfer (donation, or a migration whose recipient is the vault) and is ignored, so it cannot false-trigger a pause. | Alert + auto-pause on a deficit |
| M4: every old migration reaches active approval quorum — and then releases | Liveness of the attestor fleet (`below approval quorum`), and of everything after it (`quorum reached but not released`: a release starved of daily allowance, paused, or made releasable by a threshold cut without a `ReleasePending`, which the releaser cannot see). | Alert, throttled per nonce |
| Source supply | `totalIssuance + TotalMigrated` on Pendulum must never grow past its first-seen anchor: minted PEN is burned honestly and drains the fixed-supply vault ahead of late migrators, satisfying every other check. | Alert (`SOURCE SUPPLY GREW`); pausing is a human decision |
| Unreleasable burns | A burn to the zero or vault address can never be approved or released; it is paged once and excluded from the pending count so it cannot page liveness forever or block RB-7. | Alert once |

## Configuration (environment)

| Variable | Meaning |
|---|---|
| `PENDULUM_WS` | WebSocket of the monitor's own Pendulum node, run with `--state-pruning archive` (see below) |
| `BASE_RPC_URL` | Base JSON-RPC endpoint — a **single dedicated node**, and a different provider than the attestors use. Load-balanced pools can serve `eth_getLogs` from a lagging replica; each scan range is probed for its end block first, which turns a behind-node into a loud replay instead of a silent gap, but a dedicated node removes the hazard entirely |
| `VAULT_ADDRESS` | MigrationVault address on Base |
| `POLL_INTERVAL_MS` | Poll cadence (default 60s) |
| `GRACE_SECONDS` | Liveness alert threshold (default 30 min) |
| `UNMATCHED_EVENT_GRACE_SECONDS` | Time a lagging Pendulum source view gets to reveal the burn behind a safe Base event before a cannot-verify pause (default 10 min). The first sighting of an unmatched event pages immediately, so this is the window operators have to repair a stalled node. Size it to your node-ops response time |
| `SOURCE_CLOCK_SKEW_MARGIN_SECONDS` | Clock-skew allowance for the fabrication proof (default 120s): once the finalized Pendulum view has passed the moment the monitor first observed an unmatched Base event by this margin and the nonce still does not exist, the event is provably fabricated and pauses without waiting out the grace. The anchor is the monitor's own clock, not the Base block timestamp (which trails real time after a sequencer outage); a future-dated source view proves nothing |
| `SOURCE_STALE_ALERT_SECONDS` | Age of the finalized Pendulum view (vs. wall clock) past which the monitor pages that it is going blind (default 300s) |
| `ISSUANCE_TOLERANCE` | Growth of Pendulum `totalIssuance + TotalMigrated` over its first-seen anchor (12-decimal pallet units) tolerated before `SOURCE SUPPLY GREW` pages (default 0). That sum cannot grow under any legitimate flow; growth means PEN was minted at the source and can be honestly burned against the fixed-supply vault |
| `ALERT_WEBHOOK_URL` | Webhook receiving JSON alerts — wire this to paging |
| `GUARDIAN_PRIVATE_KEY` | Optional: a guardian key enabling automatic `pause()` on conservation violations |
| `BASE_CHAIN_ID` | Default 8453 |
| `PENDULUM_START_BLOCK` | **Required on first run:** first block to scan, normally the pallet activation block or the next finalized block before unpausing |
| `PENDULUM_START_NONCE` | Nonce expected at that block (default 0; set explicitly when starting after any migrations) |
| `BASE_START_BLOCK` | **Required on first run:** vault deployment block or an earlier block |
| `STATE_FILE` | Durable two-chain cursor, pending tuples, and liveness timestamps (default `./monitor-state.json`) |
| `BASE_FINALITY_TAG` | `safe` (default) or `finalized`; only events inside this boundary are trusted |
| `BASE_MAX_BLOCK_RANGE` | Maximum `eth_getLogs` range (default 10,000 blocks) |

## Run

```sh
npm install
npm run build
npm start
```

Run it under a process manager and treat "monitor down" itself as a paging
condition: an unwatched migration is the risk model failing silently. The
two-chain cursors, unresolved canonical tuples, their original finalized block
timestamps, and alert throttles are atomically persisted. A malformed or
Pendulum-genesis/Base-chain/vault-mismatched state file is fatal rather than
silently resetting the security history.

**Node requirements.** The Pendulum node must run with `--state-pruning
archive` (or `archive-canonical`): the monitor catches up block by block
through historical state, and a default-pruned node (256 blocks ≈ 51 min)
cannot serve that after any monitor outage longer than the pruning horizon —
the monitor then wedges loudly on restart. Recovery is pointing `PENDULUM_WS`
at an archive node, never editing the state file. A monitor whose *running*
source view stalls pages `PENDULUM SOURCE VIEW STALLED` well before the
unmatched-event grace can force a cannot-verify pause; treat that page as
"restore the node now".

An automatic pause is reported as successful only after the transaction is
canonical inside the configured Base confirmation boundary and `paused()` is
true there. A merely-mined pause is reported as pending finality.
