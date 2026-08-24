# PEN Migration Invariant Monitor

Independent watchdog for the PEN migration (PRD §6.5). **Must run on
infrastructure separate from every attestor** — its whole value is being an
independent pair of eyes on both chains.

Checks every poll:

| Check | Meaning | Reaction |
|---|---|---|
| M2a: `totalReleased <= TotalMigrated × conversionFactor` | Tokens may never leave the vault without a corresponding finalized burn on Pendulum. A violation is the signature of attestor-quorum compromise. | Alert + auto-pause the vault (if `GUARDIAN_PRIVATE_KEY` is set) |
| M2b: `balanceOf(vault) + totalReleased + totalSwept >= totalSupply` | Vault-internal conservation. Only a **deficit** alerts: a surplus is a harmless inbound transfer (donation, or a migration whose recipient is the vault) and is ignored, so it cannot false-trigger a pause. | Alert + auto-pause on a deficit |
| M4: every nonce older than `GRACE_SECONDS` is consumed on Base | Liveness of the attestor fleet (outage, cap deferral, pause). Per-nonce reads are batched via Multicall3 so a large backlog cannot starve the checks above. | Alert |

## Configuration (environment)

| Variable | Meaning |
|---|---|
| `PENDULUM_WS` | WebSocket of the monitor's own Pendulum node |
| `BASE_RPC_URL` | Base JSON-RPC endpoint (ideally a different provider than the attestors use) |
| `VAULT_ADDRESS` | MigrationVault address on Base |
| `POLL_INTERVAL_MS` | Poll cadence (default 60s) |
| `GRACE_SECONDS` | Liveness alert threshold (default 30 min) |
| `ALERT_WEBHOOK_URL` | Webhook receiving JSON alerts — wire this to paging |
| `GUARDIAN_PRIVATE_KEY` | Optional: a guardian key enabling automatic `pause()` on conservation violations |
| `BASE_CHAIN_ID` | Default 8453 |

## Run

```sh
npm install
npm run build
npm start
```

Run it under a process manager and treat "monitor down" itself as a paging
condition: an unwatched migration is the risk model failing silently. Note the
liveness state (`nonceFirstSeen`) is in-memory — after a restart, grace timers
restart from zero, which can only delay (never lose) a liveness alert.
