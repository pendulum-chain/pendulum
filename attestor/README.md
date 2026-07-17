# PEN Migration Attestor

Daemon run by each of the four attestor operators (3-of-4, initially
team-operated — PRD §6.4 / D4).
Watches **relay-finalized** blocks on the operator's **own** Pendulum node for
`tokenMigration.MigrationInitiated` events and submits the matching
`approve(nonce, recipient, palletAmount)` transaction to the MigrationVault on
Base. The vault releases the tokens on the 3rd matching approval; attestors
never communicate with each other — the contract is the only coordination
point.

## Non-negotiable operational rules (PRD A1–A5)

1. **Run your own Pendulum full node** and point `PENDULUM_WS` at it. Using a
   public RPC means trusting that RPC with release authority.
2. **Key isolation:** the attestor key signs only vault `approve` calls. Keep
   it in an HSM/KMS signer where possible; never reuse it elsewhere. The same
   address pays gas — keep it funded with Base ETH (the daemon alerts below
   `MIN_GAS_BALANCE_WEI`).
3. **Separate infrastructure per operator** — different hosting, different
   credentials, nothing shared with other attestors or with the monitor.
4. The daemon **exits on any decode or processing error** instead of skipping
   events. Run it under a process manager (systemd example below) and page a
   human when it restart-loops: a stuck attestor on a runtime upgrade usually
   means the metadata changed and the daemon needs updating.

## Configuration (environment)

| Variable | Meaning |
|---|---|
| `PENDULUM_WS` | WebSocket of your own Pendulum node, e.g. `ws://127.0.0.1:9944` |
| `BASE_RPC_URL` | Base JSON-RPC endpoint |
| `VAULT_ADDRESS` | MigrationVault address on Base |
| `ATTESTOR_PRIVATE_KEY` | This attestor's signing key (0x-prefixed) |
| `CHECKPOINT_FILE` | Path persisting the last processed block (default `./checkpoint.json`) |
| `START_BLOCK` | First Pendulum block to scan on the very first run |
| `MIN_GAS_BALANCE_WEI` | Low-gas alert threshold (default 0.01 ETH) |
| `ALERT_WEBHOOK_URL` | Optional webhook receiving JSON alerts |
| `BASE_CHAIN_ID` | Default 8453 (Base mainnet) |

## Run

```sh
npm install
npm run build
npm start
```

### systemd example

```ini
[Unit]
Description=PEN migration attestor
After=network-online.target

[Service]
EnvironmentFile=/etc/pen-attestor/env
WorkingDirectory=/opt/pen-attestor
ExecStart=/usr/bin/node dist/main.js
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

## Behavior details

- Blocks are processed strictly in order; the checkpoint advances only after
  every event in a block is handled. A crash re-processes at most one block —
  safe, because approvals are idempotent (`nonceConsumed`/`hasApproved` are
  checked first, and duplicate submissions revert harmlessly).
- The daemon verifies at startup that its address is in the vault's attestor
  set and refuses to run otherwise.
- After a Pendulum **runtime upgrade**, verify event decoding against the new
  metadata on a staging node before letting the fleet advance past the
  upgrade block (see docs/pen-migration-runbooks.md).
