# PEN Governance After the Base Migration — How It Works

| | |
|---|---|
| **Status** | Explainer (companion to the PRD/ADR) |
| **Audience** | PEN holders, contributors, prospective voters |
| **Related** | [PRD §6.7 / G1–G5](pen-base-migration-prd.md), [ADR-001 governance section](adr-001-pen-base-migration-approach.md), `contracts/src/PENGovernor.sol` |

This is a plain-language walkthrough of the hybrid governance model, with two
real-world examples. It describes *how a decision gets made and executed* after
PEN has migrated to Base.

## The three organs

The whole model rests on one idea: **a decision routes to the organ that can
actually execute it.**

- **PEN holders** are the electorate. Voting power is the delegated
  `ERC20Votes` balance of PEN on Base. One practical catch: a holder has **zero
  voting power until they delegate** (to themselves or a representative).
  Holding tokens isn't voting; delegating is.
- **The on-chain track** — `PENGovernor` + a `TimelockController` — handles
  anything that is a deterministic call to a Base contract the timelock
  controls: the `MigrationVault` parameters (caps, attestor set, threshold,
  guardian, remainder sweep) and any Base-side treasury the timelock owns.
  Binding and trustless: nothing but a passed vote can move it.
- **The off-chain track** — Snapshot + an elected executor Safe — handles
  decisions that aren't a single on-chain call: discretionary, multi-step, or
  living off Base (including on the Pendulum parachain). Snapshot signals
  intent gaslessly; the Safe carries it out.

A **guardian Safe** (instant pause, no vote) and the **Pendulum technical
committee** (runtime and security actions on the parachain) sit outside the
token vote — they exist because some actions must be fast, or must run on a
chain the Base Governor can't reach.

```mermaid
flowchart TD
    H["PEN holders<br/><i>delegated voting power</i>"]
    H --> A["<b>On-chain track · binding</b>"]
    H --> B["<b>Off-chain track · signaled</b>"]

    A --> A1["Propose<br/><i>target: vault.setCaps(…)</i>"]
    A1 --> A2["Vote · 5 days<br/><i>quorum + majority</i>"]
    A2 --> A3["Queue → timelock<br/><i>48-hour public delay</i>"]
    A3 --> A4["Execute<br/><i>timelock calls the vault</i>"]
    A4 --> AX(["Example 1 · raise the daily cap"])

    B --> B1["Snapshot proposal<br/><i>gasless · vault excluded</i>"]
    B1 --> B2["Vote · ~7 days<br/><i>PEN-on-Base holders sign</i>"]
    B2 --> B3["Executor Safe acts<br/><i>elected multisig</i>"]
    B3 --> BX(["Example 2 · fund a liquidity program"])

    G["Guardian Safe — emergency pause, no vote"]
    T["Pendulum committee — runtime & security"]
```

The Governor's timing parameters are the deployment defaults and are themselves
governable: voting delay ~1 day, voting period 5 days, timelock delay 48 hours,
quorum a low fraction of supply at launch (raised as circulating supply grows),
plus a proposal threshold of voting power required to open a proposal.

## Example 1 — raising the migration vault's daily cap (on-chain track)

**The situation:** migration is live with deliberately conservative caps.
Volume picks up and legitimate migrations start getting deferred by the rolling
daily cap. The community wants to raise `dailyCap` and `perReleaseCap`. This
belongs on the on-chain track because it is exactly one deterministic action —
a call to `vault.setCaps(...)` — and the timelock is the vault's admin, so no
human needs discretion or custody.

**How it plays out:**

1. A delegate holding at least the proposal threshold of voting power calls
   `propose(...)` with a single action: target `MigrationVault`, calldata
   `setCaps(newPerRelease, newDaily)`, and a human-readable description. The
   proposal appears on Tally in the pending state.
2. After the voting delay (~1 day — this is also when the voting-power snapshot
   is taken, so buying tokens afterward can't influence the vote), voting opens
   for five days. Delegates cast for, against, or abstain. To pass, the
   proposal needs quorum and more for than against.
3. On success, anyone calls `queue(...)`, scheduling the action inside the
   `TimelockController` behind a 48-hour delay. This delay is the safety valve:
   for two days the exact pending change is public, and if it looks wrong the
   guardian can pause the vault while the community reacts.
4. After the 48 hours, anyone calls `execute(...)`. The timelock — as the
   vault's admin — makes the `setCaps` call. The cap is now raised.

Start to finish is roughly **eight days**, and at no point does a trusted party
decide anything. The timelock is the only address that can call `setCaps`, and
it only ever acts on a vote that already passed. Attestor rotation, threshold
changes, and the end-of-window remainder sweep all run through this identical
path.

## Example 2 — funding a liquidity and market-making program (off-chain track)

**The situation:** the DAO wants to bootstrap PEN/USDC liquidity and retain a
market maker for six months, funded with, say, 2,000,000 PEN from the community
treasury. This does not reduce to one on-chain call — it means choosing a
market maker, negotiating terms, moving funds (possibly across venues or
chains), and exercising judgment over six months. That is what the off-chain
track is for.

**How it plays out:**

1. A holder posts the proposal to the forum for discussion, then creates a
   Snapshot proposal. The voting strategy reads PEN balances on Base at a
   snapshot block, with the `MigrationVault` address **excluded** — so the
   large unmigrated balance in the vault can't vote, and quorum is measured
   against circulating supply rather than total supply.
2. Voting runs for roughly a week and is **gasless**: holders sign messages,
   they don't pay gas or even need to delegate. Choices can be a simple
   for/against or several funding options.
3. If it passes, the elected executor Safe carries out the mandate — transfers
   the PEN, contracts the market maker, and manages the engagement over its
   lifetime.

The honest trade-off: Snapshot itself is a *signal*, not an on-chain
instruction, so this track trusts the Safe signers to honor the result. That is
why the signers are elected and why, if you want to harden it, **oSnap** (UMA's
optimistic oracle) can post the Snapshot outcome on-chain so that — if
unchallenged — it becomes directly executable by the Safe, turning "the Safe
should comply" into an economic guarantee rather than a social one.

## The routing rule, and three things that trip people up

The whole model collapses to one question: **can the decision be expressed as a
deterministic call to a Base contract the timelock owns?** If yes, it goes
on-chain and executes trustlessly (Example 1). If it needs discretion, multiple
steps, or lives off Base — including anything on the Pendulum parachain, which
the Base Governor cannot reach — it goes to Snapshot and the Safe (Example 2),
or for runtime and security matters, to the Pendulum committee.

Three practical notes that matter in real use:

- **Delegation is a prerequisite for the on-chain track.** A holder with a
  large balance but no delegation has no voting power and can't even meet the
  proposal threshold. This surprises people constantly; launch communications
  should make "delegate to yourself" a first-class step.
- **Emergencies deliberately skip governance.** The guardian Safe can pause the
  vault in one transaction, with no vote, precisely because a 48-hour timelock
  is the wrong tool for an active incident. Governance then decides the actual
  fix through the slow, deliberate path. The guardian is intentionally not the
  Governor.
- **Quorum is tuned for a migration in progress.** Because the vault holds most
  of the supply early on, on-chain quorum is set to a low fraction at launch
  and raised by governance as circulating supply grows, and Snapshot excludes
  the vault outright. Otherwise quorum measured against total supply would be
  unreachable in the early months.
