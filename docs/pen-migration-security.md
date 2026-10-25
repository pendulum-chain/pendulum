# PEN Migration — Security Overview

How the PEN migration from Pendulum to Base is secured: what has to be
trusted, which properties the system enforces, and how a released runtime can
be verified. The full design is in the [technical PRD](pen-base-migration-prd.md).

## Components

| Component | Where | Role |
|---|---|---|
| `token-migration` pallet | Pendulum | Burns PEN and emits `MigrationInitiated { nonce, who, base_address, amount }`. Ships paused. |
| `PEN` | Base | Fixed-supply ERC-20: 150,000,000 PEN minted once to the vault, no mint function. |
| `MigrationVault` | Base | Releases PEN once 3 of 4 attestors approve the same `(nonce, recipient, amount)`. |
| Attestors (×4) | Off-chain | Watch relay-finalized Pendulum blocks and submit approvals to the vault. |
| Releaser | Off-chain | Retries releases deferred by a cap or a pause. No privileges. |
| Monitor | Off-chain | Reconciles both chains independently and can pause the vault on a conservation violation. |
| `PENGovernor` + `TimelockController` | Base | On-chain governance that takes over vault administration after the bootstrap phase. |

## Trust model

Correctness relies on:

1. at most 2 of the 4 attestor keys being compromised at any time,
2. Polkadot relay-chain finality, and
3. the correctness of the vault contract.

There is no cryptographic verification of Pendulum state on Base. The design is
trusted and damage-bounded: the attestor set is initially operated by the
team, and the safeguards below limit what a compromise can do.

## Safeguards

- **Release caps.** A per-release cap and a rolling daily cap limit how much
  can leave the vault (1,000,000 PEN each for the soft launch, raised by
  governance later).
- **Pause.** A guardian can pause releases in a single transaction but can
  never unpause; unpausing requires the vault admin.
- **Admin changes.** Admin transfers are two-step. After the governance
  handover every admin action passes a 48-hour timelock, during which a
  designated canceller can veto a queued operation. The monitor pages every
  operation the timelock schedules.
- **Sweep floor.** The unmigrated remainder cannot be moved before
  2027-03-01 00:00 UTC (immutable), and only through a governance decision.
- **Separation of duties.** The guardian and the monitor are operated by
  people who hold no attestor keys.
- **Pallet limits.** The pallet ships paused. Locked, vesting and staked
  balances cannot be migrated, and the minimum migration is 100 PEN.

## Invariants

- `vault balance + totalReleased + totalSwept >= PEN.totalSupply`; only a
  deficit is treated as a violation.
- A nonce is released at most once, and only with exactly the
  `(recipient, amount)` that was approved.
- Only approvals from current attestors count; a removed attestor's
  approvals stop counting immediately.
- Approved releases that are deferred by a cap or a pause stay reserved
  against any sweep.
- A migration reduces Pendulum total issuance by exactly the migrated amount.
- Amounts are converted from 12 to 18 decimals (×10⁶) exactly once, at release.

## Assurance

- Unit and invariant tests for the pallet, the contracts and all three
  services, run in CI.
- End-to-end tests against forked Pendulum mainnet state, a local relay chain
  and Base Sepolia, including failure drills (attestor loss, pause and
  unpause, attestor rotation, window close) and a runtime-upgrade drill.
- Repeated internal adversarial review of the full stack; all critical and
  high findings were fixed and regression-tested.

## Verifying the runtime (Pendulum spec 26)

The runtime that added the pallet was built reproducibly with srtool and
published as the GitHub release `pendulum-release-26`.

| | |
|---|---|
| Asset | `pendulum_runtime.compact.compressed.wasm` (2,177,219 bytes) |
| sha256 | `370ac425e69125ac3d23971a1d2eba7966b15c7026621dd6a778c099432eb66d` |
| blake2_256 (code hash) | `0x6a19a408826fb4d2ab04886c864714dda188df2a1bfa11f27b1b2d0c0192e896` |
| Source commit | `14eb720945e1c37e4b8f3f107157c86e00e8b2be` |

The code hash is the value the on-chain upgrade authorization checked. To
verify, download the asset from the release and compare its blake2_256 with
the hash above.
