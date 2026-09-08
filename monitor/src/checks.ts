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

/**
 * A Base event whose nonce is at or beyond the source node's next nonce may be
 * legitimate data observed through a temporarily faster RPC. Give that source
 * view a bounded window to catch up. Older missing nonces cannot be explained
 * by node lag and must fail immediately.
 */
export function shouldAwaitSource(
	eventNonce: bigint,
	nextExpectedNonce: bigint,
	firstSeenMs: number,
	nowMs: number,
	graceSeconds: number,
): boolean {
	return eventNonce >= nextExpectedNonce && !isStale(firstSeenMs, nowMs, graceSeconds);
}

/**
 * A Base event is PROVABLY without a Pendulum source once the monitor's
 * finalized source view has passed the wall-clock moment the monitor FIRST
 * OBSERVED the event (plus a clock-skew margin) and the nonce still does not
 * exist.
 *
 * Why this is sound: a legitimate release's burn is relay-FINALIZED strictly
 * before any attestor submits an approval, so the burn block's timestamp
 * precedes the real time at which the monitor could first observe that
 * approval on Base. Substrate timestamps are strictly monotone, so a finalized
 * head whose timestamp is past that observation time by more than any collator
 * clock drift already contains every block the burn could live in. If the
 * nonce is still unknown then, no amount of further waiting can reveal it —
 * the event is fabricated, and the auto-pause must not sit out the RPC-lag
 * grace period.
 *
 * The anchor is the monitor's own clock, deliberately NOT the Base block
 * timestamp: OP-stack L2 block timestamps trail real time by the length of any
 * sequencer outage while it catches up, and an artificially old event
 * timestamp would let a merely-lagging source "prove" a legitimate event
 * fabricated (review round 9). A lagging source never satisfies this predicate
 * (its head timestamp trails the observation), and a future-dated source view
 * (a collator clock ahead of real time) proves nothing either and is excluded,
 * so neither can turn node lag into a false pause.
 */
export function provablyUnsourced(
	firstSeenMs: number,
	sourceFinalizedTsMs: number,
	nowMs: number,
	skewMarginMs: number,
): boolean {
	if (sourceFinalizedTsMs > nowMs + skewMarginMs) return false;
	return sourceFinalizedTsMs >= firstSeenMs + skewMarginMs;
}

export interface MigrationTuple {
	nonce: bigint;
	recipient: string;
	palletAmount: bigint;
}

function sameAddress(left: string, right: string): boolean {
	return left.toLowerCase() === right.toLowerCase();
}

export function approvalMismatch(expected: MigrationTuple | undefined, actual: MigrationTuple): string | undefined {
	if (!expected) return `nonce ${actual.nonce} has no finalized Pendulum migration`;
	if (!sameAddress(expected.recipient, actual.recipient)) {
		return `nonce ${actual.nonce} recipient ${actual.recipient} != finalized ${expected.recipient}`;
	}
	if (expected.palletAmount !== actual.palletAmount) {
		return `nonce ${actual.nonce} pallet amount ${actual.palletAmount} != finalized ${expected.palletAmount}`;
	}
	return undefined;
}

export function releaseMismatch(
	expected: MigrationTuple | undefined,
	actual: MigrationTuple & { tokenAmount: bigint },
	conversionFactor: bigint,
): string | undefined {
	const tupleMismatch = approvalMismatch(expected, actual);
	if (tupleMismatch) return tupleMismatch;
	const expectedTokenAmount = actual.palletAmount * conversionFactor;
	if (actual.tokenAmount !== expectedTokenAmount) {
		return `nonce ${actual.nonce} token amount ${actual.tokenAmount} != converted ${expectedTokenAmount}`;
	}
	return undefined;
}

export function blockRanges(fromBlock: bigint, toBlock: bigint, maxRange: bigint): Array<[bigint, bigint]> {
	if (maxRange <= 0n) throw new Error(`invalid maxRange ${maxRange}`);
	const result: Array<[bigint, bigint]> = [];
	for (let start = fromBlock; start <= toBlock; start += maxRange) {
		const end = start + maxRange - 1n;
		result.push([start, end > toBlock ? toBlock : end]);
	}
	return result;
}

export function chunks<T>(items: T[], size: number): T[][] {
	if (!Number.isSafeInteger(size) || size <= 0) throw new Error(`invalid chunk size ${size}`);
	const result: T[][] = [];
	for (let start = 0; start < items.length; start += size) result.push(items.slice(start, start + size));
	return result;
}
