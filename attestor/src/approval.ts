/**
 * The attestor's approval path: what it approves for a migration event, how
 * its transaction is followed until mined (re-sent with higher fees when it is
 * not), when a failed approval is a lost race or its own miss, and when the
 * checkpoint may pass a block. `main.ts` injects the chain clients, so every
 * decision here is unit tested against an in-memory vault (see
 * approval.test.ts).
 */

import {
	BaseError,
	ContractFunctionZeroDataError,
	encodeAbiParameters,
	encodeFunctionData,
	formatGwei,
	keccak256,
	type Hash,
	type Hex,
	type LocalAccount,
	type TransactionReceipt,
} from "viem";
import type { Severity } from "./alert.js";
import { isUnreleasable, ownApprovalMissed, sendRejection, type MigrationEvent } from "./checks.js";
import { vaultAbi } from "./vaultAbi.js";

/** Multiplier applied to the estimated gas for `approve`. See the note in
 *  `approve`: the same call can take the cheap record path or the expensive
 *  threshold-crossing release path. */
export const GAS_LIMIT_MULTIPLIER = 4n;

/** How persistently to confirm that a failed approval was merely a lost race
 *  before treating it as fatal. See `handledSettled`. */
const RACE_RECHECK_ATTEMPTS = 5;
const RACE_RECHECK_DELAY_MS = 3000;

/** op-geth replaces a pending transaction only when the new one raises both
 *  its fee cap and its tip by at least this many percent. */
export const MIN_REPLACEMENT_PERCENT = 10;

/** How often a sent approval's receipt is polled: about one Base block. */
const RECEIPT_POLL_MS = 2000;

export interface Fees {
	maxFeePerGas: bigint;
	maxPriorityFeePerGas: bigint;
}

/** When, how often and how far an own approval that is not mined is re-sent
 *  with higher fees. */
export interface FeeBumpPolicy {
	/** Re-send an attempt still not mined after this long. */
	afterMs: number;
	/** Raise both fees by at least this many percent per re-send. */
	percent: number;
	/** Re-sends per approval. */
	max: number;
	/** No attempt ever offers a higher maxFeePerGas. */
	maxFeePerGas: bigint;
}

/** One own approval transaction: the identical call, kept at one nonce once the
 *  node has accepted it, and re-sent there with higher fees until it is mined. */
export interface Submission {
	data: Hex;
	gas: bigint;
	/** Set by the first attempt the node accepted, and fixed from then on. */
	nonce?: number;
	/** The latest attempt's fees. */
	fees?: Fees;
	/** Every attempt at `nonce`; any of them may be the one that is mined. */
	hashes: Hash[];
	bumps: number;
	/** The page that it is not being mined went out. */
	paged: boolean;
}

/** A block processed against the latest Base state whose releasable events are
 *  not yet confirmed inside the finality boundary. The checkpoint may not
 *  advance past it until they are. */
export interface UnconfirmedBlock {
	block: number;
	/** Events awaiting durability. Unreleasable tuples are excluded: they are
	 *  permanently skipped (with a critical alert) and never resolve on Base. */
	events: MigrationEvent[];
	/** When this block's approvals were last (re-)submitted. */
	lastAttemptMs: number;
}

/** The Base reads and writes of the approval path, as this attestor's signer.
 *  Reads without a block number are at the latest state. */
export interface VaultChain {
	nonceConsumed(nonce: bigint, blockNumber?: bigint): Promise<boolean>;
	/** Whether this attestor's own approval of `payload` is recorded. */
	hasApproved(payload: Hex, blockNumber?: bigint): Promise<boolean>;
	/** Gas `approve` needs at the latest state; rejects with the vault's revert. */
	approveGas(event: MigrationEvent): Promise<bigint>;
	/** The signer's account nonce: after its mined transactions (`latest`), or
	 *  also after those the node holds in its pool (`pending`). */
	accountNonce(blockTag: "latest" | "pending"): Promise<number>;
	/** The node's current EIP-1559 fee estimate. */
	fees(): Promise<Fees>;
	sendRawTransaction(serialized: Hex): Promise<unknown>;
	/** Undefined while the transaction is not mined. */
	receipt(hash: Hash): Promise<TransactionReceipt | undefined>;
	/** The newest block inside the Base finality boundary. */
	boundaryBlock(): Promise<bigint>;
}

export interface Attestor {
	chain: VaultChain;
	account: LocalAccount;
	baseChainId: number;
	vaultAddress: `0x${string}`;
	baseFinalityTag: string;
	baseFinalityTimeoutMs: number;
	missedApprovalAlertCount: number;
	feeBump: FeeBumpPolicy;
	/** Severity defaults to warning. */
	alert(subject: string, detail: unknown, severity?: Severity): Promise<void>;
	log(message: string, extra?: unknown): void;
	now(): number;
	sleep(ms: number): Promise<void>;
	/** Consecutive migrations that resolved on-chain while our own approval never
	 *  landed. Lost races do not count; any mined approval of ours resets it. */
	missedApprovals: number;
	/** Own approvals not mined yet, by payload hash: a retry resumes the same
	 *  transaction instead of sending a second one at another nonce. */
	submissions: Map<Hex, Submission>;
	/** The lowest nonce no transaction of this process has been accepted at.
	 *  The node's pending count can trail it (a load-balanced endpoint, a pool
	 *  that dropped the transaction), and a nonce one payload was sent at is
	 *  never signed for another. */
	nextNonce: number;
}

/** `MigrationVault.payloadHash`: what approvals are recorded under. */
export function payloadHash(event: MigrationEvent): Hex {
	return keccak256(
		encodeAbiParameters(
			[{ type: "uint64" }, { type: "address" }, { type: "uint256" }],
			[event.nonce, event.recipient, event.palletAmount],
		),
	);
}

/** True when this migration no longer needs our approval (released, or we
 *  already approved) at the given Base block, or at latest when omitted. With
 *  4 attestors racing to the same event, losing the race is the NORMAL case,
 *  not an error, and concluding that at latest needs no finality — durability
 *  is enforced separately before the checkpoint moves (see `confirmDurable`). */
async function handledAt(attestor: Attestor, event: MigrationEvent, blockNumber?: bigint): Promise<boolean> {
	if (await attestor.chain.nonceConsumed(event.nonce, blockNumber)) return true;
	return attestor.chain.hasApproved(payloadHash(event), blockNumber);
}

/**
 * `handledAt` latest, but tolerant of an RPC that has not caught up yet.
 *
 * Public endpoints are load-balanced across nodes and give no read-after-write
 * consistency guarantee, so immediately after our transaction reverts the
 * state proving WHY it reverted may not be visible yet. A single read that
 * happens to hit a lagging node reports "not handled", which turns the most
 * ordinary event in this system — losing the k-of-n race — into an
 * unexplained failure and takes the daemon down. Re-check with backoff before
 * concluding anything is actually wrong.
 */
async function handledSettled(attestor: Attestor, event: MigrationEvent): Promise<boolean> {
	for (let attempt = 0; attempt < RACE_RECHECK_ATTEMPTS; attempt++) {
		try {
			if (await handledAt(attestor, event)) return true;
		} catch (error) {
			attestor.log("recheck failed, retrying", error);
		}
		if (attempt < RACE_RECHECK_ATTEMPTS - 1) await attestor.sleep(RACE_RECHECK_DELAY_MS);
	}
	return false;
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

/**
 * The fees to re-send an attempt that was not mined with: both raised by the
 * policy's percent, and at least to the node's current estimate so that a base
 * fee which rose past the attempt is caught up with in one step, never above
 * the cap. Undefined when the cap leaves no room for a replacement the pool
 * accepts.
 */
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

/** Poll the receipts of `hashes` until one is mined or `timeoutMs` has passed. */
async function minedReceipt(attestor: Attestor, hashes: Hash[], timeoutMs: number): Promise<TransactionReceipt | undefined> {
	const deadline = attestor.now() + timeoutMs;
	for (;;) {
		for (const hash of hashes) {
			const receipt = await attestor.chain.receipt(hash);
			if (receipt) return receipt;
		}
		const left = deadline - attestor.now();
		if (left <= 0) return undefined;
		await attestor.sleep(Math.min(RECEIPT_POLL_MS, left));
	}
}

/** Sign the submission's call (always the same `to`, `data`, `value` and gas)
 *  at `nonce` with `fees`, and send it. The hash is known even when the send
 *  fails. */
async function sendAttempt(
	attestor: Attestor,
	submission: Submission,
	nonce: number,
	fees: Fees,
): Promise<{ hash: Hash; error?: unknown }> {
	const serialized = await attestor.account.signTransaction({
		type: "eip1559",
		chainId: attestor.baseChainId,
		nonce,
		to: attestor.vaultAddress,
		data: submission.data,
		value: 0n,
		gas: submission.gas,
		...fees,
	});
	const hash = keccak256(serialized);
	try {
		await attestor.chain.sendRawTransaction(serialized);
		return { hash };
	} catch (error) {
		return { hash, error };
	}
}

/** Send a submission the node has not accepted yet, at the lowest nonce that no
 *  other transaction of this process holds. A refusal for the price or a full
 *  pool is retried at the next bump; a nonce some transaction this process
 *  does not track holds is skipped, never replaced. */
async function sendFirst(attestor: Attestor, submission: Submission, label: string): Promise<void> {
	submission.fees ??= cappedFees(await attestor.chain.fees(), attestor.feeBump.maxFeePerGas);
	const nonce = Math.max(await attestor.chain.accountNonce("pending"), attestor.nextNonce);
	const { hash, error } = await sendAttempt(attestor, submission, nonce, submission.fees);
	const rejection = error === undefined ? undefined : sendRejection(error);
	if (error === undefined || rejection === "known") {
		submission.nonce = nonce;
		submission.hashes.push(hash);
		attestor.nextNonce = nonce + 1;
		return;
	}
	if (rejection === undefined) throw error;
	if (rejection === "nonce") attestor.nextNonce = nonce + 1;
	attestor.log(`approval not accepted at nonce ${nonce}, trying again later: ${label}`, error);
}

/** Send the latest attempt again, unchanged. That is no bump, but it brings
 *  back a transaction a node dropped while still counting its nonce as used,
 *  which would otherwise leave a gap every later approval waits behind. A node
 *  that still holds it answers that it knows it. */
async function resendLatest(attestor: Attestor, submission: Submission, nonce: number, label: string): Promise<void> {
	const { error } = await sendAttempt(attestor, submission, nonce, submission.fees as Fees);
	const rejection = error === undefined ? undefined : sendRejection(error);
	if (error !== undefined && rejection !== "known" && rejection !== "nonce") {
		attestor.log(`re-sending the latest attempt at nonce ${nonce} failed: ${label}`, error);
	}
}

/**
 * Send the submission and follow it until one of its attempts is mined.
 *
 * An attempt still not mined after `feeBump.afterMs` is re-sent: the identical
 * call at the same nonce with both fees raised (`bumpedFees`), at most
 * `feeBump.max` times, and only while the approval is still needed. When no
 * bump is left or the cap stops one, it pages once and keeps waiting, re-sending
 * the latest attempt unchanged: later approvals would queue behind this nonce
 * anyway. Returns undefined when the approval is no longer needed (released, or
 * ours recorded) before a receipt was seen; its nonce is never given to another
 * payload.
 */
async function sendUntilMined(
	attestor: Attestor,
	event: MigrationEvent,
	submission: Submission,
	label: string,
): Promise<TransactionReceipt | undefined> {
	const policy = attestor.feeBump;
	for (;;) {
		if (submission.nonce === undefined) await sendFirst(attestor, submission, label);
		const receipt = await minedReceipt(attestor, submission.hashes, policy.afterMs);
		if (receipt) return receipt;
		const { nonce } = submission;
		if (await handledAt(attestor, event)) {
			// No longer needed, so no bump; but the nonce stays this approval's.
			if (nonce !== undefined) await resendLatest(attestor, submission, nonce, label);
			return undefined;
		}
		const minedNonce = nonce === undefined ? undefined : await attestor.chain.accountNonce("latest");
		if (nonce !== undefined && minedNonce !== undefined && minedNonce > nonce) {
			// Our nonce is used: by an attempt mined since the poll, or by a
			// transaction this process did not send. Then the approval needs a
			// new transaction.
			const late = await minedReceipt(attestor, submission.hashes, 0);
			if (late) return late;
			attestor.log(`nonce ${nonce} was used by another transaction; sending the approval again: ${label}`);
			Object.assign(submission, { nonce: undefined, fees: undefined, hashes: [], bumps: 0, paged: false });
			continue;
		}
		const last = submission.fees as Fees;
		const fees =
			submission.paged || submission.bumps >= policy.max
				? undefined
				: bumpedFees(last, await attestor.chain.fees(), policy);
		if (fees === undefined) {
			if (!submission.paged) {
				submission.paged = true;
				const reason =
					submission.bumps >= policy.max ? `${submission.bumps} fee bump(s) used up` : "the fee cap stops the next bump";
				await attestor.alert(
					"own approval not mined",
					`${reason}: transaction nonce ${nonce ?? "not accepted by the node"}, account's next mined nonce ${minedNonce ?? "unknown"}, ` +
						`maxFeePerGas ${formatGwei(last.maxFeePerGas)} gwei (cap ${formatGwei(policy.maxFeePerGas)} gwei); ${label}`,
					"critical",
				);
			}
			if (nonce !== undefined) await resendLatest(attestor, submission, nonce, label);
			continue;
		}
		submission.bumps += 1;
		submission.fees = fees;
		if (nonce === undefined) continue;
		const { hash, error } = await sendAttempt(attestor, submission, nonce, fees);
		// Tracked even when refused: polling a receipt that never comes is
		// harmless, and the earlier attempts at this nonce stay valid.
		submission.hashes.push(hash);
		if (error !== undefined) attestor.log(`fee bump ${submission.bumps} at nonce ${nonce} not accepted: ${label}`, error);
		else attestor.log(`fee bump ${submission.bumps} at nonce ${nonce}: ${label} tx=${hash}`);
	}
}

/** Submit the approval for one migration event, skipping work already done.
 *  Returns true when the event must later be confirmed durable at the Base
 *  finality boundary (submitted, raced, or already handled), false when it is
 *  permanently unreleasable and excluded from durability tracking. */
export async function approve(attestor: Attestor, event: MigrationEvent): Promise<boolean> {
	const label = `nonce=${event.nonce} recipient=${event.recipient} amount=${event.palletAmount}`;

	if (isUnreleasable(event.recipient, event.palletAmount, attestor.vaultAddress)) {
		await attestor.alert("CRITICAL: unreleasable migration event skipped permanently", label, "critical");
		return false;
	}

	const key = payloadHash(event);
	if (await handledAt(attestor, event)) {
		attestor.submissions.delete(key);
		attestor.log(`skip (already released or approved): ${label}`);
		return true;
	}

	let mined = false;
	try {
		let submission = attestor.submissions.get(key);
		if (submission === undefined) {
			// Pad the gas limit generously. The SAME approve call executes one of
			// two very different paths depending on what has landed by the time
			// it is mined: either it merely records an approval, or it is the one
			// that crosses the threshold and therefore performs the release,
			// including an ERC-20 transfer. Gas estimated while the cheap path
			// applied is not enough for the expensive one, and with several
			// attestors racing the same migration that reordering is the normal
			// case, not an edge case. An under-estimate reverts OutOfGas, which
			// reads as an unexplained failure and takes the daemon down.
			const gas = (await attestor.chain.approveGas(event)) * GAS_LIMIT_MULTIPLIER;
			const data = encodeFunctionData({
				abi: vaultAbi,
				functionName: "approve",
				args: [event.nonce, event.recipient, event.palletAmount],
			});
			submission = { data, gas, hashes: [], bumps: 0, paged: false };
			attestor.submissions.set(key, submission);
		}
		const receipt = await sendUntilMined(attestor, event, submission, label);
		attestor.submissions.delete(key);
		if (receipt === undefined) throw new Error(`own approval not mined before the migration resolved (${label})`);
		// Mined, so our submission path works even if the race was lost.
		mined = true;
		attestor.missedApprovals = 0;
		if (receipt.status !== "success") {
			throw new Error(`approve transaction reverted: ${receipt.transactionHash} (${label})`);
		}
		attestor.log(`approved: ${label} tx=${receipt.transactionHash}`);
	} catch (error) {
		// Unresolved on-chain, the failure is genuine and propagates to the
		// fatal handler. Resolved, it is the expected race when the release
		// (or our own retried tx) landed between our pre-check and the
		// transaction — benign.
		if (!(await handledSettled(attestor, event))) throw error;
		attestor.submissions.delete(key);
		const missed = await ownApprovalMissed(error, mined, () =>
			attestor.chain.hasApproved(key).catch((readError: unknown) => {
				attestor.log(`own-approval read failed: ${label}`, readError);
				throw readError;
			}),
		);
		if (missed === undefined) {
			attestor.log(`skip (resolved on-chain; could not read whether our own approval is recorded): ${label}`, error);
			return true;
		}
		if (!missed) {
			attestor.log(`skip (raced, resolved on-chain): ${label}`);
			return true;
		}
		// Resolved by the others while our own approval never landed: nothing
		// is lost this time, but an attestor whose submissions keep failing
		// silently leaves the fleet one short.
		attestor.missedApprovals += 1;
		attestor.log(
			`own approval did not land (${attestor.missedApprovals} in a row), resolved on-chain by others: ${label}`,
			error,
		);
		if (attestor.missedApprovals % attestor.missedApprovalAlertCount === 0) {
			await attestor.alert(
				"own approvals are not landing",
				`${attestor.missedApprovals} consecutive migrations resolved without this attestor; last ${label}: ${error}`,
				"critical",
			);
		}
	}
	return true;
}

/** Approve every event of one finalized Pendulum block and queue the block for
 *  the durability check. */
export async function processBlock(
	attestor: Attestor,
	block: number,
	events: MigrationEvent[],
	unconfirmed: UnconfirmedBlock[],
): Promise<void> {
	const tracked: MigrationEvent[] = [];
	for (const event of events) {
		if (await approve(attestor, event)) tracked.push(event);
	}
	unconfirmed.push({ block, events: tracked, lastAttemptMs: attestor.now() });
}

/** All releasable events of `entry` are resolved at `blockNumber` — either the
 *  nonce is consumed (released) or our approval is recorded there. A boundary
 *  block that predates the vault (a fresh deployment on a chain whose safe head
 *  still trails it) cannot have anything durable in it: the read returns no
 *  data, which means "not yet", never "fatal". */
async function eventsDurableAt(attestor: Attestor, entry: UnconfirmedBlock, blockNumber: bigint): Promise<boolean> {
	try {
		for (const event of entry.events) {
			if (!(await handledAt(attestor, event, blockNumber))) return false;
		}
		return true;
	} catch (error) {
		if (error instanceof BaseError && error.walk((e) => e instanceof ContractFunctionZeroDataError)) {
			attestor.log(`vault has no state at ${attestor.baseFinalityTag} block ${blockNumber} yet; checkpoint waits`);
			return false;
		}
		throw error;
	}
}

/** Drop every leading block whose events are resolved inside the Base finality
 *  boundary, and return the last one dropped: the durable checkpoint may move
 *  there. A block that refuses to settle (our approval reorged out and nothing
 *  replaced it) is re-approved after `baseFinalityTimeoutMs` rather than waited
 *  on forever — the checkpoint cannot pass it until it settles, so nothing is
 *  ever skipped. */
export async function confirmDurable(attestor: Attestor, unconfirmed: UnconfirmedBlock[]): Promise<number | undefined> {
	let boundary: bigint | undefined;
	let confirmed: number | undefined;
	while (unconfirmed.length > 0) {
		const head = unconfirmed[0];
		if (head.events.length > 0) {
			boundary ??= await attestor.chain.boundaryBlock();
			if (!(await eventsDurableAt(attestor, head, boundary))) {
				if (attestor.now() - head.lastAttemptMs >= attestor.baseFinalityTimeoutMs) {
					await attestor.alert(
						"approvals not durable at the Base finality boundary, re-submitting",
						`Pendulum block ${head.block} (${head.events.length} event(s), boundary ${attestor.baseFinalityTag})`,
					);
					head.lastAttemptMs = attestor.now();
					for (const event of head.events) {
						await approve(attestor, event);
					}
				}
				break;
			}
		}
		confirmed = head.block;
		unconfirmed.shift();
	}
	return confirmed;
}
