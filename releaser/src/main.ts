/**
 * PEN migration releaser.
 *
 * When a migration reaches the attestor threshold but cannot be released
 * immediately — the rolling daily cap is exhausted, the vault is paused, or it
 * is under-funded — `MigrationVault.approve` records it as pending and emits
 * `ReleasePending` instead of reverting. Those deferrals heal on their own
 * (the leaky bucket refills, governance unpauses), but the vault does not
 * self-execute: somebody has to call the permissionless `release()`.
 *
 * Nothing else in the system does. Attestors only ever submit `approve` for new
 * finalized events, and the monitor is a deliberately read-only watchdog. So
 * without this service a launch-day backlog would sit pending until an operator
 * cleared it by hand, one nonce at a time.
 *
 * Deliberately a separate process rather than a mode of the attestor or the
 * monitor: it keeps the audited approve path untouched, keeps the watchdog
 * read-only, and its key carries no privilege at all — `release()` can only pay
 * the recipient the attestors already agreed on. Two instances can run
 * concurrently; the loser of a race simply observes a consumed nonce.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { createPublicClient, createWalletClient, defineChain, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { blockRanges, classifyReleaseFailure, toPalletAmount } from "./checks.js";
import { config } from "./config.js";
import { vaultAbi } from "./vaultAbi.js";

interface PendingRelease {
	nonce: bigint;
	recipient: `0x${string}`;
	palletAmount: bigint;
}

interface PersistedState {
	/** Next Base block to scan `ReleasePending` from. */
	fromBlock: string;
	/** Pending set, persisted so a restart does not lose work already scanned. */
	pending: Array<{ nonce: string; recipient: string; palletAmount: string }>;
}

const baseChain = defineChain({
	id: config.baseChainId,
	name: "base",
	nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
	rpcUrls: { default: { http: [config.baseRpcUrl] } },
});

const account = privateKeyToAccount(config.releaserPrivateKey);
const publicClient = createPublicClient({ chain: baseChain, transport: http(config.baseRpcUrl) });
const walletClient = createWalletClient({ account, chain: baseChain, transport: http(config.baseRpcUrl) });

/** nonce -> pending release. Keyed by nonce: the vault consumes a nonce once. */
const pending = new Map<bigint, PendingRelease>();
let fromBlock = config.startBlock;

function log(message: string): void {
	console.log(`${new Date().toISOString()} ${message}`);
}

async function alert(subject: string, detail: string): Promise<void> {
	console.error(`${new Date().toISOString()} ALERT: ${subject} — ${detail}`);
	if (!config.alertWebhookUrl) return;
	try {
		await fetch(config.alertWebhookUrl, {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify({ service: "pen-releaser", releaser: account.address, subject, detail }),
		});
	} catch (webhookError) {
		console.error("alert webhook failed", webhookError);
	}
}

function loadState(): void {
	try {
		const state = JSON.parse(readFileSync(config.stateFile, "utf8")) as PersistedState;
		fromBlock = BigInt(state.fromBlock);
		for (const p of state.pending) {
			pending.set(BigInt(p.nonce), {
				nonce: BigInt(p.nonce),
				recipient: p.recipient as `0x${string}`,
				palletAmount: BigInt(p.palletAmount),
			});
		}
	} catch {
		// First run: start from the configured block with an empty set.
	}
}

function saveState(): void {
	const state: PersistedState = {
		fromBlock: fromBlock.toString(),
		pending: [...pending.values()].map((p) => ({
			nonce: p.nonce.toString(),
			recipient: p.recipient,
			palletAmount: p.palletAmount.toString(),
		})),
	};
	writeFileSync(config.stateFile, JSON.stringify(state));
}

/** Scan for newly deferred releases and add them to the pending set. */
async function ingestNewPending(toBlock: bigint, conversionFactor: bigint): Promise<void> {
	for (const [start, end] of blockRanges(fromBlock, toBlock, config.maxBlockRange)) {
		const logs = await publicClient.getContractEvents({
			address: config.vaultAddress,
			abi: vaultAbi,
			eventName: "ReleasePending",
			fromBlock: start,
			toBlock: end,
		});
		for (const entry of logs) {
			const { nonce, recipient, tokenAmount } = entry.args as {
				nonce: bigint;
				recipient: `0x${string}`;
				tokenAmount: bigint;
			};
			if (pending.has(nonce)) continue;
			pending.set(nonce, { nonce, recipient, palletAmount: toPalletAmount(tokenAmount, conversionFactor) });
			log(`deferred release observed: nonce=${nonce} recipient=${recipient}`);
		}
		fromBlock = end + 1n;
	}
}

/** Drop entries the vault has already consumed (by us, a peer, or a rival tuple). */
async function pruneConsumed(): Promise<void> {
	const entries = [...pending.values()];
	if (entries.length === 0) return;
	const results = await publicClient.multicall({
		contracts: entries.map((p) => ({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "nonceConsumed" as const,
			args: [p.nonce] as const,
		})),
		allowFailure: true,
	});
	results.forEach((result, i) => {
		if (result.status === "success" && result.result === true) {
			pending.delete(entries[i].nonce);
		}
	});
}

/** Try to push each pending release through; classify what comes back. */
async function drainPending(): Promise<void> {
	for (const p of [...pending.values()]) {
		const label = `nonce=${p.nonce} recipient=${p.recipient} amount=${p.palletAmount}`;
		try {
			// Simulate first so a still-capped release costs no gas.
			const { request } = await publicClient.simulateContract({
				account,
				address: config.vaultAddress,
				abi: vaultAbi,
				functionName: "release",
				args: [p.nonce, p.recipient, p.palletAmount],
			});
			const txHash = await walletClient.writeContract(request);
			const receipt = await publicClient.waitForTransactionReceipt({ hash: txHash });
			if (receipt.status !== "success") {
				throw new Error(`release reverted on-chain: ${txHash}`);
			}
			pending.delete(p.nonce);
			log(`released: ${label} tx=${txHash}`);
		} catch (error) {
			const reason = error instanceof Error ? error.message : String(error);
			switch (classifyReleaseFailure(reason)) {
				case "done":
					pending.delete(p.nonce);
					log(`skip (already consumed): ${label}`);
					break;
				case "retry":
					// Expected while the daily bucket refills — stay quiet.
					break;
				case "blocked":
					await alert(
						"release blocked above the per-release cap",
						`${label} — needs a governance setCaps to clear; it cannot self-heal`,
					);
					break;
				case "unexpected":
					await alert("unexpected release failure", `${label} — ${reason}`);
					break;
			}
		}
	}
}

async function checkGasBalance(): Promise<void> {
	const balance = await publicClient.getBalance({ address: account.address });
	if (balance < config.minGasBalanceWei) {
		await alert("gas balance low", `${account.address} holds ${balance} wei`);
	}
}

async function main(): Promise<void> {
	loadState();
	const conversionFactor = await publicClient.readContract({
		address: config.vaultAddress,
		abi: vaultAbi,
		functionName: "conversionFactor",
	});
	log(`releaser ${account.address} started; scanning from block ${fromBlock}, ${pending.size} pending`);
	await checkGasBalance();

	for (;;) {
		try {
			const latest = await publicClient.getBlockNumber();
			await ingestNewPending(latest, conversionFactor);
			await pruneConsumed();
			await drainPending();
			saveState();
			log(`ok: ${pending.size} pending, scanned through block ${fromBlock - 1n}`);
		} catch (cycleError) {
			await alert("releaser cycle failed", `${cycleError}`);
		}
		await new Promise((resolve) => setTimeout(resolve, config.pollIntervalMs));
	}
}

setInterval(
	() => void checkGasBalance().catch((error) => console.error("gas balance check failed", error)),
	10 * 60 * 1000,
);

main().catch(async (error) => {
	await alert("releaser startup failed", `${error}`);
	process.exit(1);
});
