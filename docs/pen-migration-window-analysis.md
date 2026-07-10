# PEN Migration: is a 6-month window enough?

| | |
|---|---|
| **Question** | Can the migration window be 6 months instead of 12, so attestor + monitoring infrastructure can be scaled down sooner? |
| **Answer** | **Yes — with ~1.5 months of margin, even at Pendulum's current degraded block time.** |
| **Data source** | Pendulum mainnet, live query at ~block 7,384,000 (2026-07-10) via `rpc-pendulum.prd.pendulumchain.tech` |

## TL;DR

The only thing that can *force* a longer window is genuinely time-locked
vesting, because everything else can be unlocked on demand. **All real vesting
completes in ~4.5 months at the current block rate**, so a 6-month window
clears every locked token with room to spare. The window length is really an
*adoption* question, not a lock question — and 6 months is a healthy buffer.

## The supply, and why locks are the only calendar constraint

Supply is **fixed at 149.93M PEN** (confirmed: no inflation). At the snapshot:

| Bucket | Amount | How it becomes migratable | Calendar-gated? |
|---|---|---|---|
| Freely transferable | ~110.45M | already is | no — day one |
| Vesting lock, **already vested** (stale) | ~23.9M | one `vest()` call | no — instant |
| Staked (`parachain-staking`) | ~3.75M | unstake, ~8h unbond at current block time | no — hours |
| Governance (democracy locks) | ~1.16M | remove vote / conviction expiry | holder-managed, short |
| Reserved (identity/proxy deposits) | ~0.0005M (484) | release the deposit | no — instant |
| **Vesting, genuinely still vesting** | **~11.87M** | **wait for the schedule** | **yes — the only real constraint** |

(Locks overlap — an account is frozen by the *max* of its locks, not the sum —
so the net non-transferable figure is ~39.48M, and the rows above are gross.)

Only the last row is time-locked. So the whole "6 vs 12 months" decision
reduces to: **how long until that ~11.87M of vesting finishes?**

## The vesting timeline — computed at the *actual* block time

Vesting releases **per block**, not per wall-clock second, so the answer
depends on block time. Pendulum's target is 12s, but it is **currently running
at a measured ~23.6s** (averaged over the last 10,000 blocks). We compute at
that slower, real rate so the argument is conservative — a faster block time
only shortens these numbers.

Still-vesting balance remaining, in wall-clock months at the measured ~23.6s:

| Horizon | Still vesting |
|---|---|
| +1 month | ~11.13M |
| +2 months | ~9.39M |
| +3 months | ~8.64M |
| +4 months | **~0.40M** |
| +5 months | **0** |
| +6 months | **0** |

The last real vesting schedule ends at **+505,375 blocks from now**, which is:

- **~4.5 months** at the current measured ~23.6s block time,
- ~3.8 months at 20s,
- ~2.3 months at the 12s target.

So even at today's degraded rate, **every vesting schedule finishes roughly
1.5 months before a 6-month window would close.** If block production recovers
toward target, the margin only grows. The block-time concern does not threaten
the conclusion across its entire plausible range (12s → 24s).

## The one permanent exception (irrelevant to 6 vs 12)

**30,000 PEN** sits in **6 vesting schedules of 5,000 PEN each with a
`u32::MAX` start block** — "never-starts" locks that don't vest under *any*
finite window. They are stranded whether the window is 6 months or 12. That's
0.02% of supply; handle those 6 accounts case-by-case (contact the holders, or
accept them as a permanent Pendulum-side residue). They are not an argument for
a longer window.

## The real constraint is adoption, not locks

Because locks clear in ~4.5 months, the window length is fundamentally about
giving **holders, exchanges, and custodians** time to *act* — unstake,
`vest()`, and migrate. A 6-month window is ~4.5 months of unlocking plus ~1.5
months of buffer, which is ample for a well-communicated migration.

And a 6-month target is **low-risk**, because the close is operational, not a
hard cliff:

- Set the vault's `earliestSweepTimestamp` to **~6 months**. It is immutable
  and marks the *earliest* the remainder can be swept, so 6 months is exactly
  what *preserves the option to wind down early*. Setting it to 12 would force
  the remainder to stay locked until then even if migration finishes fast.
- The actual close is a runbook, driven by migration progress: pause the
  pallet (stop new burns) → let in-flight migrations settle → sweep the
  remainder → shut down the 4 attestors + monitor.
- If adoption lags, you keep the infrastructure running longer — nothing forces
  you to stop at 6 months. You are setting a target, not signing a contract.

## Recommendation

- **Plan a 6-month window** and size the attestor + monitoring commitment to
  ~6 months.
- Set `earliestSweepTimestamp ≈ deploy + 6 months`.
- Actively manage the two non-lock items: a **comms plan** so holders migrate
  in time, and outreach to the **6 permanent-lock holders**.

Nothing in the on-chain lock data argues for 12 months.

## Caveats

- This is a live snapshot from a public RPC at one block; the vesting curve
  shifts slightly as schedules progress. Re-run against an internal node at
  deploy time for an exact, fresh figure — but the *shape* (all real vesting
  done by ~4.5 months at current block time) is stable.
- Numbers are rounded; treat them as decision-grade, not accounting-grade.
