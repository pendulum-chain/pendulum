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
| `BASE_RPC_URL` | Base JSON-RPC endpoint |
| `VAULT_ADDRESS` | MigrationVault address |
| `RELEASER_PRIVATE_KEY` | Gas-only signing key (no privileges) |
| `START_BLOCK` | Vault deployment block — where log scanning begins on a first run |
| `STATE_FILE` | Scan checkpoint + pending set (default `./releaser-state.json`) |
| `POLL_INTERVAL_MS` | Default 60s |
| `MAX_BLOCK_RANGE` | Max blocks per `eth_getLogs` (default 10,000) |
| `MIN_GAS_BALANCE_WEI` | Low-gas alert threshold |
| `ALERT_WEBHOOK_URL` | Optional JSON alert webhook |
| `BASE_CHAIN_ID` | Default 8453 |

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
| `ExceedsDailyCap`, `EnforcedPause`, `InsufficientVaultBalance`, `NotEnoughApprovals` | **Retry quietly** — self-heals; this is the normal backlog case |
| `NonceAlreadyConsumed` | **Done** — drop it |
| `ExceedsPerReleaseCap` | **Alert** — cannot self-heal, needs a governance `setCaps` behind the timelock |
| anything else | **Alert** |

State (scan checkpoint + pending set) is persisted, so a restart resumes
without rescanning from the deployment block or losing pending work.

**Known limitation:** the pending set is driven by `ReleasePending`, which the
vault emits when a threshold is crossed inside `approve()`. A payload made
releasable purely by a governance `setThreshold` *decrease* emits no such event
and is not picked up here — that path is rare, timelocked, guarded by the
7-day sweep settling period, and covered by runbook RB-6.
