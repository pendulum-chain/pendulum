# PEN → Base Migration: Community Overview

This document accompanies the community discussion post about migrating PEN
from the Pendulum parachain to Base. It explains the working design in plain
language so holders can evaluate it. **It is not a governance proposal** — the
final parameters (migration window, attestor operators, caps, guardian, quorum
mechanics, end-of-window policy) will be fixed in a formal proposal after the
community discussion.

For the full engineering specification, see the
[technical PRD](pen-base-migration-prd.md). Deeper companions:
[governance guide](pen-governance-guide.md),
[migration-window analysis](pen-migration-window-analysis.md),
[approach rationale (ADR-001)](adr-001-pen-base-migration-approach.md).

## The design in one paragraph

PEN becomes a **fixed-supply ERC-20 on Base**: exactly 150,000,000 PEN,
18 decimals, with the **entire supply minted exactly once at deployment** into
a migration vault. The token
has **no mint function, no owner authority, and no upgradeable proxy** — its
supply can never be increased by anyone. Holders migrate one-way: transferable
PEN is removed from circulation on Pendulum (the working design burns it), and
the vault on Base releases the same amount to the holder's Base address after
independent verification. There is no Base→Pendulum path.

## What happens when you migrate

1. You call the migration function on Pendulum and provide your Base (EVM)
   address.
2. Your transferable PEN is removed from Pendulum circulation, and Pendulum
   emits a migration event with a unique ID, your Base address, and the amount.
3. After Pendulum reaches relay-chain finality, each attestor service
   independently observes the event from its own node.
4. When the Base vault has **three matching approvals** for the identical
   (ID, address, amount), it releases your PEN on Base.

End to end this normally completes within minutes of finality. **The action is
irreversible** — the UI enforces address checksum validation, warns when the
destination is a smart contract, requires an explicit confirmation, and
recommends a small test migration for large amounts.

**Only freely transferable PEN can migrate.** Staked, vesting, locked, or
reserved balances must first be freed (unstake, claim vested tokens, remove
governance votes). The UI shows your locked balance and what to do about it.

## Your amount does not change

Moving from 12 decimals (Pendulum) to 18 decimals (Base) is an exact technical
conversion of base units by 10⁶. **1 PEN on Pendulum = 1 PEN on Base.** Your
amount and your share of supply are unchanged.

## Supply transparency — and why exactly 150 million

- `totalSupply()` on Base equals the full maximum supply from day one.
- The vault's balance is **excluded from circulating supply** — only migrated
  tokens count as circulating.
- Every release is publicly verifiable on Base against a finalized Pendulum
  burn; an independent monitor continuously checks that
  `vault balance + released = total supply` and that nothing was ever released
  without a matching burn.

One detail we want to state explicitly rather than have discovered later:
Pendulum's live on-chain issuance today is slightly **below** 150 million
(~149.93M) — an untidy artifact of the chain's history (fee burns and similar),
not a meaningful tokenomics figure, and one that keeps drifting slightly as
fees continue to be burned. The Base token is deliberately set to a **clean,
canonical 150,000,000**, which is the right constant for trackers,
integrations, and an immutable token contract.

What happens to the difference (~67,000 PEN, about 0.045% of supply):

- It **cannot be released by the migration** — releases require a matching
  burn on Pendulum, and no burns can ever exist for tokens that were never in
  circulation there. It sits inert in the vault.
- It is **excluded from circulating supply** for the entire migration.
- At window close it moves — together with any unmigrated remainder — to the
  **community treasury**, via the same governed, timelocked sweep. It is not
  allocated to the team or any individual; only a public governance decision
  can ever spend it.

Net effect: every holder's conversion stays exactly 1:1, and the rounding
delta ends up under community control rather than as a strange decimal baked
into the token forever.

## Security model, stated honestly

Base cannot cryptographically verify Pendulum state (that would require an
on-chain Polkadot light client — a multi-year effort). For a finite, one-way
migration the design instead uses a **3-of-4 attestor model with strict damage
limits**:

- Each attestor watches finalized Pendulum events **from its own node**, so no
  single faulty or malicious RPC node can feed all attestors wrong data.
- A release needs **3 of 4** attestors to approve the identical migration
  tuple; each key is isolated on separate infrastructure.
- The attestors may initially be **team-operated** (Pendulum currently has no
  external node operators). **This is a meaningful trust trade-off, not a claim
  of trustlessness.**

Because operator independence is limited at the start, the protections that
actually carry the security are:

- **Rate caps** — a per-release maximum and a rolling 24-hour cap on total
  releases, so even a full compromise of the attestor set is limited to a
  pre-agreed daily amount before it can be stopped.
- **A fast pause guardian** — a separate Safe (held by people who do not hold
  attestor keys) that can freeze all releases in a single transaction.
- **A ≥48-hour timelock** on every sensitive change: unpausing, cap changes,
  attestor changes, and any movement of unmigrated supply. Nothing sensitive
  can happen silently or instantly.
- **Independent monitoring** — a watchdog on separate infrastructure that
  verifies every release against a finalized Pendulum event and alerts (and
  can auto-pause) on any inconsistency or attestor outage.

The final proposal will publish the concrete parameters: the attestor set and
its independence arrangements, the cap values, the guardian Safe and its
threshold, and the monitoring setup.

## The migration window

The window's length is one of the main questions of the community discussion.
Two facts frame it:

- **An earliest close date is not an automatic sweep.** No unmigrated PEN
  moves just because the window elapsed. Moving any remainder (to a
  treasury-controlled address, a burn, or another approved path) requires a
  separate governance decision and a timelocked execution — and extending the
  window is always an option.
- **On-chain locks do not force a long window.** Analysis of live chain state
  ([details](pen-migration-window-analysis.md)) shows almost all genuinely
  time-locked (vesting) PEN unlocks within a few months; a small residue can
  be force-unlocked by referendum if needed. Most "locked" balances (staking,
  already-vested tokens, governance votes) can be freed by their holders at
  any time within hours.

## Governance after migration

The intended model is hybrid:

- **On-chain (binding):** an OpenZeppelin Governor + Timelock on Base controls
  the Base-side contracts and treasury; visible and votable via Tally.
- **Off-chain (signaling):** Snapshot for broader community decisions, executed
  by an elected Safe.
- **Pendulum side:** the existing technical committee retains narrowly-scoped
  authority (security patches, emergency actions) while the chain runs — with
  no discretionary control over Base-side PEN or its treasury.

A hard design requirement: **the migration vault must neither vote nor make
quorum unreachable.** Quorum is defined against circulating supply with the
vault explicitly excluded, in both Snapshot and on-chain governance. See the
[governance guide](pen-governance-guide.md) for worked examples of both tracks.

## What this migration does NOT do

- No two-way bridge between Base and Pendulum.
- No change to the 1:1 conversion, the maximum supply, or any holder's share.
- No migration of non-PEN assets (Spacewalk-wrapped and XCM assets are
  unaffected).
- No automatic migration of staked/locked/vesting balances, and no automatic
  migration of exchange-held PEN — unless an exchange announces a supported
  process, expect to withdraw to self-custody and use the public migration
  route.
- No immediate shutdown of the Pendulum chain (its longer-term operating model
  is a separate discussion).
- No new centralized-exchange listing funded as part of the migration, and no
  new PEN staking program on Base.

## Rollout, in order

1. Community discussion (the post this document accompanies).
2. Team response + formal governance proposal fixing the final parameters.
3. Testnet deployment and end-to-end testing (Foucoco + Base Sepolia).
4. Security reviews, key-management ceremonies, monitoring setup, and
   operational drills.
5. Mainnet soft launch with conservative caps (team + invited large holders).
6. Public launch of the migration UI; caps raised via governance.
7. Tracker updates (total vs. circulating supply methodology).
8. Admin handover to token holders: during launch, the vault's settings
   (caps, attestor set, pause/unpause) are administered by a team multisig so
   the system can be wired and verified quickly. As the final step, that admin
   power is transferred to the community governance structure — the
   token-holder Governor behind the 48-hour timelock — after which no
   sensitive parameter can change without a public on-chain vote.

Nothing is irreversible until the formal proposal is approved and the reviewed
contracts and operational setup are live.
