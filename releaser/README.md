# PEN Migration Releaser

Drains **deferred** migration releases on Base.

When a migration reaches the attestor threshold but cannot be released right
away — the rolling daily cap is exhausted, the vault is paused, or it is
under-funded — the vault records it as pending and emits `ReleasePending`
rather than reverting. Those conditions heal on their own, but the vault does
not self-execute: someone must call the permissionless `release()`.

Nothing else does. Attestors only submit `approve` for new finalized events;
the monitor is deliberately read-only. Without this service a launch-day
backlog sits pending until an operator clears it by hand, nonce by nonce.

## Why a separate process

- The attestor's approve path is the most delicate code in the stack;
  adding a second responsibility to it re-opens that risk.
- The monitor must stay a cheap, read-only, independent watchdog.
- **This key holds no privilege.** `release()` is permissionless and can only
  pay the recipient the attestors already approved, subject to the same caps
  and pause. It needs gas and nothing else, and must never be an attestor,
  guardian or admin key.

Run **two instances** for redundancy — the loser of a race just observes a
consumed nonce and drops it.

## Configuration

| Variable | Meaning |
|---|---|
| `BASE_RPC_URL` | Base JSON-RPC endpoint — prefer a single dedicated node: each scan range's end block is probed first so a lagging load-balanced replica causes a loud replay instead of a silently skipped `ReleasePending` |
| `VAULT_ADDRESS` | MigrationVault address |
| `RELEASER_PRIVATE_KEY` | Gas-only signing key (no privileges) |
| `START_BLOCK` | Vault deployment block — where log scanning begins on a first run |
| `STATE_FILE` | Scan checkpoint + pending set (default `./releaser-state.json`) |
| `POLL_INTERVAL_MS` | Default 60s; at least 1000 |
| `MAX_BLOCK_RANGE` | Max blocks per `eth_getLogs` (default 10,000; a whole number of at least 1). A range the endpoint refuses as too large is read again at half the size, down to one block, and the size doubles again after 10 full-size reads in a row, never above this value; the current size is in each cycle's `ok:` log line. Set it to the endpoint's cap to avoid the refused reads (Base Sepolia's public endpoint allows 200) |
| `MIN_GAS_BALANCE_WEI` | Low-gas alert threshold |
| `ALERT_WEBHOOK_URL` | Optional JSON alert webhook in Slack incoming-webhook form: `text` holds `[CRITICAL]` or `[WARNING]`, the releaser and the subject, then the detail; `severity`, `subject` and `detail` are also sent as fields. A non-2xx answer, a timeout (10 s) or a connection error is logged as a failed delivery and tried again after 2 s and after 10 s more; a 4xx other than 408 and 429 is not retried, as repeating the request cannot change it |
| `HEARTBEAT_URL` | Optional http(s) URL requested (GET) after a cycle that completed without error, at most once a minute; point it at a dead-man's switch that pages when the requests stop. Startup refuses any other value. A failed request is logged (error name and code only, never the URL), never fatal |
| `BASE_CHAIN_ID` | Default 8453; startup refuses a `BASE_RPC_URL` that serves another chain (`eth_chainId`) |
| `BASE_FINALITY_TAG` | `safe` (default) or `finalized`; the scan cursor stays inside this boundary and a pending entry is dropped only once its nonce is consumed there |
| `BLOCKED_ALERT_INTERVAL_MS` | Re-page interval for the same page about one release: governance-blocked, or failing unexpectedly (default 6h — a blocked release needs a ≥48h timelocked action, so per-poll paging would bury the signal). A different page about it goes out at once |
| `READ_BATCH_SIZE` | Bound on Multicall calldata and individual fallback concurrency (default 100; a whole number of at least 1) |
| `FEE_BUMP_AFTER_SECONDS` | Re-send a release transaction that is still not mined after this long (default 60) |
| `FEE_BUMP_PERCENT` | Raise both fees by at least this many percent per re-send (default 20; a whole number of at least 10, the minimum a node accepts for a replacement) |
| `FEE_BUMP_MAX` | Re-sends per transaction before the `release transaction not mined` page (default 3) |
| `MAX_FEE_PER_GAS_GWEI` | No release transaction ever offers a higher `maxFeePerGas` (default 1) |

## Run

```sh
npm install && npm run build && npm start
```

## Behaviour

Each cycle it scans new `ReleasePending` logs, drops nonces the vault has
already consumed, then simulates and submits `release()` for the rest.
Simulating first means a still-capped release costs no gas.

Failures are classified rather than treated alike:

| Revert | Outcome |
|---|---|
| `ExceedsDailyCap`, `EnforcedPause`, `NotEnoughApprovals` | **Retry quietly** — expected temporary conditions |
| `NonceAlreadyConsumed` (at the finality boundary) | **Done** — drop it |
| `ExceedsPerReleaseCap`, `InsufficientVaultBalance` | **Alert, throttled** to `BLOCKED_ALERT_INTERVAL_MS` — cannot self-heal without governance or operator action |
| anything else | **Alert** `unexpected release failure`, **throttled** to `BLOCKED_ALERT_INTERVAL_MS` per release |

A successful `release()` does **not** drop the entry immediately: it leaves the
durable pending set only once the nonce reads as consumed at the
`safe`/`finalized` boundary, so a reorged-away release is still ours to retry.
Until then re-attempts are gas-free simulations classified as pending finality.

Each release is signed at the account's pending nonce with the node's fee
estimate, capped at `MAX_FEE_PER_GAS_GWEI`. One not mined within
`FEE_BUMP_AFTER_SECONDS` is followed from cycle to cycle instead of being
signed again, and no other release is sent meanwhile, since it would only
queue behind that nonce. Every `FEE_BUMP_AFTER_SECONDS` it is re-sent as the
identical call at the same nonce with both fees raised by `FEE_BUMP_PERCENT`,
and at least to the current estimate, at most `FEE_BUMP_MAX` times. When no
bump is left, or the cap stops one, it pages `release transaction not mined`
(with the nonce, the account's next mined nonce and the fees; throttled to
`BLOCKED_ALERT_INTERVAL_MS`) and keeps waiting. Once another transaction uses
its nonce, or the node holds nothing at a nonce below it, the release is
simulated and sent again. The transaction in flight is kept in memory only:
after a restart, a pending one stuck below the cap has to be replaced by hand.

State (safe/finalized scan checkpoint + pending set) is atomically persisted
and bound to the configured chain and vault, so a restart resumes without
trusting unsafe logs or losing pending work. Malformed state is fatal, and so is
an endpoint that does not serve `BASE_CHAIN_ID`, checked before the state is
read on every start. Multicall
is disabled permanently only when an on-chain code check proves it absent; a
temporary provider error falls back in bounded batches for that cycle.

**Known limitation:** the pending set is driven by `ReleasePending`, which the
vault emits when a threshold is crossed inside `approve()`. A payload made
releasable purely by a governance `setThreshold` *decrease* emits no such event
and is not picked up here — that path is rare, timelocked, guarded by the
7-day sweep settling period, and covered by runbook RB-6.
