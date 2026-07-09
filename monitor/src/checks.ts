/**
 * Pure conservation/liveness predicates for the invariant monitor.
 *
 * These are isolated from the chain-client plumbing in `main.ts` so they can be
 * unit-tested exhaustively (see checks.test.ts). Every subtlety that has bitten
 * a review round lives here.
 */

/**
 * (M2a) Nothing may leave the vault that was not burned on Pendulum.
 *
 * `totalReleased` (Base) must never exceed the burned total converted to token
 * units. It lags `totalMigrated` only via finality delay, never the other way,
 * because a release requires attestations of an already-finalized burn — so any
 * excess is the strongest possible signal of attestor compromise.
 */
export function releasedExceedsMigrated(
	totalReleased: bigint,
	totalMigrated: bigint,
	conversionFactor: bigint,
): boolean {
	return totalReleased > totalMigrated * conversionFactor;
}

/**
 * (M2b) Vault-internal conservation.
 *
 * Under all legitimate contract logic `balance + released + swept` is *exactly*
 * `totalSupply`: every path that removes tokens bumps a counter in lockstep
 * (`_release`→`totalReleased`, `sweepRemainder`→`totalSwept`). Real loss can
 * therefore only ever show up as a DEFICIT.
 *
 * A SURPLUS (sum > totalSupply) is harmless and must NOT trip the check: it can
 * be produced permissionlessly by anyone transferring PEN into the vault (a
 * plain donation, or a migration whose recipient is the vault address). Treating
 * a surplus as a violation would let a single dust transfer pause the vault on
 * every poll — unrecoverable until the window-close sweep, and it would train
 * on-call to ignore the highest-severity alert. Hence the strict `<`.
 */
export function vaultConservationDeficit(
	vaultBalance: bigint,
	totalReleased: bigint,
	totalSwept: bigint,
	totalSupply: bigint,
): boolean {
	return vaultBalance + totalReleased + totalSwept < totalSupply;
}

/**
 * (M4) Liveness: a migration the monitor has known about for longer than the
 * grace period, and which is still not consumed on Base, is stalled.
 */
export function isStale(firstSeenMs: number, nowMs: number, graceSeconds: number): boolean {
	return nowMs - firstSeenMs > graceSeconds * 1000;
}
