# PEN → Base Migration — Implementation Overview

**Date:** 2026-07-07
**Branches:** `feat/pen-base-migration` in this repo and in the portal repo
(`~/Documents/portal`, based on the React 19 branch `fix-issues-with-new-ss58format`;
note: `origin/staging` there is still the older Preact codebase).

This document is the map of everything built for the migration. Design and
requirements live in the [PRD](pen-base-migration-prd.md); the approach
rationale in [ADR-001](adr-001-pen-base-migration-approach.md); the token
extension decisions in [token standards](pen-token-contract-standards.md).

## Architecture recap (one paragraph)

PEN holders call `tokenMigration.migrate(amount, base_address)` on Pendulum;
the amount is burned and a `MigrationInitiated` event with a unique nonce is
emitted. Five independent attestor daemons watch relay-finalized blocks (each
on its own node) and submit `approve(nonce, recipient, amount)` to the
MigrationVault on Base; the 3rd matching approval releases pre-minted tokens.
The PEN ERC-20 has its entire max issuance minted to the vault at deployment
and no mint function — worst-case loss is bounded by the vault's rate caps,
watched by an independent monitor that can auto-pause. One-way by design.

## Components delivered

### Pendulum repo (`feat/pen-base-migration`)

| Component | Location | Status |
|---|---|---|
| `token-migration` pallet | `pallets/token-migration/` | Burn-and-emit extrinsic, unique nonces, dust/ED + lock handling, pause origin; 10 unit tests + benchmark test suite (frame-benchmarking v2) |
| Runtime wiring | `runtime/pendulum/src/lib.rs` | Pallet index 102, min amount 1 PEN, pause = root/half-council or 2/3 technical committee, added to `BaseFilter` whitelist and `define_benchmarks`; compiles with and without `runtime-benchmarks` (Foucoco intentionally skipped — production-direct decision) |
| `PEN.sol` | `contracts/src/` | Fixed-supply `ERC20 + ERC20Permit + ERC20Votes`, EIP-6372 timestamp clock, full supply minted to vault, no owner/mint/proxy |
| `MigrationVault.sol` | `contracts/src/` | 3-of-5 on-chain approvals per exact tuple, permanent nonce consumption, 12→18 decimal conversion in one place, per-release + daily caps (defer, not kill), guardian pause (approvals recorded while paused), rotation retroactively invalidates removed attestors, two-step admin, pending-release accounting protecting the timelocked remainder sweep |
| `PENGovernor.sol` | `contracts/src/` | OZ Governor composition through a TimelockController (hybrid governance, timestamp clock) |
| Deploy scripts | `contracts/script/` | `Deploy.s.sol` (vault→token→setToken dance, admin handover to bootstrap Safe), `DeployGovernance.s.sol` (timelock+governor role wiring, deployer admin renounced); parameters documented in `contracts/.env.example` |
| Contract tests | `contracts/test/` | 30 Foundry tests incl. fuzz (supply invariant), full Governor proposal lifecycle, replay/race/rotation/caps/pause/sweep-pending scenarios |
| Attestor daemon | `attestor/` | TypeScript; finalized-heads-only, strictly ordered blocks, crash-safe checkpoint, idempotent + race-tolerant approvals, fail-fast on decode errors (4-field shape asserted), startup set-membership check, low-gas/webhook alerts; ops guide in its README |
| Invariant monitor | `monitor/` | Independent watchdog: conservation checks (block-pinned reads) + per-nonce liveness; webhook alerts; optional guardian auto-pause |
| Runbooks | `docs/pen-migration-runbooks.md` | RB-1…RB-6: key compromise, outage, invariant breach, pause/unpause, runtime upgrade, attestor rotation |
| Internal security review | `docs/pen-migration-internal-review.md` | Independent adversarial pass; 2 high + 1 medium findings, all fixed (see below) |

### Portal repo (`feat/pen-base-migration`)

| Component | Location | Status |
|---|---|---|
| Migration page | `src/pages/migration/` | Amount validation (transferable, minimum, migrate-all-or-leave-ED), EIP-55 address validation with checksummed preview, `eth_getCode` contract-destination warning + extra confirmation, irreversibility confirmation, pause banner, locked-balance hint, post-finalization release tracking (approvals x/3 → released, BaseScan link) |
| Pallet hook | `src/hooks/migration/useMigrationPallet.tsx` | Extrinsic submission resolving at finality with the emitted nonce; pause query; on-chain constants |
| Base status hook | `src/hooks/migration/useBaseReleaseStatus.ts` | Polls the vault over plain JSON-RPC (no EVM dependency; selectors precomputed, keccak via `@polkadot/util-crypto`) |
| EVM helpers | `src/helpers/ethereum.ts` | EIP-55 checksum, payload-hash mirroring the vault's `abi.encode`, minimal `eth_call`/`eth_getCode` client |
| Config | `src/constants/migration.ts` | Vault address via `VITE_MIGRATION_VAULT_ADDRESS`, Base RPC via `VITE_BASE_RPC_URL`; page degrades gracefully when unset |
| Routing/nav | `src/app.tsx`, `src/components/Layout/links.tsx` | `/pendulum/migration`; nav item hidden on other tenants |

## Internal security review — summary

Adversarial review of the whole stack found and fixed: (1) attestor daemons
crash-looping on the *normal* 3-of-5 approval race — now a benign re-checked
skip; (2) monitor reads not pinned to one block — could false-positive a
conservation alert and auto-pause the vault; (3) `sweepRemainder` could
strand quorum-approved-but-deferred releases — now excluded via
`pendingApprovedAmount` accounting with a timelocked `clearStalePending`
restricted to consumed nonces. Details and verified-not-vulnerable list in
[pen-migration-internal-review.md](pen-migration-internal-review.md).

## Verification status

- Pallet: `cargo test -p token-migration` 10/10; with `runtime-benchmarks` 11/11.
- Runtime: `cargo check -p pendulum-runtime` clean, both feature sets.
- Contracts: `forge test` 30/30 (incl. 512-run fuzz).
- Attestor & monitor: `tsc --noEmit` clean.
- Portal: `tsc --noEmit` and ESLint clean; committed through lint-staged.

## Commit map (this repo)

`docs → pallet → contracts(core) → runtime wiring → contracts(governance) →
attestor → monitor → runbooks → benchmarks → security fixes → env template`
— see `git log` on the branch for hashes.

## Still open (cannot be done from the repo)

1. **Decisions D1–D6** (PRD §4.2) — most urgently max issuance (D3) and
   decimals (D2), both baked into immutable contracts at deployment.
2. External audits (PRD §9) — the internal review doc is the starting brief.
3. Benchmark run on reference hardware → replace manual weights.
4. Attestor operator onboarding + key ceremonies; Safe setups (D4).
5. Exchange coordination, DefiLlama/CoinGecko supply endpoints, comms.
6. Portal deploy config: set `VITE_MIGRATION_VAULT_ADDRESS` once deployed;
   decide whether the portal feature must be ported to the Preact `staging`
   branch or ships with the React 19 codebase.
