/**
 * The attestor's approval path: what it approves for a migration event, when a
 * failed approval is a lost race or its own miss, and when the checkpoint may
 * pass a block. `main.ts` injects the chain clients, so every decision here is
 * unit tested against an in-memory vault (see approval.test.ts).
 */

import {
	BaseError,
	ContractFunctionZeroDataError,
	encodeAbiParameters,
	keccak256,
	type Hash,
	type Hex,
	type TransactionReceipt,
} from "viem";
import { isUnreleasable, ownApprovalMissed, type MigrationEvent } from "./checks.js";

/** Multiplier applied to the estimated gas for `approve`. See the note in
 *  `approve`: the same call can take the cheap record path or the expensive
 *  threshold-crossing release path. */
export const GAS_LIMIT_MULTIPLIER = 4n;

/** How persistently to confirm that a failed approval was merely a lost race
 *  before treating it as fatal. See `handledSettled`. */
const RACE_RECHECK_ATTEMPTS = 5;
const RACE_RECHECK_DELAY_MS = 3000;

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
	sendApprove(event: MigrationEvent, gas: bigint): Promise<Hash>;
	waitForReceipt(hash: Hash): Promise<TransactionReceipt>;
	/** The newest block inside the Base finality boundary. */
	boundaryBlock(): Promise<bigint>;
}

export interface Attestor {
	chain: VaultChain;
	vaultAddress: `0x${string}`;
	baseFinalityTag: string;
	baseFinalityTimeoutMs: number;
	missedApprovalAlertCount: number;
	alert(subject: string, detail: unknown): Promise<void>;
	log(message: string, extra?: unknown): void;
	now(): number;
	sleep(ms: number): Promise<void>;
	/** Consecutive migrations that resolved on-chain while our own approval never
	 *  landed. Lost races do not count; any mined approval of ours resets it. */
	missedApprovals: number;
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

/** Submit the approval for one migration event, skipping work already done.
 *  Returns true when the event must later be confirmed durable at the Base
 *  finality boundary (submitted, raced, or already handled), false when it is
 *  permanently unreleasable and excluded from durability tracking. */
export async function approve(attestor: Attestor, event: MigrationEvent): Promise<boolean> {
	const label = `nonce=${event.nonce} recipient=${event.recipient} amount=${event.palletAmount}`;

	if (isUnreleasable(event.recipient, event.palletAmount, attestor.vaultAddress)) {
		await attestor.alert("CRITICAL: unreleasable migration event skipped permanently", label);
		return false;
	}

	if (await handledAt(attestor, event)) {
		attestor.log(`skip (already released or approved): ${label}`);
		return true;
	}

	let mined = false;
	try {
		// Pad the gas limit generously. The SAME approve call executes one of two
		// very different paths depending on what has landed by the time it is
		// mined: either it merely records an approval, or it is the one that
		// crosses the threshold and therefore performs the release, including an
		// ERC-20 transfer. Gas estimated while the cheap path applied is not
		// enough for the expensive one, and with several attestors racing the
		// same migration that reordering is the normal case, not an edge case.
		// An under-estimate reverts OutOfGas, which reads as an unexplained
		// failure and takes the daemon down.
		const gas = (await attestor.chain.approveGas(event)) * GAS_LIMIT_MULTIPLIER;
		const txHash = await attestor.chain.sendApprove(event, gas);
		const receipt = await attestor.chain.waitForReceipt(txHash);
		// Mined, so our submission path works even if the race was lost.
		mined = true;
		attestor.missedApprovals = 0;
		if (receipt.status !== "success") {
			throw new Error(`approve transaction reverted: ${txHash} (${label})`);
		}
		attestor.log(`approved: ${label} tx=${txHash}`);
	} catch (error) {
		// Unresolved on-chain, the failure is genuine and propagates to the
		// fatal handler. Resolved, it is the expected race when the release
		// (or our own retried tx) landed between our pre-check and the
		// transaction — benign.
		if (!(await handledSettled(attestor, event))) throw error;
		const missed = await ownApprovalMissed(error, mined, () =>
			attestor.chain.hasApproved(payloadHash(event)).catch((readError: unknown) => {
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
