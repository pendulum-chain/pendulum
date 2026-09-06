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
   public RPC means trusting that RPC with release authority. Run the node
   with **`--state-pruning archive`** (or `archive-canonical`): the daemon
   catches up block by block through historical state, and a default-pruned
   node (256 blocks ≈ 51 min) cannot serve that after any daemon outage
   longer than the pruning horizon — the daemon then wedges loudly on
   restart. Recovery from that state is pointing `PENDULUM_WS` at an archive
   node, never editing the checkpoint.
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
| `BASE_FINALITY_TAG` | Base confirmation boundary the checkpoint waits for: `safe` (default) or `finalized` |
| `BASE_FINALITY_TIMEOUT_MS` | How long a block's approvals may stay outside that boundary before an alert + idempotent re-submission (default 15 min for `safe`, 45 min for `finalized`) |
| `HEAD_STALL_ALERT_MS` | Page when no finalized Pendulum head has arrived for this long (default 5 min): the daemon is push-driven, so a node that stops finalizing would otherwise idle undetected |

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

- Blocks are processed strictly in order against the **latest** Base state
  (submission, race detection), so throughput is submission latency and a
  lost k-of-n race — the most ordinary event in the system — is a log line,
  never an alert. The durable **checkpoint trails separately** at the Base
  finality boundary: it advances past a block only once every releasable
  event in it is resolved inside `safe`/`finalized` state. A crash therefore
  re-processes only the blocks whose approvals were not yet durable — safe,
  because approvals are idempotent (`nonceConsumed`/`hasApproved` are checked
  first, and duplicate submissions revert harmlessly). If a block's approvals
  refuse to settle (a reorg dropped them), they are re-submitted after
  `BASE_FINALITY_TIMEOUT_MS` with an alert; the checkpoint never passes an
  unsettled block.
- Checkpoints are atomically replaced, bound to the Pendulum genesis plus the
  configured Base chain and vault, and malformed files are fatal. Never delete or replace one merely to
  clear an alert; reconcile it against both chains first.
- The daemon verifies at startup that its address is in the vault's attestor
  set and refuses to run otherwise.
- After a Pendulum **runtime upgrade**, verify event decoding against the new
  metadata on a staging node before letting the fleet advance past the
  upgrade block (see docs/pen-migration-runbooks.md).
