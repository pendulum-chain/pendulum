# PEN ERC-20 on Base: Token Standards and Extensions

**Status:** Accepted
**Date:** 2026-07-07
**Companion docs:** [PRD](pen-base-migration-prd.md) (requirement T1), [ADR-001](adr-001-pen-base-migration-approach.md)

This document records which token standards and contract extensions the PEN ERC-20 on Base implements, which it deliberately omits, and why. The guiding principle (from ADR-001): the token is the *permanent* artifact of the migration and must be maximally trustless and boring — every line of behavior should be audited OpenZeppelin code, and every capability we exclude is an attack surface or credibility question we never have to answer.

## Baseline

- **OpenZeppelin Contracts 5.x** (latest audited release at implementation time), plain inheritance — no proxy, no upgradeability.
- Total composition: `ERC20 + ERC20Permit + ERC20Votes`, roughly 20 lines of custom Solidity (constructor + required overrides).
- Constructor mints the entire max issuance to the MigrationVault; there is no other supply-affecting code path.

## Included extensions

| Extension | Standard | Function it provides | Why included |
|---|---|---|---|
| `ERC20` | EIP-20 | Core fungible token | — |
| `ERC20Permit` | EIP-2612 | `permit(owner, spender, value, deadline, v, r, s)`: approvals via off-chain EIP-712 signatures instead of an on-chain `approve` transaction. Enables one-transaction approve-and-swap and relayer-paid (gasless) onboarding. | Table stakes for a modern token; zero added trust assumptions; expected by aggregators and wallets. |
| `ERC20Votes` | EIP-5805 | Checkpointed balance history + delegation (`delegate`, `delegateBySig`, `getVotes`, `getPastVotes`, `getPastTotalSupply`). Required by OZ Governor / Tally. | Governance is a hard requirement (hybrid model, ADR-001) and checkpointing **cannot be retrofitted** into an immutable token. Note: votes only count after delegation — holder docs must tell users to self-delegate. |
| EIP-6372 clock | (part of Votes) | `clock()` / `CLOCK_MODE()`: whether checkpoints are keyed by block number (OZ default) or timestamp. | Must be decided **before deployment** and the Governor must be deployed with the same mode. Leaning: **timestamp** on an L2 (human-legible, robust to block-cadence changes). Tracked in PRD T1. |

### Interplay notes

- **Smart-contract wallets and `permit`:** EIP-2612 verifies ECDSA signatures only, so Safe/ERC-4337 accounts (which sign via ERC-1271) cannot use `permit`. This is normal and acceptable: those users fall back to `approve` or to **Permit2** (Uniswap's external allowance contract), which provides signature-based, expiring approvals for *any* ERC-20 with no token-side support required. We implement nothing for Permit2; it simply works alongside.

## Considered and excluded

| Extension / standard | What it does | Why excluded |
|---|---|---|
| `ERC20Burnable` | Holders burn their own tokens | Breaks the two core invariants: `totalSupply()` would drift below max issuance (tracker requirement, PRD Goal 1) and the monitor's `vault + released = totalSupply` check (PRD M2) would need carve-outs. Vault-remainder disposition at window close is handled by the vault (V9), not the token. |
| `ERC20FlashMint` (EIP-3156) | Flash loans via temporary mint/burn | Directly contradicts the no-mint, fixed-`totalSupply` guarantee. |
| ERC-1363 | `transferAndCall` / `approveAndCall` — receiver contracts react to transfers atomically | Clean, finalized standard but niche adoption; integrators don't expect it; adds surface without demand. Revisit only if a concrete integration needs it (it can't be added later — acceptable, wrappers exist). |
| ERC-3009 | `transferWithAuthorization` — USDC-style gasless *transfers* with random nonces | Payments-oriented; not in OZ core; only worthwhile if PEN becomes a payments rail. Account abstraction covers the UX need without token support. |
| ERC-2771 | Meta-transactions via a trusted forwarder baked into the token | Adds a *permanent* trusted-forwarder assumption to a credibly-neutral asset. Gasless UX is solved wallet-side (ERC-4337/7702) instead. |
| `ERC20Pausable`, blacklist, fee-on-transfer, rebasing | Transfer restrictions / supply games | Break DeFi integrations and tracker math; red flags for listing teams and sophisticated holders. The *vault* is pausable; the token never is. |
| ERC-777-style transfer hooks | Sender/receiver callbacks on transfer | Reentrancy-prone design; the standard is effectively dead. |
| Upgradeability (proxy) | Post-deployment logic changes | The single biggest credibility cost for a fixed-supply token. All operational flexibility lives in the vault (parameters + pause), never in the token. |

## The one exclusion that forecloses a future capability: ERC-7802 / SuperchainERC20

Because Base is an OP-stack chain, the recent **ERC-7802 (SuperchainERC20)** standard deserves an explicit decision rather than a silent default. It enables native (unwrapped) movement of a token between OP Superchain chains — but it works by granting `crosschainMint` / `crosschainBurn` rights to the SuperchainTokenBridge predeploy. That reintroduces exactly the mint authority this design eliminates, and since the token is immutable, **it cannot be added later**.

**Decision: excluded.** The no-mint guarantee is the more valuable property for an investor-facing fixed-supply token. If Superchain presence ever matters, a wrapped representation can be built on top without touching PEN itself.

## Resulting contract shape

```solidity
contract PEN is ERC20, ERC20Permit, ERC20Votes {
    constructor(address vault) ERC20("Pendulum", "PEN") ERC20Permit("Pendulum") {
        _mint(vault, MAX_ISSUANCE);
    }
    // + the small required overrides (_update, nonces) and,
    //   if timestamp mode is chosen, clock() / CLOCK_MODE()
}
```

Every behavioral line is OpenZeppelin's audited code — which is precisely the property that makes the token cheap to review (PRD §9) and easy for third parties to verify.
