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

- The attestor's approve path is the most review-scarred code in the stack;
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
| `POLL_INTERVAL_MS` | Default 60s |
| `MAX_BLOCK_RANGE` | Max blocks per `eth_getLogs` (default 10,000) |
| `MIN_GAS_BALANCE_WEI` | Low-gas alert threshold |
| `ALERT_WEBHOOK_URL` | Optional JSON alert webhook |
| `BASE_CHAIN_ID` | Default 8453 |
| `BASE_FINALITY_TAG` | `safe` (default) or `finalized`; the scan cursor stays inside this boundary and a pending entry is dropped only once its nonce is consumed there |
| `BLOCKED_ALERT_INTERVAL_MS` | Re-page interval for governance-blocked releases (default 6h — the condition needs a ≥48h timelocked action, so per-poll paging would bury the signal) |
| `READ_BATCH_SIZE` | Bound on Multicall calldata and individual fallback concurrency (default 100) |

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
| anything else | **Alert** |

A successful `release()` does **not** drop the entry immediately: it leaves the
durable pending set only once the nonce reads as consumed at the
`safe`/`finalized` boundary, so a reorged-away release is still ours to retry.
Until then re-attempts are gas-free simulations classified as pending finality.

State (safe/finalized scan checkpoint + pending set) is atomically persisted
and bound to the configured chain and vault, so a restart resumes without
trusting unsafe logs or losing pending work. Malformed state is fatal. Multicall
is disabled permanently only when an on-chain code check proves it absent; a
temporary provider error falls back in bounded batches for that cycle.

**Known limitation:** the pending set is driven by `ReleasePending`, which the
vault emits when a threshold is crossed inside `approve()`. A payload made
releasable purely by a governance `setThreshold` *decrease* emits no such event
and is not picked up here — that path is rare, timelocked, guarded by the
7-day sweep settling period, and covered by runbook RB-6.
