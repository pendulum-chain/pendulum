/**
 * Pure predicates for the releaser.
 *
 * Isolated from the chain-client plumbing in `main.ts` so they can be unit
 * tested exhaustively (see checks.test.ts), mirroring `attestor/src/checks.ts`
 * and `monitor/src/checks.ts`.
 */

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
export function classifyReleaseFailure(reason: string): ReleaseOutcome {
	if (reason.includes("NonceAlreadyConsumed")) return "done";
	if (
		reason.includes("ExceedsDailyCap") ||
		reason.includes("EnforcedPause") ||
		reason.includes("InsufficientVaultBalance") ||
		// The threshold can drop below the quorum again if an attestor is
		// removed after ReleasePending was emitted; a replacement approving
		// restores it.
		reason.includes("NotEnoughApprovals")
	) {
		return "retry";
	}
	if (reason.includes("ExceedsPerReleaseCap")) return "blocked";
	return "unexpected";
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

/**
 * Split [fromBlock, toBlock] into chunks no larger than `maxRange` blocks.
 *
 * Public Base RPCs cap `eth_getLogs` ranges; a releaser restarting after a long
 * outage would otherwise request a span the provider rejects and make no
 * progress at all. Returns [] when there is nothing new to scan.
 */
export function blockRanges(fromBlock: bigint, toBlock: bigint, maxRange: bigint): Array<[bigint, bigint]> {
	if (maxRange <= 0n) throw new Error(`invalid maxRange ${maxRange}`);
	const ranges: Array<[bigint, bigint]> = [];
	for (let start = fromBlock; start <= toBlock; start += maxRange) {
		const end = start + maxRange - 1n;
		ranges.push([start, end > toBlock ? toBlock : end]);
	}
	return ranges;
}
