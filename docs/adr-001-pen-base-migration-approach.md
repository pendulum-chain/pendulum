# ADR-001: Approach for migrating PEN from Pendulum to Base

**Status:** Accepted
**Date:** 2026-07-07
**Deciders:** Pendulum / SatoshiPay team
**Companion doc:** [pen-base-migration-prd.md](pen-base-migration-prd.md) (the full requirements for the chosen approach)

## Context

PEN is the native token of the Pendulum parachain (Substrate, Polkadot, 12 decimals, sr25519 accounts). We want to move it to Base as an ERC-20. Hard requirements that shaped the decision:

1. `totalSupply()` on Base must equal PEN's maximum issuance **from day one**, so trackers (DefiLlama, CoinGecko) never show a confusing supply split across two chains.
2. Users migrate by giving up PEN on Pendulum and receiving it on Base (lock/burn-and-release).
3. This is a **migration**, not a bridge product: finite lifetime, one direction, and the token contract that remains afterwards should be maximally trustless and boring.

The fundamental constraint behind everything below: **Base cannot cheaply verify Pendulum state.** Real cryptographic verification of Polkadot finality on an EVM chain requires an on-chain light client (BEEFY signature verification, validator-set tracking). That is what Snowbridge and Hyperbridge are, and each took a dedicated team years. Every approach is therefore a different answer to the question: *what do we trust instead, and how do we bound the damage if that trust fails?*

Additional constraints: sr25519 signatures cannot be affordably verified on the EVM (rules out direct "prove you own this Substrate account" claims on Base); we control the Pendulum runtime (this repo), so adding a pallet is cheap for us; PEN teleport to AssetHub already shipped (#553), so the Snowbridge route is partially paved.

## Decision

**Option A — a purpose-built one-way migration:** a small `token-migration` pallet on Pendulum (burn + event with a unique nonce and target H160), a fixed-supply ERC-20 on Base with the entire max issuance pre-minted into a MigrationVault, and a 3-of-4 attestor set (initially team-operated, each on its own node — PRD D4) that watches relay-finalized Pendulum events and submits **on-chain approvals** to the vault; the third matching approval releases the tokens.

Post-migration governance is **hybrid** (Option G4 below): OZ Governor + Timelock for Base-side contracts and treasury, Snapshot + executor Safe for off-chain/cross-chain matters, technical committee retained for Pendulum runtime actions.

## Options considered — migration mechanism

### Option A: Purpose-built one-way migration (attestor-based) — CHOSEN

| Dimension | Assessment |
|---|---|
| Complexity | Medium — ~150-line pallet, ~300-line vault, 4 small daemons; 3–6 weeks + security review |
| Trust model | 3-of-4 designated attestors (initially team-operated, PRD D4); damage bounded by rate caps + pause + independent monitor + separation of duties |
| Meets supply requirement | Yes, by construction (pre-mint to vault) |
| UX | One extrinsic on Pendulum, tokens arrive on Base automatically |
| Ongoing burden | Attestor ops for the migration window only; nothing permanent |

**Pros:**
- We control both ends; the design can be exactly as simple as the problem requires.
- The token contract itself ends up with **zero trust assumptions** (no mint function, no owner, no proxy) — the trusted component (vault + attestors) is temporary and rate-limited.
- Pre-minting satisfies the day-one supply requirement trivially.
- One-way design halves the attack surface of a bridge (no Base→Pendulum attestation).
- The on-chain-approvals variant needs no coordination infrastructure at all.

**Cons:**
- The attestor set is a real trust assumption (mitigated by independence, caps, monitoring, pause — see PRD §8).
- We own the operational burden: key ceremonies, monitoring, runbooks, gas funding.
- Custom code needs careful adversarial review (though the surface is small and standard).

### Option B1: Ride existing infrastructure — AssetHub → Snowbridge → Ethereum → Base standard bridge

| Dimension | Assessment |
|---|---|
| Complexity | Low code, very high integration/UX complexity (4 hops) |
| Trust model | Strongest available (light-client bridges + canonical rollup bridge) |
| Meets supply requirement | **No** |
| UX | Multi-hop, multi-wallet, slow, fee-laden |
| Ongoing burden | Dependent on three external bridge systems |

**Pros:** trust-minimized end to end; almost no code to write (teleport to AssetHub already shipped); no attestors to operate.

**Cons — and why it was rejected:**
- The Base token would be a bridge-wrapped representation whose supply reflects only what has been bridged — **fails the day-one total-supply requirement outright.**
- We would not control the Base contract (created by the OP standard bridge), so no `ERC20Votes`, no governance integration, no say in metadata.
- Four-hop UX (Pendulum → AssetHub → Ethereum → Base) is unacceptable for a general holder base, and each hop has its own fees, delays, and failure modes.
- This route is designed for *bridging*, and it is a fine answer to "make PEN reachable"; it is a poor answer to "migrate PEN's home."

### Option B2: Hyperbridge (ISMP) as message channel

| Dimension | Assessment |
|---|---|
| Complexity | High — ISMP pallet-stack integration into the runtime, dependency on external relayer economics |
| Trust model | Consensus proofs (BEEFY) — trust-minimized, no attestor set of our own |
| Meets supply requirement | Only with the same pre-mint-to-vault construction as Option A |
| UX | Good (direct Polkadot↔Base messaging) |
| Ongoing burden | Permanent runtime dependency on ISMP pallets across all future SDK upgrades |

**Pros:** genuinely trust-minimized without building a light client ourselves; direct route to Base; would be the right backbone if we ever wanted a permanent two-way bridge.

**Cons — and why it was rejected:**
- Heavy runtime integration for a mechanism we intend to run for a bounded migration window, then decommission.
- Adds a permanent maintenance tax: the ISMP pallet stack must survive every Polkadot-SDK upgrade this repo goes through.
- Still needs the vault/pre-mint construction to satisfy the supply requirement, so it replaces only the attestor layer — the most easily bounded part of Option A — at the highest integration cost.
- **Revisit trigger:** if two-way bridging ever becomes a product requirement, re-evaluate Hyperbridge before extending Option A.

### Option C: Snapshot + Merkle-claim airdrop

| Dimension | Assessment |
|---|---|
| Complexity | Low on Base (Merkle distributor), but a hard identity problem |
| Trust model | Trustless claims on Base — but only after a trusted registration/snapshot step |
| Meets supply requirement | Yes (pre-mint to distributor) |
| UX | Hard cutover; claim flow; registration prerequisite |
| Ongoing burden | Low |

**Pros:** the Base side is fully trustless once the Merkle root is set; minimal infrastructure; clean if the chain is being shut down on a fixed date.

**Cons — and why it was rejected:**
- **sr25519 cannot be verified on the EVM**, so users cannot prove ownership of their Pendulum account in the claim contract. They would have to register a Base address *on Pendulum before the snapshot* — which is already half of Option A's pallet, without its flexibility.
- Forces a hard cutover: balances frozen at block X, one shot at the Merkle root, no way to accommodate late unstakers (staking/vesting locks mean many holders *cannot* be ready at an arbitrary snapshot date).
- Whoever computes the Merkle root is a single trusted party at one critical moment — concentration of the same trust Option A spreads across 5 parties and time.
- Only appropriate for a scheduled chain shutdown, which is not (yet) the plan.

## Options considered — key sub-decisions within Option A

### Supply model: pre-mint to vault (chosen) vs. mint-on-demand

Mint-on-demand is the classic bridge pattern but fails the day-one supply requirement (`totalSupply` grows with migrations) and — worse — requires a live minter privilege on the token forever, making infinite mint the top attack scenario. Pre-minting the max issuance to the vault makes `totalSupply` correct from deployment, lets the token ship with **no mint function at all**, and caps the worst case at the vault's remaining balance. Circulating supply is reported to trackers as `totalSupply − vault balance`. This resolved what initially looked like a conflict between the "full supply visible" and "lock and mint" requirements: migration becomes lock/burn-and-**release**.

### Attestation transport: on-chain approvals (chosen) vs. off-chain signature aggregation vs. light client

- **On-chain approvals** (chosen): each attestor sends `approve(nonce, recipient, amount)` directly to the vault; the contract counts distinct-attestor approvals of the identical tuple and executes on the k-th. The chain is the coordinator — no signature-collection service, no API, no gossip; attestors share nothing but the contract address. Cost: k transactions per migration instead of one (cents on Base). Chosen for operational simplicity and attestor independence.
- **Off-chain aggregation** (Wormhole-style): attestors sign EIP-712 payloads, a service collects k signatures, anyone submits one `release(..., sigs[])` transaction. One tx per migration and user-self-serve claims, but requires building and operating a coordination service — rejected as unnecessary at migration volumes.
- **Light client / consensus proofs:** correct in the limit, disproportionate for a one-way migration (see Options B1/B2).

### Pendulum-side effect: burn vs. lock — OPEN (PRD D1, recommendation: burn)

Burn keeps the global invariant (`PEN on Pendulum + released on Base = max issuance`) trivially auditable and leaves no honeypot account on the Substrate side. Lock only makes sense if reverse flow is ever plausible — which the one-way decision forecloses. Kept open in the PRD only until the Pendulum chain end-state discussion concludes.

## Options considered — post-migration governance

The forcing fact: **migrated PEN cannot vote on Pendulum.** Whether burned or locked, it is invisible to `pallet-democracy`/referenda, so on-chain governance power on Pendulum shrinks to the unmigrated remainder — an adversely-selected and ever-cheaper-to-capture electorate.

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **G1: Keep Pendulum as the governance chain** | No new infrastructure; familiar tooling | Governance token has left the chain — legitimacy collapses and capture gets cheaper daily; permanent coretime + collator + SDK-upgrade burden just to host votes | Rejected |
| **G2: Snapshot + executor Safe only** | Free, gasless, fastest to ship; works for any decision scope | Execution is trusted (Safe could ignore votes); weak optics for treasury-scale decisions | Rejected as sole mechanism; retained as a component |
| **G3: Full on-chain Governor + Timelock only** | Trustless execution; the setup investors recognize (Tally) | Only natively controls Base-side things; gas-cost voting UX; overkill for off-chain/cross-chain decisions | Rejected as sole mechanism; retained as a component |
| **G4: Hybrid (G2 + G3 + Pendulum technical committee) — CHOSEN** | Trustless where the assets live (Base treasury, vault parameters); pragmatic everywhere else; technical committee keeps the chain patchable without pretending it is token-governed | Two venues to operate; requires clear scoping of what is decided where | **Chosen** |

Consequences for the token contract: `ERC20Votes` must be included at deployment (immutable token — cannot be retrofitted). Snapshot quorums must be defined against circulating supply with the vault address excluded, or they are unreachable early in the migration.

## Trade-off analysis (summary)

The decisive requirement was **day-one supply correctness**, which only a self-deployed, pre-minted token satisfies — eliminating B1 outright and reducing B2 to "a more expensive attestor replacement." Between A and C, the sr25519 problem means C secretly contains A's registration pallet anyway, while adding a hard-cutover constraint that conflicts with staking/vesting lock realities. Within A, every sub-choice followed one principle: **make the permanent artifact (the token) trustless and boring, and confine all trust into a temporary, rate-limited, monitored, pausable component.**

## Consequences

**Easier:** tracker/investor-facing supply story (correct from day one); security review (small, standard surfaces); incident response (caps + pause + single trusted component); eventual decommissioning (turn off attestors, sweep vault per governance vote).

**Harder:** we own attestor operations (key ceremonies, monitoring, gas funding, external-operator onboarding); users must trust the attestor set during the window (mitigated, not eliminated); no reverse path if anyone regrets migrating.

**To revisit:** Hyperbridge if two-way bridging ever becomes a requirement; the Pendulum chain end-state (interacts with burn-vs-lock, D1); governance venue consolidation once migration completes.

## Action items

1. [ ] Resolve PRD open decisions D1–D6 (see [PRD §4.2](pen-base-migration-prd.md))
2. [ ] Spec the `token-migration` pallet in this repo
3. [ ] Draft `PEN.sol` + `MigrationVault.sol` and run internal adversarial reviews (PRD §9)
4. [ ] Open attestor-operator conversations (D4) and exchange coordination (PRD §11)
