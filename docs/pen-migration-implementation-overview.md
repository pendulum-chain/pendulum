# PEN → Base Migration — Implementation Overview

**Branches:** `feat/pen-to-base-migration` in this repo (PR #559) and
`feat/pen-base-migration` in the portal repo (PR #655).

This document is the map of everything built for the migration. Design and
requirements live in the [PRD](pen-base-migration-prd.md); the approach
rationale in [ADR-001](adr-001-pen-base-migration-approach.md); the token
extension decisions in [token standards](pen-token-contract-standards.md).

## Architecture recap (one paragraph)

PEN holders call `tokenMigration.migrate(amount, base_address)` on Pendulum;
the amount is burned and a `MigrationInitiated` event with a unique nonce is
emitted. Four attestor daemons (initially team-operated, PRD D4) watch
relay-finalized blocks — each on its own node — and submit
`approve(nonce, recipient, amount)` to the MigrationVault on Base; the 3rd
matching approval releases pre-minted tokens.
The PEN ERC-20 has its entire max issuance minted to the vault at deployment
and no mint function — worst-case loss is bounded by the vault's rate caps,
watched by an independent monitor that can auto-pause. One-way by design.

## Components delivered

### Pendulum repo (`feat/pen-to-base-migration`)

| Component | Location | Status |
|---|---|---|
| `token-migration` pallet | `pallets/token-migration/` | Burn-and-emit `migrate` (user) + `migrate_treasury`/`set_treasury_destination` (governance, fixed Base destination) extrinsics sharing one nonce space and event; unique nonces, dust/ED + lock handling, KeepAlive treasury withdraw, ships paused, pause origin; 21 unit tests + benchmark test suite (frame-benchmarking v2) |
| Runtime wiring | `runtime/pendulum/src/lib.rs` | Pallet index 102, minimum migration amount 100 PEN (sized to dominate the attestor fleet's per-migration Base gas, so dust spam cannot grief it), pause = root/half-council or 2/3 technical committee, added to `BaseFilter` whitelist and `define_benchmarks`; compiles with and without `runtime-benchmarks` (Foucoco intentionally skipped — that chain is no longer live; validation is local plus Base Sepolia) |
| `PEN.sol` | `contracts/src/` | Fixed-supply `ERC20 + ERC20Permit + ERC20Votes`, EIP-6372 timestamp clock, full supply minted to vault, no owner/mint/proxy |
| `MigrationVault.sol` | `contracts/src/` | 3-of-4 on-chain approvals per exact tuple, permanent nonce consumption, 12→18 decimal conversion in one place, per-release + daily caps (defer, not kill), guardian pause (approvals recorded while paused), rotation retroactively invalidates removed attestors, two-step admin, pending-release accounting protecting the timelocked remainder sweep |
| `PENGovernor.sol` | `contracts/src/` | OZ Governor composition through a TimelockController (hybrid governance, timestamp clock) |
| Deploy scripts | `contracts/script/` | `Deploy.s.sol` (vault→token→setToken dance, admin handover to bootstrap Safe), `DeployGovernance.s.sol` (timelock+governor role wiring, deployer admin renounced); parameters documented in `contracts/.env.example` |
| Contract tests | `contracts/test/` | 37 Foundry tests incl. fuzz (supply invariant), full Governor proposal lifecycle, replay/race/rotation/caps/pause/sweep-pending scenarios |
| Attestor daemon | `attestor/` | TypeScript; finalized-heads-only, strictly ordered blocks, crash-safe checkpoint, idempotent + race-tolerant approvals, fail-fast on decode errors (4-field shape asserted), startup set-membership check, low-gas/webhook alerts; ops guide in its README |
| Invariant monitor | `monitor/` | Independent watchdog: conservation checks (block-pinned reads) + per-nonce liveness batched via Multicall3; webhook alerts; optional guardian auto-pause |
| Releaser | `releaser/` | Drains cap-deferred releases via the permissionless `release()`; unprivileged gas-only key; classifies self-healing vs governance-blocked failures |
| Test harness | `testing/` | Automates all four phases of the local test plan: contracts on Anvil, the pallet against a Chopsticks fork of live mainnet state, the full attestor/monitor/releaser pipeline end to end, and relay-chain finality under Zombienet |
| Runbooks | `docs/pen-migration-runbooks.md` | RB-1…RB-7: key compromise, outage, invariant breach, pause/unpause, runtime upgrade, attestor rotation, window close |
| Internal security review | `docs/pen-migration-internal-review.md` | The project's security-assurance record across seven adversarial rounds |

### Portal repo (`feat/pen-base-migration`, PR #655)

| Component | Location | Status |
|---|---|---|
| Migration page | `src/pages/migration/` | Amount validation (transferable, minimum, migrate-all-or-leave-ED, per-release-cap warning), EIP-55 address validation with checksummed preview, `eth_getCode` contract-destination warning + extra confirmation, irreversibility confirmation, pause banner, locked-balance hint, post-finalization release tracking (approvals x/3 → released, BaseScan link) |
| Pallet hook | `src/hooks/migration/useMigrationPallet.tsx` | Extrinsic submission resolving at finality with the emitted nonce; pause query; on-chain constants |
| Base status hook | `src/hooks/migration/useBaseReleaseStatus.ts` | Polls the vault over plain JSON-RPC (no EVM dependency; selectors precomputed, keccak via `@polkadot/util-crypto`) |
| EVM helpers | `src/helpers/ethereum.ts` | EIP-55 checksum, payload-hash mirroring the vault's `abi.encode`, minimal `eth_call`/`eth_getCode` client |
| Config | `src/constants/migration.ts` | Vault address via `VITE_MIGRATION_VAULT_ADDRESS`, Base RPC via `VITE_BASE_RPC_URL`; page degrades gracefully when unset |
| Routing/nav | `src/app.tsx`, `src/components/Layout/links.tsx` | `/pendulum/migration`; nav item hidden on other tenants |

## Internal security review — summary

Adversarial review of the whole stack found and fixed: (1) attestor daemons
crash-looping on the *normal* k-of-n approval race — now a benign re-checked
skip; (2) monitor reads not pinned to one block — could false-positive a
conservation alert and auto-pause the vault; (3) `sweepRemainder` could
strand quorum-approved-but-deferred releases — now excluded via
`pendingApprovedAmount` accounting with a timelocked `clearStalePending`
restricted to consumed nonces. Details and verified-not-vulnerable list in
[pen-migration-internal-review.md](pen-migration-internal-review.md).

## Verification status

| Suite | Result |
|---|---|
| `cargo test -p token-migration` | 21 (22 with `runtime-benchmarks`) |
| `cargo check -p pendulum-runtime` | clean, both feature sets |
| `forge test` | 37, incl. 512-run fuzz and a full Governor lifecycle |
| `attestor` / `monitor` / `releaser` | 6 / 7 / 7 |
| Portal `yarn build` (tsc + vite) | clean against `main` |
| `testing/src/phase1-base.mjs` | 11/11 against the real deploy script on Anvil |
| `testing/src/phase2-pendulum.mjs` | 14/14 against a Chopsticks fork of live mainnet state |
| `testing/src/phase3-e2e.mjs` | 7/7 end to end, four attestors + monitor + releaser |
| `testing/src/phase4-zombienet.mjs` | 7/7 against a real relay (~2-block parachain finality lag) |
| `testing/src/rehearsal.mjs` | 15/15 full stack: local Zombienet Pendulum + Base Sepolia |
| `testing/src/drills.mjs` | 13/13 failure drills (RB-1/3/4/6/7) on the Sepolia stack |
| `testing/src/drill-rb5-upgrade.mjs` | 5/5 — the real spec-26 upgrade enacted under a live fleet on a mainnet fork |

Seven internal adversarial review rounds have run; each found real issues
(sometimes in a prior round's own fix), all fixed with regression tests and
recorded in [pen-migration-internal-review.md](pen-migration-internal-review.md).
**Decision (PRD §9): no external audit is commissioned** — the residual risk is
consciously accepted and carried by the threat-model mitigations (caps,
independent monitoring + auto-pause, guardian, ≥48h timelock, separation of
duties) plus the conservative soft launch. Standing practice: any change to the
fund-release path triggers a fresh internal review round before deployment.

## Still open (cannot be done from the repo)

1. **Decisions D1–D6** (PRD §4.2) — all recorded as decided (burn, 18
   decimals, 150M with the rounding delta to the treasury, 3-of-4
   team-operated attestors, 3-month internal window target subject to the
   community discussion, transferable-only migration). The final window and
   parameters are fixed by the formal governance proposal.
2. Security sign-off before mainnet funding: no external audit will be
   commissioned (PRD §9) — a final internal review pass over the shipped
   revision, plus the operational drills.
3. Benchmark weights generated locally; regenerate on production hardware if the launch timeline allows.
4. Attestor operator onboarding + key ceremonies; Safe setups (D4).
5. Exchange coordination, DefiLlama/CoinGecko supply endpoints, comms.
6. Portal deploy config: set `VITE_MIGRATION_VAULT_ADDRESS` once deployed;
   decide whether the portal feature must be ported to the Preact `staging`
   branch or ships with the React 19 codebase.
