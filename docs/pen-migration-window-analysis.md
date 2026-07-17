# PEN Migration: the 3-month window — analysis and conditions

| | |
|---|---|
| **Decision** | Migration window target: **3 months**. Block time will be improved toward the 12s target; if any vesting hasn't finished by close, a **referendum force-unlocks the rest**. |
| **Verdict of this analysis** | Workable — but *conditional*. At the 12s target, all vesting finishes in ~2.3 months (fits). At today's measured ~23.6s it takes ~4.5 months (does not fit). The deciding variable is when the block-time fix lands; the referendum is the safety net that makes the plan sound either way. |
| **Data source** | Pendulum mainnet, live query at ~block 7,384,000 (2026-07-10) via `rpc-pendulum.prd.pendulumchain.tech` |

## Why the window length is a vesting question

Live Pendulum issuance is **fixed at ~149.93M PEN** (no inflation, confirmed;
the Base token's clean 150M includes a ~67k rounding delta that stays in the
vault and goes to the treasury at the sweep — PRD D3). Every locked
bucket except one can be freed *on demand*, with no calendar dependency:

| Bucket | Amount | How it becomes migratable | Calendar-gated? |
|---|---|---|---|
| Freely transferable | ~110.45M | already is | no — day one |
| Vesting lock, **already vested** (stale) | ~23.9M | one `vest()` call | no — instant |
| Staked (`parachain-staking`) | ~3.75M | unstake, 2-round unbond (hours) | no |
| Governance (democracy locks) | ~1.16M | remove vote / conviction expiry | holder-managed, short |
| Reserved (identity/proxy deposits) | ~484 PEN | release the deposit | no — instant |
| **Vesting, genuinely still vesting** | **~11.87M** | **wait for the schedule (per block)** | **yes — the only real constraint** |

So "is 3 months enough?" reduces to: does the ~11.87M of genuine vesting
finish within 3 months?

## The block-time dependency, quantified

Vesting releases **per block**, so wall-clock completion depends directly on
block time. The last real schedule completes **505,375 blocks** from the
snapshot. That is:

| Average block time over the window | Vesting completes in | Fits 3 months? |
|---|---|---|
| 12s (target) | **~2.3 months** | yes, ~0.7 months margin |
| ~15.6s | ~3.0 months | exactly the break-even |
| 20s | ~3.8 months | no |
| ~23.6s (measured today, 10k-block avg) | ~4.5 months | no |

Two concrete planning numbers fall out of this:

- **Break-even: the window-average block time must be ≤ ~15.6s.**
- **If the chain jumps from today's ~23.6s straight to 12s, the fix must be
  live within ~6 weeks of the window opening** for vesting to finish inside
  3 months (each week at the slow rate consumes roughly half a week of the
  margin).

So: improve block time *early* in the window, not toward the end.

## The fallback that makes 3 months safe anyway

If block production doesn't recover fast enough, some residue of the 11.87M is
still vesting at close. This is covered — with on-chain machinery that
**already exists in this runtime**:

- The `vesting-manager` pallet exposes a **root-gated
  `remove_vesting_schedule(who, index)`**. A referendum (or the same
  governance track that authorizes the window close) can remove the remaining
  schedules, which unlocks the tokens immediately; holders then migrate
  normally before the final sweep.
- The same mechanism cleanly handles the **~30,000 PEN in 6 permanent
  "never-starts" schedules** (`u32::MAX` start block) that would otherwise be
  stranded under *any* finite window — 3, 6, or 12 months alike. These 6
  accounts need the referendum (or case-by-case outreach) regardless of the
  window length, so they are not an argument for a longer window.

And structurally, the close is **operational, not a hard cliff**: the sequence
is pause the pallet → settle in-flight migrations → reconcile → sweep
(runbook RB-7). If adoption or vesting lags, the infrastructure simply runs a
few weeks longer — a 3-month *target* with the option to extend, not a
contract.

## What the shorter window changes operationally

1. **`earliestSweepTimestamp` ≈ deploy + 3 months.** It is immutable and marks
   the *earliest* allowed sweep — setting it at 3 months preserves the option
   to wind down on schedule while never forcing it.
2. **Daily-cap throughput now matters.** Migrating ~150M PEN within ~90 days
   needs an *average* release throughput of ~1.7M PEN/day. The PRD's
   initial-cap guidance (~1–2% of vault per day = 1.5–3M/day) is compatible,
   but the deliberately conservative soft-launch caps must be **raised
   promptly via governance** once the launch is verified — cap raises are on
   the critical path of a 3-month plan in a way they weren't at 6–12 months.
3. **Comms compress.** Holders' checklist (call `vest()`, unstake ~hours,
   remove governance votes, migrate) is quick per holder, but exchanges and
   passive holders need the announcement, reminders, and deadline pressure
   inside a much shorter arc. The ~23.9M of *already-vested-but-stale* locks
   (holders who never called `vest()`) is the strongest evidence that passive
   holders exist and need active prodding.
4. **Referendum lead time counts against the window.** A Pendulum referendum
   has voting + enactment periods (weeks). If block time hasn't recovered by
   ~month 2, *start the unlock referendum then* — don't wait until the window
   ends to begin a multi-week governance process.

## Recommendation (under the 3-month decision)

- Set `earliestSweepTimestamp ≈ deploy + 3 months`.
- Land the block-time improvement **within the first ~6 weeks** of the window;
  track window-average block time against the ~15.6s break-even.
- Pre-draft the vesting-unlock referendum so it can be submitted at ~month 2
  if vesting is projected to overrun — it is also the vehicle for the 30k
  sentinel-lock tail either way.
- Verify launch caps quickly and raise `dailyCap` early; ~1.7M PEN/day average
  throughput is required arithmetic, not an optimization.
- Size the attestor + monitoring commitment to ~3 months with a soft option to
  extend a few weeks.

## Caveats

- Live snapshot from a public RPC at one block; re-run against an internal
  node at deploy time (the shape is stable, exact figures drift as schedules
  progress).
- Block-time projections assume the improvement is a step change to ~12s; a
  gradual ramp lands between the table rows. The break-even framing
  (window-average ≤ ~15.6s) is the robust way to track it.
- Numbers are decision-grade, not accounting-grade.
