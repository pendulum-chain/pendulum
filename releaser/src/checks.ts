/**
 * Pure predicates for the releaser.
 *
 * Isolated from the chain-client plumbing in `main.ts` so they can be unit
 * tested exhaustively (see checks.test.ts), mirroring `attestor/src/checks.ts`.
 */

import { BaseError, ContractFunctionRevertedError } from "viem";

/** What to do with a pending release whose `release()` attempt failed. */
export type ReleaseOutcome =
	/** Resolved on-chain — drop it from the pending set. */
	| "done"
	/** Blocked by a condition that heals on its own — keep retrying quietly. */
	| "retry"
	/** Blocked by a condition that CANNOT heal without a governance action. */
	| "blocked"
	/** Not a condition we model — alert a human. */
	| "unexpected";

/**
 * Classify a failed `release()` against the vault's revert set.
 *
 * The distinction that matters operationally is `retry` vs `blocked`:
 *
 *  - `ExceedsDailyCap` clears by itself as the rolling leaky bucket refills, so
 *    it is the normal, expected outcome of a launch-day backlog and must stay
 *    silent — it is precisely what this service exists to drain.
 *  - `ExceedsPerReleaseCap` NEVER clears on its own: the amount is above the
 *    per-release ceiling, so only a governance `setCaps` (behind the 48h
 *    timelock) can unblock it. Retrying is harmless but pointless, so it is
 *    surfaced instead of being swallowed.
 *  - `NonceAlreadyConsumed` means somebody else got there first (another
 *    releaser instance, or a conflicting tuple winning the nonce). Benign.
 */
export function contractErrorName(error: unknown): string | undefined {
	if (error instanceof ContractFunctionRevertedError) return error.data?.errorName;
	if (!(error instanceof BaseError)) return undefined;
	const reverted = error.walk((cause) => cause instanceof ContractFunctionRevertedError);
	return reverted instanceof ContractFunctionRevertedError ? reverted.data?.errorName : undefined;
}

export function classifyReleaseFailure(errorName: string | undefined): ReleaseOutcome {
	if (errorName === "NonceAlreadyConsumed") return "done";
	if (
		errorName === "ExceedsDailyCap" ||
		errorName === "EnforcedPause" ||
		errorName === "PendingFinality" ||
		// The threshold can drop below the quorum again if an attestor is
		// removed after ReleasePending was emitted; a replacement approving
		// restores it.
		errorName === "NotEnoughApprovals"
	) {
		return "retry";
	}
	if (
		errorName === "ExceedsPerReleaseCap" ||
		errorName === "InsufficientVaultBalance" ||
		// Synthetic: ExceedsDailyCap for an amount LARGER than dailyCap itself.
		// The allowance can never reach it, so this is a governance setCaps
		// matter, not a refill wait (the vault now rejects such cap pairs, but
		// one deployed before that may still carry one).
		errorName === "ExceedsDailyCapPermanently"
	) {
		return "blocked";
	}
	return "unexpected";
}

/** Divide work into bounded batches. Both Multicall calldata and individual
 * fallback concurrency use the same explicit limit. */
export function chunks<T>(items: T[], size: number): T[][] {
	if (!Number.isSafeInteger(size) || size <= 0) throw new Error(`invalid chunk size ${size}`);
	const result: T[][] = [];
	for (let start = 0; start < items.length; start += size) result.push(items.slice(start, start + size));
	return result;
}

/**
 * Convert the `tokenAmount` carried by `ReleasePending` back into the
 * `palletAmount` that `release()` expects.
 *
 * The vault derives `tokenAmount = palletAmount * conversionFactor`, so the
 * division is always exact. A non-exact result means the event and the vault's
 * conversion factor disagree — a data inconsistency we must not paper over by
 * silently truncating, because calling `release()` with a truncated amount
 * would compute a different payload hash and never match the approvals.
 */
export function toPalletAmount(tokenAmount: bigint, conversionFactor: bigint): bigint {
	if (conversionFactor <= 0n) throw new Error(`invalid conversionFactor ${conversionFactor}`);
	if (tokenAmount % conversionFactor !== 0n) {
		throw new Error(`tokenAmount ${tokenAmount} is not a multiple of conversionFactor ${conversionFactor}`);
	}
	return tokenAmount / conversionFactor;
}

/** How endpoints word a refused `eth_getLogs` span: Base's public endpoints
 *  (`block range greater than 200 max`), geth and reth (`exceed(s) maximum
 *  block range`), and hosted providers (`limited to a 10,000 range`, `ranges
 *  over 10000 blocks`, `query returned more than 10000 results`, `Log response
 *  size exceeded`). */
const RANGE_TOO_LARGE = /\branges?\b|more than [\d,]+ results|response size/i;

/** True when an endpoint refused an `eth_getLogs` call for the size of its
 *  block range (or of its answer), which a smaller range avoids. */
export function isRangeTooLarge(error: unknown): boolean {
	let depth = 0;
	for (let cause: unknown = error; cause && depth < 10; cause = (cause as { cause?: unknown }).cause, depth++) {
		for (const key of ["details", "shortMessage", "message"] as const) {
			const text = (cause as Record<string, unknown>)[key];
			if (typeof text === "string" && RANGE_TOO_LARGE.test(text)) return true;
		}
	}
	return false;
}

/** Full-size reads in a row after which a reduced span is tried at twice its
 *  size again. */
const GROW_AFTER_READS = 10;

/**
 * Read [fromBlock, toBlock] in order, in consecutive ranges of at most
 * `span.size` blocks. Public Base RPCs cap `eth_getLogs` ranges, some far below
 * any sensible default (Base Sepolia's at 200 blocks), so a range the endpoint
 * refuses as too large is read again at half the size, down to one block. After
 * `GROW_AFTER_READS` full-size reads in a row succeed, the size doubles again,
 * never above `span.max`. `span` persists across calls; any other failure, or
 * a refused single block, propagates.
 */
export async function readInRanges(
	fromBlock: bigint,
	toBlock: bigint,
	span: { size: bigint; max: bigint; streak: number },
	read: (start: bigint, end: bigint) => Promise<void>,
): Promise<void> {
	if (span.size <= 0n) throw new Error(`invalid block range size ${span.size}`);
	for (let start = fromBlock; start <= toBlock; ) {
		const end = start + span.size - 1n < toBlock ? start + span.size - 1n : toBlock;
		try {
			await read(start, end);
		} catch (error) {
			if (span.size === 1n || !isRangeTooLarge(error)) throw error;
			span.size /= 2n;
			span.streak = 0;
			continue;
		}
		// Only a read of the full size shows that the size is accepted.
		if (end - start + 1n === span.size && ++span.streak >= GROW_AFTER_READS) {
			span.size = span.size * 2n < span.max ? span.size * 2n : span.max;
			span.streak = 0;
		}
		start = end + 1n;
	}
}
