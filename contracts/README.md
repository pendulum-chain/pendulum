# PEN on Base — Migration Contracts

Solidity contracts for the one-way migration of PEN from the Pendulum parachain
to Base. Design and requirements: [PRD](../docs/pen-base-migration-prd.md),
[ADR-001](../docs/adr-001-pen-base-migration-approach.md),
[token standards](../docs/pen-token-contract-standards.md).

## Contracts

- **`src/PEN.sol`** — fixed-supply ERC-20 (`ERC20 + ERC20Permit + ERC20Votes`,
  EIP-6372 timestamp clock). The entire max issuance is minted to the
  MigrationVault in the constructor; there is no mint function, no owner and no
  proxy.
- **`src/MigrationVault.sol`** — holds the unmigrated supply and releases it on
  the 3rd matching on-chain approval from the attestor set (per Pendulum
  migration nonce). Includes per-release and daily caps, guardian pause,
  timelock-friendly two-step admin, attestor rotation that retroactively
  invalidates a removed attestor's approvals, and a time-locked remainder sweep
  for the end of the migration window.

## Deployment order

The token and vault reference each other, so:

1. Deploy `MigrationVault(admin, guardian, attestors[4], threshold=3, conversionFactor=1e6, perReleaseCap, dailyCap, earliestSweepTimestamp)`
2. Deploy `PEN(vault, maxIssuance)` — mints the full supply into the vault
3. `vault.setToken(pen)` (admin, one-time; verifies the vault holds 100% of supply)

`conversionFactor = 1e6` converts 12-decimal pallet amounts to the 18-decimal
token; attestors always submit the raw pallet amount from the
`MigrationInitiated` event — the conversion happens in the vault and nowhere
else (PRD V7).

## Build & test

Requires [Foundry](https://getfoundry.sh). Dependencies are git submodules
(`lib/openzeppelin-contracts` v5.4.0, `lib/forge-std`); after a fresh clone run
`git submodule update --init --recursive` or `forge install`.

```sh
forge build
forge test
```

## Open parameters (fixed at deployment, see PRD §4.2)

- `maxIssuance` — exact figure pending decision D3
- decimals/`conversionFactor` — 18/1e6 pending decision D2
- attestor addresses, caps, `earliestSweepTimestamp` — decisions D4/D5
