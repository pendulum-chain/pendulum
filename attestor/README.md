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
| `PENDULUM_GENESIS_HASH` | Genesis hash that node must serve (default Pendulum's, `0x5d3c2986…462c86`); set it only for a local test chain |
| `BASE_RPC_URL` | Base JSON-RPC endpoint |
| `VAULT_ADDRESS` | MigrationVault address on Base |
| `ATTESTOR_PRIVATE_KEY` | This attestor's signing key (0x-prefixed) |
| `CHECKPOINT_FILE` | Path persisting the last processed block (default `./checkpoint.json`) |
| `START_BLOCK` | First Pendulum block to scan on the very first run |
| `MIN_GAS_BALANCE_WEI` | Low-gas alert threshold (default 0.01 ETH) |
| `ALERT_WEBHOOK_URL` | Optional webhook receiving JSON alerts in Slack incoming-webhook form: `text` holds `[CRITICAL]` or `[WARNING]`, the attestor and the subject, then the detail; `severity`, `subject` and `detail` are also sent as fields. A non-2xx answer, a timeout (10 s) or a connection error is logged as a failed delivery |
| `HEARTBEAT_URL` | Optional URL requested (GET) after every finalized head processed without error; point it at a dead-man's switch that pages when the requests stop. A failed request is logged, never fatal |
| `BASE_CHAIN_ID` | Default 8453 (Base mainnet) |
| `BASE_FINALITY_TAG` | Base confirmation boundary the checkpoint waits for: `safe` (default) or `finalized` |
| `BASE_FINALITY_TIMEOUT_MS` | How long a block's approvals may stay outside that boundary before an alert + idempotent re-submission (default 15 min for `safe`, 45 min for `finalized`) |
| `HEAD_STALL_ALERT_MS` | Page when no finalized Pendulum head has arrived for this long (default 5 min): the daemon is push-driven, so a node that stops finalizing would otherwise idle undetected |
| `MISSED_APPROVAL_ALERT_COUNT` | Page after this many consecutive migrations resolved on-chain without this attestor's own approval landing, and again every as many more (default 3, a whole number of at least 1) |
| `FEE_BUMP_AFTER_SECONDS` | Re-send an own approval that is still not mined after this long (default 60: Base mines a block every 2 s, so this means stuck, not slow) |
| `FEE_BUMP_PERCENT` | Raise both fees by at least this many percent per re-send (default 20; a whole number of at least 10, the minimum a node accepts for a replacement) |
| `FEE_BUMP_MAX` | Re-sends per approval before the `own approval not mined` page (default 3) |
| `MAX_FEE_PER_GAS_GWEI` | No approval ever offers a higher `maxFeePerGas` (default 1, about 200 times Base's usual base fee; one approval uses at most about 300k gas) |

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
- A failed approval for a migration the other attestors released meanwhile
  is a lost race only when our transaction was mined, or the vault rejected it
  as already released or already approved by us. Any other failure (a send
  error, a receipt that never arrived, an empty gas wallet, a removed key) is
  logged with the original error as `own approval did not land` and counted;
  `MISSED_APPROVAL_ALERT_COUNT` consecutive misses page `own approvals are not
  landing`. The rest of the fleet still releases, so this is the only signal
  that the fleet is one attestor short. Any mined approval resets the count.
  Only failed submissions count: a migration already resolved when the daemon
  reaches it is skipped without one, and so is a failure whose own-approval
  check cannot be read. Under a burst, where each failed submission can take
  minutes, the page therefore comes after that many failed submissions, which
  may span many more migrations.
- Each approval is signed locally at the node's pending nonce, or higher when
  an earlier approval of this process already holds that nonce: a nonce one
  migration's approval was sent at is never signed for another. An approval
  still not mined after `FEE_BUMP_AFTER_SECONDS` is re-sent as the identical
  call at the same nonce, with both fees raised by `FEE_BUMP_PERCENT` and at
  least to the node's current estimate, at most `FEE_BUMP_MAX` times and never
  above `MAX_FEE_PER_GAS_GWEI`. Once the migration is released, or its own
  approval is recorded, a transaction still pending is no longer needed, but
  it is followed and bumped the same way until its nonce is used: every later
  approval waits behind that nonce, and mined it merely reverts. When no bump
  is left, or the cap stops one, it pages `own approval not mined` once (with
  the nonce, the account's next mined nonce, the fees and whether the migration
  is already resolved) and keeps waiting, since later approvals would queue
  behind that nonce anyway. Whenever it stops bumping, it re-sends the last
  attempt unchanged, so a node that dropped the transaction while still
  counting its nonce gets it back instead of leaving a gap. An approval that an
  RPC error interrupted keeps its transaction: the retry, or the next approval
  before it sends anything, follows it to the end. A first attempt whose nonce
  is taken (mined already, or held by a transaction this process did not send)
  moves on to the next nonce at once, at most five in a row, without spending
  a bump. A node refusing a send for its price, an underpriced replacement or
  a full pool is retried at the next bump instead of being fatal. Transactions
  in flight are kept in memory only: after a restart, an approval whose
  transaction is still pending is sent again at the next nonce, so a pending
  transaction stuck below the cap has to be replaced by hand.
- Checkpoints are atomically replaced, bound to the Pendulum genesis plus the
  configured Base chain and vault, and malformed files are fatal. Never delete or replace one merely to
  clear an alert; reconcile it against both chains first.
- Checkpoints are also bound to the attestor address and its vault attestor
  generation (bumped by every `addAttestor`). A new key started on another
  key's checkpoint, or a removed and re-added key on its old one, refuses to
  start: resuming would skip the unreleased migrations before it, whose earlier
  approvals no longer count. The error says how to recover: a new key starts
  with no checkpoint and `START_BLOCK` before the oldest unreleased migration;
  a re-added key lowers `lastProcessedBlock` to such a block and sets
  `attestorGeneration` to the current value, keeping every other field.
  Checkpoints written before these fields existed are bound to the first key
  that starts on them.
- The daemon verifies at startup that its address is in the vault's attestor
  set, that `BASE_RPC_URL` serves `BASE_CHAIN_ID` (`eth_chainId`, which the
  checkpoint's Base chain must match too) and that `PENDULUM_WS` serves
  `PENDULUM_GENESIS_HASH`, and refuses to run otherwise. These checks run on
  every start, the first one without a checkpoint included.
- After a Pendulum **runtime upgrade**, verify event decoding against the new
  metadata on a staging node before letting the fleet advance past the
  upgrade block.
