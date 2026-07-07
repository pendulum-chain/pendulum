# PRD: PEN Token Migration from Pendulum to Base

| | |
|---|---|
| **Status** | Draft v1 |
| **Date** | 2026-07-07 |
| **Owner** | Pendulum / SatoshiPay team |
| **Scope** | One-way migration of the native PEN token from the Pendulum parachain (Polkadot) to an ERC-20 on Base, plus post-migration governance |

---

## 1. Summary

We will migrate the PEN token — the native token of the Pendulum Substrate parachain — to Base as a **fixed-supply ERC-20**. The full maximum issuance is pre-minted at deployment into a **MigrationVault** contract; the token contract has **no mint function**. Users migrate by calling a `migrate` extrinsic on Pendulum that removes their PEN from circulation and emits an event carrying their Base address. A **3-of-5 set of independent attestors**, each running their own Pendulum node, observes relay-chain-finalized events and submits matching **on-chain approvals** to the vault on Base; the third matching approval releases the tokens from the vault to the user.

Post-migration governance is **hybrid**: an OpenZeppelin Governor + Timelock on Base for on-chain control of Base-side contracts and treasury, Snapshot for off-chain/cross-chain decisions, executed by an elected Safe multisig, with a technical committee retained for Pendulum-side runtime actions for as long as the chain runs.

The migration is **one-way**. No reverse flow (Base → Pendulum) will be built.

## 2. Background and motivation

- PEN currently exists only as the native token of the Pendulum parachain (12 decimals, Substrate/sr25519 accounts).
- Liquidity, investor attention, and tooling (DefiLlama, CoinGecko, Etherscan-class explorers, DeFi integrations) are concentrated in EVM ecosystems; Base is the chosen destination.
- A key requirement is that supply statistics on Base are **correct and complete from day one**: `totalSupply()` must equal PEN's maximum issuance so trackers never display a confusing split between two chains.
- Verifying Pendulum state cryptographically on an EVM chain would require an on-chain Polkadot light client (BEEFY verification) — a multi-year effort (cf. Snowbridge, Hyperbridge). For a finite-lifetime, one-way migration, a k-of-n attestation model with strict blast-radius limits is the appropriate engineering trade-off.

## 3. Goals

1. A live ERC-20 PEN token on Base whose `totalSupply()` equals the PEN maximum issuance from the moment of deployment.
2. A live MigrationVault on Base holding all unmigrated supply, releasing tokens only on 3-of-5 attestor agreement.
3. A `token-migration` pallet on Pendulum allowing any holder to migrate transferable PEN to a Base address of their choice.
4. Trackers (DefiLlama, CoinGecko, CoinMarketCap) display correct total and circulating supply (vault balance excluded from circulating).
5. Worst-case loss from full attestor-quorum compromise is bounded by rate caps and detected by independent monitoring within minutes.
6. A functioning hybrid governance stack on Base after migration.

### Non-goals (explicitly out of scope)

- **Two-way bridging** (Base → Pendulum) — the design is one-way by construction.
- **On-chain light-client verification** of Pendulum state on Base.
- **Cross-chain governance execution** (Base votes cryptographically executing Substrate calls).
- Migration of any token other than native PEN (Spacewalk-wrapped assets, XCM assets, etc. are unaffected).
- Decommissioning plan for the Pendulum chain itself (tracked separately; this PRD only requires that migration works while the chain runs).

## 4. Decisions

### 4.1 Locked in

| Decision | Choice | Rationale |
|---|---|---|
| Supply model on Base | Entire max issuance pre-minted to vault at deployment; token has no mint function, no owner, no upgradeability | Correct tracker stats from day one; eliminates infinite-mint attack surface; worst case bounded by vault balance |
| Migration direction | One-way only | Halves the attack surface; no Base-side event attestation needed |
| Attestation transport | **On-chain approvals**: each attestor sends its own `approve` transaction to the vault; the k-th matching approval executes the release | No off-chain signature-coordination infrastructure; the chain is the coordinator; attestors are fully independent processes |
| Attestor threshold | 3-of-5 | Tolerates 2 offline/compromised attestors without halting or without theft, respectively |
| Governance | Hybrid: OZ Governor + Timelock (Base contracts/treasury) + Snapshot + executor Safe + Pendulum technical committee | On-chain teeth where the assets live; pragmatic elsewhere |
| Token extensions | `ERC20Permit` + `ERC20Votes` included at deployment | `ERC20Votes` cannot be retrofitted into an immutable token; required for future Governor voting |

### 4.2 Open — must be resolved before implementation freeze

| # | Decision | Options | Recommendation |
|---|---|---|---|
| D1 | Pendulum-side effect of `migrate` | **Burn** vs. lock in keyless pallet account | **Burn.** Migration is one-way; burning keeps the invariant `PEN on Pendulum + released on Base = max issuance` trivially auditable and leaves no honeypot |
| D2 | Decimals on Base | Keep **12** vs. scale to **18** (×10⁶) | **18** (DeFi convention, avoids integration friction), provided max-issuance ×10⁶ arithmetic is verified exact end-to-end and dust-rounding is impossible by construction (12→18 is exact; only relevant if any 18→12 display path exists) |
| D3 | Exact max issuance figure | Confirm the canonical number from tokenomics (including whether any never-minted allocation counts) | Must match what trackers/documentation state today |
| D4 | Attestor set composition | 5 team-operated keys vs. 3 team + 2 external partners | At least 1–2 external/independent operators |
| D5 | Migration window end policy | Open indefinitely vs. close at date T; disposition of vault remainder (burn / DAO treasury) | Announce ≥ 12-month window; decide remainder disposition via governance vote before T |
| D6 | Encumbered balances policy | Handling of staked (`parachain-staking`), vesting (`vesting-manager`), governance-locked, and sub-ED balances | Require unstake/unlock first (migration accepts only transferable balance); publish this clearly since unstaking delay gates user migration speed |

## 5. System overview

```
 PENDULUM (Polkadot parachain)                      BASE (OP-stack L2)
┌─────────────────────────────┐                   ┌──────────────────────────────┐
│ token-migration pallet      │                   │ PEN ERC-20 (immutable)       │
│  migrate(amount, h160)      │                   │  totalSupply = max issuance  │
│  → burn/lock PEN            │                   │  no mint, no owner           │
│  → event {nonce, h160, amt} │                   ├──────────────────────────────┤
└──────────┬──────────────────┘                   │ MigrationVault               │
           │ finalized events                     │  holds unmigrated supply     │
           ▼                                      │  approve(nonce, to, amt)     │
   5 × attestor daemon ──────── Base txs ───────▶ │  3rd matching approval       │
   (own full node each,                           │  → transfer to user          │
    relay-finality only)                          │  caps · pause · timelock     │
                                                  └──────────────────────────────┘
   invariant monitor (independent): Σ burned on Pendulum == Σ released on Base
```

**Happy path:** user calls `migrate(amount, base_address)` on Pendulum → PEN burned/locked, event with unique `nonce` emitted → block reaches relay-chain finality → each attestor independently decodes the event and submits `approve(nonce, recipient, amount)` on Base → on the 3rd identical approval the vault transfers `amount` (decimal-adjusted) to `recipient` and marks `nonce` consumed.

## 6. Component requirements

### 6.1 Pendulum: `token-migration` pallet

- **P1** — Extrinsic `migrate(amount: Balance, base_address: H160)`; atomically removes `amount` of transferable native PEN from the caller (per D1: burn or transfer to keyless pallet account) and emits `MigrationInitiated { nonce: u64, base_address: H160, amount: Balance }`.
- **P2** — `nonce` is a monotonically increasing storage counter; globally unique across the pallet's lifetime; never reused, including across runtime upgrades.
- **P3** — Rejects: `amount` below a configurable minimum (dust threshold ≥ existential-deposit-scale), non-transferable balance (staked, vesting-locked, reserved), and `amount` that would leave the caller between 0 and the existential deposit (must migrate to exactly 0 or stay ≥ ED).
- **P4** — Pallet is pausable via a privileged origin (technical committee / root) to halt new migrations during incidents.
- **P5** — Storage exposes cumulative migrated total for monitoring (`TotalMigrated`).
- **P6** — No knowledge of Base state; the pallet is fire-and-forget. Documentation and UI must make irreversibility explicit.
- **P7** — Deployed to Foucoco (testnet runtime) first with identical logic.

### 6.2 Base: `PEN` ERC-20

- **T1** — OpenZeppelin `ERC20` + `ERC20Permit` + `ERC20Votes`. No other custom logic. Decide the EIP-6372 clock mode (block number vs. timestamp) before deployment — the Governor must use the same clock, and it cannot be changed later.
- **T2** — Constructor mints the entire max issuance (per D2/D3) to the MigrationVault address and nothing else. No `mint`/`burn` owner functions; no `Ownable`; **not upgradeable** (no proxy).
- **T3** — `name`/`symbol` consistent with existing branding (`Pendulum`, `PEN`); decimals per D2.

### 6.3 Base: `MigrationVault`

- **V1** — `approve(nonce, recipient, amount)` callable only by addresses in the attestor set. Approvals are counted per `keccak256(abi.encode(nonce, recipient, amount))` — attestors must agree on the *identical* tuple. A conflicting tuple for the same nonce counts separately and never merges.
- **V2** — On the k-th (k = 3) distinct-attestor approval of the same tuple: verify nonce unconsumed → verify rate caps → mark nonce consumed → `transfer(recipient, amount)`. Consumed nonces are permanent (mapping, not sequential counter — out-of-order finalization/submission must work).
- **V3** — One approval per attestor per tuple; duplicate approvals from the same attestor revert.
- **V4** — **Rate caps:** per-release maximum and rolling 24h aggregate maximum, both governance-configurable behind the timelock. Initial values sized so a full quorum compromise loses a bounded, pre-agreed amount before pause (target: < 1–2% of vault balance per day).
- **V5** — **Pause:** a guardian role (fast Safe, small threshold) can pause releases instantly. Unpause and all parameter changes (caps, attestor set, guardian) go through a TimelockController with ≥ 48h delay.
- **V6** — Attestor set changes (add/remove/replace key) via timelocked admin only; changing the set must not invalidate pending approvals in a way that permanently strands a legitimate migration (re-approval by the new set must be possible).
- **V7** — Decimal conversion 12 → 18 (if D2 = 18) happens in exactly one place (the vault, at release), as an exact ×10⁶ multiplication.
- **V8** — Not upgradeable. All flexibility comes from parameters + pause. Emits full event history (`Approved`, `Released`, `Paused`, `CapsUpdated`, `AttestorSetUpdated`) for the monitor and for public auditability.
- **V9** — End-of-window handling per D5: a timelocked function to sweep the remainder to a governance-designated destination (or burn), callable only after a hard-coded earliest timestamp.

### 6.4 Attestor daemon (×5 independent instances)

- **A1** — Connects **only to its own Pendulum full node** (never public RPC); subscribes to **relay-chain-finalized** heads; decodes `MigrationInitiated` events. Never acts on best/unfinalized blocks.
- **A2** — For each event, submits `approve(nonce, recipient, amount)` to the vault on Base, with idempotent retry (safe to resubmit; duplicates revert harmlessly) and crash-recovery from a persisted checkpoint (last processed finalized block).
- **A3** — Each instance: separate operator, separate infrastructure, separate secp256k1 key (HSM or equivalent isolation), separately funded Base gas wallet with balance alerting.
- **A4** — No shared code paths for event *interpretation* where avoidable is nice-to-have; at minimum, no shared runtime infrastructure or key storage. No communication between attestors — the vault contract is the only coordination point.
- **A5** — Handles runtime upgrades on Pendulum gracefully (metadata refresh) and alerts on decode failures rather than skipping events silently.

### 6.5 Invariant monitor (independent watchdog)

- **M1** — Runs on infrastructure separate from all attestors; reads Pendulum (`TotalMigrated`, per-nonce events) and Base (`Released` events, vault balance) independently.
- **M2** — Continuously checks: (a) every released nonce corresponds to exactly one finalized Pendulum event with matching recipient/amount; (b) Σ released ≤ Σ migrated; (c) vault balance + Σ released = max issuance.
- **M3** — On any violation: page on-call immediately and (design decision) optionally hold a guardian key to auto-pause the vault.
- **M4** — Also monitors liveness: alerts if a finalized migration event has < 3 approvals after N minutes (attestor outage detection).

### 6.6 Migration UI

- **U1** — Web app: connect Substrate wallet, enter/connect Base address with **EIP-55 checksum validation**, explicit irreversibility confirmation, live status tracking (finalization → approvals 0/3 → released, with Base tx link).
- **U2** — Warn when the destination is a contract address (Safe is fine; other contracts may strand funds); require an extra confirmation.
- **U3** — Surface encumbered-balance state (D6): show staked/vesting amounts and guide the user through unstaking first.
- **U4** — Encourage a small test migration for large holders as a documented pattern.

### 6.7 Governance stack (hybrid)

- **G1** — Snapshot space with PEN-on-Base voting strategy. **The vault address must be excluded from voting power and quorum math** — quorum thresholds must be defined against circulating supply, not `totalSupply()`, or they are unreachable early in the migration.
- **G2** — OZ Governor + TimelockController on Base using `ERC20Votes` (delegation-based). The timelock becomes the admin of the MigrationVault parameters (caps, attestor set, end-of-window sweep) after an initial bootstrap period during which a Safe holds admin (see rollout).
- **G3** — Executor Safe (elected signers) carries out Snapshot outcomes that are off-chain or on other chains; optionally hardened later with oSnap/SafeSnap.
- **G4** — Pendulum-side runtime actions remain with the existing technical committee for as long as the chain runs; its mandate post-migration is documented (security patches, pallet pause, no discretionary treasury power).
- **G5** — The pause guardian is **not** the Governor (too slow for incidents); it is a small fast Safe, itself replaceable via timelock.

## 7. Acceptance criteria

1. **Supply correctness:** immediately after deployment, `PEN.totalSupply()` on Base equals the confirmed max issuance (D3) and 100% sits in the vault; DefiLlama/CoinGecko display total supply = max issuance and circulating supply excluding the vault.
2. **End-to-end migration:** a user migrating X PEN on Pendulum receives exactly X (decimal-adjusted) PEN on Base after relay finality + 3 approvals, with no manual intervention, on testnet and mainnet.
3. **Conservation invariant:** at all times, Σ burned/locked on Pendulum ≥ Σ released on Base, and vault balance + Σ released = max issuance; the monitor demonstrably alerts (staging drill) on injected violation.
4. **Replay safety:** a consumed nonce can never release twice (unit + fork-test proof); an attestor submitting the same approval twice has no effect.
5. **Quorum safety:** 2 colluding attestors cannot release anything; 2 offline attestors do not halt migrations.
6. **Caps and pause:** releases above per-tx or daily caps revert; guardian pause takes effect in one transaction and blocks all releases; every parameter change is observably delayed ≥ 48h by the timelock.
7. **No mint surface:** verified absence of any code path that increases `totalSupply()` post-constructor (audit assertion).
8. **Governance live:** Snapshot space operational with vault excluded from strategy; Governor + Timelock deployed, delegation working, and admin of the vault transferred per rollout plan.
9. **Audits complete:** all critical/high findings from all audit tracks (see §9) resolved or formally accepted before mainnet vault funding.
10. **Ops readiness:** runbooks exist and have been drill-tested for: attestor key compromise, attestor outage, invariant violation, pause/unpause, and Pendulum runtime upgrade.

## 8. Security requirements and threat model

**Trust assumptions:** correctness reduces to (a) ≤ 2 of 5 attestor keys compromised at any time, (b) Polkadot relay finality is honest, (c) the vault contract is correct. There is no cryptographic verification of Pendulum state on Base; the design compensates with independence, caps, monitoring, and pause.

| Threat | Mitigation |
|---|---|
| Attestor key compromise (< quorum) | 3-of-5 threshold; conflicting tuples never merge; monitor flags approvals without matching Pendulum events |
| Attestor quorum compromise (≥ 3 keys) | Rate caps bound daily loss (V4); independent monitor + guardian pause (M3, V5); key isolation & operator independence (A3, D4) make simultaneous compromise unlikely |
| Fake/reorged Pendulum events | Attestors act only on relay-finalized blocks from their own nodes (A1); post-finality reorgs are not possible on Polkadot |
| Replay / double release | Permanent consumed-nonce mapping (V2); per-attestor per-tuple dedup (V3) |
| Malicious/typo destination address | UI checksum + contract-address warnings (U1, U2); irreversibility messaging; documented test-migration pattern |
| Infinite mint on Base | Structurally impossible — no mint function (T2) |
| Governance capture of vault params | 48h timelock on all changes (V5) gives holders and the monitor time to react; guardian can pause during the window |
| Pallet abuse (griefing with dust, nonce games) | Minimum amount (P3); nonce is pallet-internal, not user-supplied (P2) |
| Attestor gas exhaustion / outage | Funded-wallet alerting (A3); liveness monitoring (M4); 2-of-5 outage tolerance |
| RPC/supply-chain trust | Own full nodes only (A1); pinned dependencies and reproducible builds for daemon and contracts |

**Standing security requirements:** all privileged Base keys in Safes or HSMs; no single human can both approve and change the attestor set; public disclosure/bug-bounty channel before mainnet; all contracts verified on the Base explorer.

## 9. Audit scope

Ranked by where the risk actually lives:

1. **MigrationVault contract (highest priority):** approval counting and tuple hashing (V1–V3), nonce consumption, cap accounting across the 24h window, pause/timelock/role wiring, attestor-set rotation edge cases (V6), decimal conversion (V7), end-of-window sweep (V9).
2. **`token-migration` pallet:** atomicity of burn/lock + event emission, nonce monotonicity across upgrades, balance-encumbrance checks (P3), pause origin, weight/benchmarking correctness.
3. **Attestor daemon:** event decoding against runtime metadata (including post-upgrade), finality handling (proof that it cannot act pre-finality), checkpoint/crash-recovery correctness, key handling.
4. **End-to-end trust-boundary review:** an adversarial walkthrough of the full pipeline (extrinsic → event → daemon → approval → release), explicitly attempting cross-component exploits that no single-component audit would catch (e.g., decode ambiguity producing divergent tuples).
5. **Operational review (lighter):** key-management ceremony, Safe configurations, timelock parameters, monitor independence.

**Out of audit scope:** OpenZeppelin library internals, Base/OP-stack infrastructure, Polkadot finality itself, the ERC-20 beyond confirming it is an unmodified OZ composition (a cheap assertion worth paying for). Token + vault ≈ 300–400 lines of Solidity total — solicit fixed bids from 2 firms; the pallet and daemon likely need a Substrate-literate auditor (may be a different firm).

## 10. Rollout plan

| Phase | Contents | Gate to next phase |
|---|---|---|
| **0 — Decisions & spec** | Resolve D1–D6; finalize this PRD; publish tokenomics/max-issuance statement | All open decisions signed off |
| **1 — Build & testnet** | Pallet on Foucoco; contracts on Base Sepolia; 5 test attestors; monitor; UI; internal adversarial testing incl. chaos drills (kill attestors, inject bad approvals) | All acceptance criteria pass on testnet |
| **2 — Audits** | §9 tracks in parallel; fix and re-verify; publish reports | No open critical/high findings |
| **3 — Mainnet soft launch** | Deploy token + vault (full supply minted); production attestor ceremony; **conservative caps**; team-only + invited large-holder migrations for 1–2 weeks; vault admin held by bootstrap Safe | Soft-launch volume clean, monitor green |
| **4 — Public launch** | Runtime upgrade enabling `migrate` for all; UI public; raise caps to target; tracker submissions (DefiLlama/CoinGecko: supply endpoints, vault as non-circulating); exchange & community comms | ≥ agreed % supply migrated or T reached |
| **5 — Governance handover** | Snapshot space live from phase 4; deploy Governor + Timelock; transfer vault admin from bootstrap Safe to timelock; elect executor Safe | — |
| **6 — Window close (per D5)** | Governance vote on remainder disposition; execute sweep (V9); decommission attestors; final conservation report published | — |

## 11. Risks and open questions

- **Adoption risk:** slow migration leaves circulating supply small and Snapshot quorums awkward — mitigate with a long window, clear comms, and quorum defined on circulating supply (G1).
- **Unstaking delay friction (D6):** staked holders face the staking unbond period before they can migrate; comms must set expectations.
- **Attestor operational maturity:** the honest hard part is ops, not code. External operators (D4) need onboarding, SLAs, and gas-funding agreements.
- **Exchange coordination:** any CEX listing PEN needs a supported path (they migrate custody balances themselves via the same mechanism); start conversations in phase 1.
- **Legal/regulatory review** of the migration mechanics and any public statements about supply — not covered by this PRD, must run in parallel.
- **Pendulum chain end-state** (full sunset vs. minimal maintenance) is deliberately out of scope but interacts with D1 and G4; schedule that decision before phase 6.

## 12. Deliverables checklist

- [ ] `token-migration` pallet (+ benchmarks, tests) in this repo, deployed to Foucoco then Pendulum
- [ ] `PEN.sol`, `MigrationVault.sol` (+ Foundry test suite incl. fork tests and invariant tests)
- [ ] Attestor daemon (open-sourced) + deployment guide for external operators
- [ ] Invariant monitor + alerting integration
- [ ] Migration web UI
- [ ] Governor + Timelock deployment scripts; Snapshot space config (vault-excluded strategy)
- [ ] Runbooks: key compromise, attestor outage, invariant breach, pause/unpause, runtime upgrade
- [ ] Audit reports (published) and fix log
- [ ] Tracker submissions and public migration documentation
