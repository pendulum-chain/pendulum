/**
 * The releaser's release attempts: simulate and send `release()` for each
 * pending entry, follow a sent transaction until it is mined (re-sent with
 * higher fees at its nonce when it is not), and classify what a failed attempt
 * means. `main.ts` injects the clients and the pending set, so the decisions
 * here are unit tested (see release.test.ts).
 */

import {
	BaseError,
	formatGwei,
	TransactionReceiptNotFoundError,
	type Account,
	type Chain,
	type Hash,
	type PublicClient,
	type TransactionReceipt,
	type Transport,
	type WalletClient,
} from "viem";
import type { Severity } from "./alert.js";
import { classifyReleaseFailure, contractErrorName } from "./checks.js";
import { vaultAbi } from "./vaultAbi.js";

/** op-geth replaces a pending transaction only when the new one raises both
 *  its fee cap and its tip by at least this many percent. */
export const MIN_REPLACEMENT_PERCENT = 10;

export interface PendingRelease {
	nonce: bigint;
	recipient: `0x${string}`;
	palletAmount: bigint;
}

export interface Fees {
	maxFeePerGas: bigint;
	maxPriorityFeePerGas: bigint;
}

/** When, how often and how far a release transaction that is not mined is
 *  re-sent with higher fees. */
export interface FeeBumpPolicy {
	/** Re-send an attempt still not mined after this long. */
	afterMs: number;
	/** Raise both fees by at least this many percent per re-send. */
	percent: number;
	/** Re-sends per transaction. */
	max: number;
	/** No attempt ever offers a higher maxFeePerGas. */
	maxFeePerGas: bigint;
}

/** A release transaction sent and not seen mined: the identical call, kept at
 *  its nonce and re-sent there with higher fees until an attempt is mined. */
export interface InFlightRelease {
	entry: PendingRelease;
	call: {
		account: Account;
		address: `0x${string}`;
		abi: typeof vaultAbi;
		functionName: "release";
		args: readonly [bigint, `0x${string}`, bigint];
		gas: bigint;
	};
	nonce: number;
	/** The latest attempt's fees. */
	fees: Fees;
	/** Every attempt at `nonce`; any of them may be the one that is mined. */
	hashes: Hash[];
	/** When the latest attempt was sent. */
	sentAtMs: number;
	bumps: number;
}

export interface Releaser {
	publicClient: PublicClient;
	walletClient: WalletClient<Transport, Chain, Account>;
	account: Account;
	vaultAddress: `0x${string}`;
	baseFinalityTag: "safe" | "finalized";
	blockedAlertIntervalMs: number;
	/** nonce -> pending release. Keyed by nonce: the vault consumes a nonce once. */
	pending: Map<bigint, PendingRelease>;
	/** nonce -> the last page about it, and when it went out. A blocked release
	 *  needs a timelocked governance action (>= 48h) to clear, and an unexpected
	 *  failure repeats every poll as well, so re-paging either every poll would
	 *  emit thousands of identical alerts and train on-call to ignore them; the
	 *  same page re-alerts at a bounded interval instead. */
	alertedAt: Map<bigint, { subject: string; at: number }>;
	feeBump: FeeBumpPolicy;
	/** The release transaction not seen mined yet, if any. Kept in memory
	 *  only. */
	inFlight?: InFlightRelease;
	alert(subject: string, detail: string, severity?: Severity): Promise<void>;
	log(message: string): void;
	now(): number;
}

/** Page `subject` about the release of `nonce`, unless the same page about it
 *  went out less than `blockedAlertIntervalMs` ago. */
async function pageThrottled(releaser: Releaser, nonce: bigint, subject: string, detail: string): Promise<void> {
	const last = releaser.alertedAt.get(nonce);
	if (last?.subject === subject && releaser.now() - last.at < releaser.blockedAlertIntervalMs) return;
	releaser.alertedAt.set(nonce, { subject, at: releaser.now() });
	await releaser.alert(subject, detail, "critical");
}

const min = (a: bigint, b: bigint) => (a < b ? a : b);
const max = (a: bigint, b: bigint) => (a > b ? a : b);

/** `fee` raised by `percent`, rounded up and by at least 1 wei. */
function raised(fee: bigint, percent: number): bigint {
	return max(fee + 1n, (fee * BigInt(100 + percent) + 99n) / 100n);
}

/** The first attempt's fees: the node's estimate, never above the cap. */
export function cappedFees(estimate: Fees, cap: bigint): Fees {
	return { maxFeePerGas: min(estimate.maxFeePerGas, cap), maxPriorityFeePerGas: min(estimate.maxPriorityFeePerGas, cap) };
}

/** The fees to re-send an attempt that was not mined with: both raised by the
 *  policy's percent, and at least to the node's current estimate, never above
 *  the cap. Undefined when the cap leaves no room for a replacement the pool
 *  accepts (both fees raised by at least `MIN_REPLACEMENT_PERCENT`). */
export function bumpedFees(last: Fees, estimate: Fees, policy: FeeBumpPolicy): Fees | undefined {
	const next = (fee: bigint, current: bigint) => min(policy.maxFeePerGas, max(raised(fee, policy.percent), current));
	const fees = {
		maxFeePerGas: next(last.maxFeePerGas, estimate.maxFeePerGas),
		maxPriorityFeePerGas: next(last.maxPriorityFeePerGas, estimate.maxPriorityFeePerGas),
	};
	const replaces =
		fees.maxFeePerGas >= raised(last.maxFeePerGas, MIN_REPLACEMENT_PERCENT) &&
		fees.maxPriorityFeePerGas >= raised(last.maxPriorityFeePerGas, MIN_REPLACEMENT_PERCENT);
	return replaces ? fees : undefined;
}

const releaseLabel = (p: PendingRelease) => `nonce=${p.nonce} recipient=${p.recipient} amount=${p.palletAmount}`;

/** A mined release leaves the pending set only once its nonce reads consumed at
 *  the finality boundary (see `pruneConsumed`); until then a re-attempt is a
 *  cheap simulation that classifies as PendingFinality and stays quiet, and a
 *  release reorged away is still ours to retry. A mined revert carries no
 *  reason (usually a lost race: a peer released, or the daily allowance went
 *  between simulation and inclusion); the next cycle's simulation classifies
 *  it, and the prune drops it if a peer won. */
function logMined(releaser: Releaser, receipt: TransactionReceipt, label: string): void {
	releaser.log(
		receipt.status === "success"
			? `released (awaiting ${releaser.baseFinalityTag}): ${label} tx=${receipt.transactionHash}`
			: `release reverted on-chain, re-evaluating next cycle: ${label}`,
	);
}

/**
 * Follow the release transaction in flight and say whether it is settled:
 * an attempt is mined, its nonce was used by another transaction, or the node
 * holds nothing at a nonce below it, so nothing will ever let it be mined. The
 * entry is then simulated again like any other. While it is pending, it is
 * re-sent `feeBump.afterMs` after the last send: the identical call at the
 * same nonce with both fees raised (`bumpedFees`), at most `feeBump.max`
 * times. When no bump is left or the cap stops one, it pages (throttled like
 * every page about a release) and keeps waiting.
 */
async function followInFlight(releaser: Releaser, tx: InFlightRelease): Promise<boolean> {
	const { publicClient, walletClient, account, feeBump, log } = releaser;
	const label = releaseLabel(tx.entry);
	for (const hash of tx.hashes) {
		const receipt = await publicClient.getTransactionReceipt({ hash }).catch((error: unknown) => {
			if (error instanceof TransactionReceiptNotFoundError) return undefined;
			throw error;
		});
		if (receipt) {
			releaser.inFlight = undefined;
			logMined(releaser, receipt, label);
			return true;
		}
	}
	const count = (blockTag: "latest" | "pending") => publicClient.getTransactionCount({ address: account.address, blockTag });
	const mined = await count("latest");
	// Its nonce was used (by an attempt whose receipt is not visible yet, or by
	// another transaction), or the node holds nothing below it: decide afresh.
	if (mined > tx.nonce || (mined < tx.nonce && (await count("pending")) <= mined)) {
		releaser.inFlight = undefined;
		log(
			`release transaction at nonce ${tx.nonce} can no longer be mined there ` +
				`(account's next mined nonce ${mined}); re-evaluating: ${label}`,
		);
		return true;
	}
	if (releaser.now() - tx.sentAtMs < feeBump.afterMs) return false;
	const fees = tx.bumps < feeBump.max ? bumpedFees(tx.fees, await publicClient.estimateFeesPerGas(), feeBump) : undefined;
	if (fees === undefined) {
		const reason = tx.bumps >= feeBump.max ? `${tx.bumps} fee bump(s) used up` : "the fee cap stops the next bump";
		await pageThrottled(
			releaser,
			tx.entry.nonce,
			"release transaction not mined",
			`${reason}: transaction nonce ${tx.nonce}, account's next mined nonce ${mined}, ` +
				`maxFeePerGas ${formatGwei(tx.fees.maxFeePerGas)} gwei (cap ${formatGwei(feeBump.maxFeePerGas)} gwei); ${label}`,
		);
		return false;
	}
	tx.bumps += 1;
	tx.fees = fees;
	tx.sentAtMs = releaser.now();
	try {
		const hash = await walletClient.writeContract({ ...tx.call, nonce: tx.nonce, ...fees });
		tx.hashes.push(hash);
		log(`fee bump ${tx.bumps} at nonce ${tx.nonce}: ${label} tx=${hash}`);
	} catch (error) {
		// The earlier attempts at this nonce stay valid; the next bump goes higher.
		const reason = error instanceof BaseError ? error.shortMessage : error;
		log(`fee bump ${tx.bumps} at nonce ${tx.nonce} not accepted: ${label} — ${reason}`);
	}
	return false;
}

/** Try to push each pending release through; classify what comes back. One
 *  release transaction is in flight at a time: a later one would only queue
 *  behind its nonce. */
export async function drainPending(releaser: Releaser, conversionFactor: bigint, dailyCap: bigint): Promise<void> {
	const { publicClient, walletClient, account, vaultAddress, baseFinalityTag, pending, alertedAt, log, feeBump } = releaser;
	if (releaser.inFlight !== undefined && !(await followInFlight(releaser, releaser.inFlight))) return;
	for (const p of [...pending.values()]) {
		const label = releaseLabel(p);
		try {
			const release = {
				account,
				address: vaultAddress,
				abi: vaultAbi,
				functionName: "release",
				args: [p.nonce, p.recipient, p.palletAmount],
			} as const;
			// Simulate first so a still-capped release costs no gas.
			await publicClient.simulateContract(release);
			// Pad the gas limit: a release writes an ERC20Votes checkpoint for the
			// vault's vote sink, and whether that is an overwrite or a new entry
			// depends on the block timestamp the transaction lands in — a state
			// the estimate cannot know. An exact estimate can run out of gas in
			// that write (seen in phase 3); doubling it is cheap insurance.
			const call = { ...release, gas: (await publicClient.estimateContractGas(release)) * 2n };
			const nonce = await publicClient.getTransactionCount({ address: account.address, blockTag: "pending" });
			const fees = cappedFees(await publicClient.estimateFeesPerGas(), feeBump.maxFeePerGas);
			const sentAtMs = releaser.now();
			const hash = await walletClient.writeContract({ ...call, nonce, ...fees });
			let receipt: TransactionReceipt;
			try {
				receipt = await publicClient.waitForTransactionReceipt({ hash, timeout: feeBump.afterMs });
			} catch (waitError) {
				// Sent, so it may be pending: followed from the next cycle on
				// instead of signing another release behind its nonce.
				releaser.inFlight = { entry: p, call, nonce, fees, hashes: [hash], sentAtMs, bumps: 0 };
				log(
					`release not seen mined yet, following it at nonce ${nonce}: ${label} tx=${hash} — ` +
						`${waitError instanceof BaseError ? waitError.shortMessage : waitError}`,
				);
				return;
			}
			logMined(releaser, receipt, label);
		} catch (error) {
			const reason = error instanceof Error ? error.message : String(error);
			// A peer may have consumed the nonce after our simulation. Settle
			// that race from state before relying on decoded custom errors.
			const confirmed = await publicClient.getBlock({ blockTag: baseFinalityTag });
			if (confirmed.number === null) throw new Error(`${baseFinalityTag} Base block has no number`);
			const consumedConfirmed = await publicClient.readContract({
				address: vaultAddress,
				abi: vaultAbi,
				functionName: "nonceConsumed",
				args: [p.nonce],
				blockNumber: confirmed.number,
			});
			const consumedLatest = consumedConfirmed ? true : await publicClient.readContract({
				address: vaultAddress,
				abi: vaultAbi,
				functionName: "nonceConsumed",
				args: [p.nonce],
			});
			const decoded = contractErrorName(error);
			// Only a consumption read at the finality boundary may drop the entry.
			// A NonceAlreadyConsumed decoded from a LATEST-state simulation on a
			// node ahead of the ones we read is the same pending-finality case.
			const errorName = consumedConfirmed
				? "NonceAlreadyConsumed"
				: consumedLatest || decoded === "NonceAlreadyConsumed"
					? "PendingFinality"
					: decoded === "ExceedsDailyCap" && p.palletAmount * conversionFactor > dailyCap
						? "ExceedsDailyCapPermanently"
						: decoded;
			switch (classifyReleaseFailure(errorName)) {
				case "done":
					pending.delete(p.nonce);
					alertedAt.delete(p.nonce);
					log(`skip (already consumed): ${label}`);
					break;
				case "retry":
					// Expected while the daily bucket refills — stay quiet.
					break;
				case "blocked":
					// Clearing this needs a timelocked governance action, so the
					// condition persists for days; page at a bounded interval.
					await pageThrottled(
						releaser,
						p.nonce,
						"release blocked and needs operator action",
						`${label} — ${errorName}; it cannot self-heal`,
					);
					break;
				case "unexpected":
					await pageThrottled(releaser, p.nonce, "unexpected release failure", `${label} — ${reason}`);
					break;
			}
		}
	}
}
