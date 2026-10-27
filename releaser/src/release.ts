/**
 * The releaser's release attempts: simulate and send `release()` for each
 * pending entry, and classify what a failed attempt means. `main.ts` injects
 * the clients and the pending set, so the decisions here are unit tested (see
 * release.test.ts).
 */

import type { Account, Chain, PublicClient, Transport, WalletClient } from "viem";
import type { Severity } from "./alert.js";
import { classifyReleaseFailure, contractErrorName } from "./checks.js";
import { vaultAbi } from "./vaultAbi.js";

export interface PendingRelease {
	nonce: bigint;
	recipient: `0x${string}`;
	palletAmount: bigint;
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

/** A release transaction that was mined and reverted. The receipt carries no
 *  revert data, so the reason is unknowable here; the next cycle's simulation
 *  classifies it properly (and pruneConsumed drops it if a peer won). */
class MinedRevert extends Error {}

/** Try to push each pending release through; classify what comes back. */
export async function drainPending(releaser: Releaser, conversionFactor: bigint, dailyCap: bigint): Promise<void> {
	const { publicClient, walletClient, account, vaultAddress, baseFinalityTag, pending, alertedAt, log } = releaser;
	for (const p of [...pending.values()]) {
		const label = `nonce=${p.nonce} recipient=${p.recipient} amount=${p.palletAmount}`;
		try {
			// Simulate first so a still-capped release costs no gas.
			const { request } = await publicClient.simulateContract({
				account,
				address: vaultAddress,
				abi: vaultAbi,
				functionName: "release",
				args: [p.nonce, p.recipient, p.palletAmount],
			});
			// Pad the gas limit: a release writes an ERC20Votes checkpoint for the
			// vault's vote sink, and whether that is an overwrite or a new entry
			// depends on the block timestamp the transaction lands in — a state
			// the estimate cannot know. An exact estimate can run out of gas in
			// that write (seen in phase 3); doubling it is cheap insurance.
			const gasLimit = (await publicClient.estimateContractGas({
				account,
				address: vaultAddress,
				abi: vaultAbi,
				functionName: "release",
				args: [p.nonce, p.recipient, p.palletAmount],
			})) * 2n;
			const txHash = await walletClient.writeContract({ ...request, gas: gasLimit });
			const receipt = await publicClient.waitForTransactionReceipt({ hash: txHash });
			if (receipt.status !== "success") {
				throw new MinedRevert(`release reverted on-chain: ${txHash}`);
			}
			// Deliberately NOT deleted here: the entry leaves the durable pending
			// set only when `pruneConsumed` sees the nonce consumed at the
			// finality boundary. Until then a re-attempt is a cheap simulation
			// that classifies as PendingFinality and stays quiet — and if the
			// release were reorged away, the entry is still ours to retry.
			log(`released (awaiting ${baseFinalityTag}): ${label} tx=${txHash}`);
		} catch (error) {
			if (error instanceof MinedRevert) {
				// Usually a benign race (a peer released, or the daily allowance
				// was consumed between simulation and inclusion). Stay quiet: the
				// next cycle prunes it if consumed and re-simulates otherwise,
				// which yields a decodable, classifiable reason.
				log(`release reverted on-chain, re-evaluating next cycle: ${label}`);
				continue;
			}
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
