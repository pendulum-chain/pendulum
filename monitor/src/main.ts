/**
 * PEN migration invariant monitor (PRD §6.5).
 *
 * Runs on infrastructure SEPARATE from every attestor and reads both chains
 * independently. Each poll it checks, at the finalized head of Pendulum and
 * the latest Base block:
 *
 *  (M2a) totalReleased on Base <= TotalMigrated on Pendulum * conversionFactor
 *        (a violation means tokens were released that were never burned —
 *        the strongest possible signal of attestor compromise)
 *  (M2b) balanceOf(vault) + totalReleased == totalSupply
 *        (conservation inside the vault itself)
 *  (M4)  liveness: every migration nonce older than GRACE_SECONDS is consumed
 *        on Base (detects a stalled attestor fleet)
 *
 * On an M2a violation the monitor alerts AND — when GUARDIAN_PRIVATE_KEY is
 * configured (design option in PRD M3) — pauses the vault immediately.
 */

import { ApiPromise, WsProvider } from "@polkadot/api";
import { createPublicClient, createWalletClient, defineChain, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";

const vaultAbi = [
	{ type: "function", name: "totalReleased", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
	{ type: "function", name: "conversionFactor", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
	{ type: "function", name: "token", stateMutability: "view", inputs: [], outputs: [{ type: "address" }] },
	{ type: "function", name: "paused", stateMutability: "view", inputs: [], outputs: [{ type: "bool" }] },
	{
		type: "function",
		name: "nonceConsumed",
		stateMutability: "view",
		inputs: [{ name: "nonce", type: "uint64" }],
		outputs: [{ type: "bool" }],
	},
	{ type: "function", name: "pause", stateMutability: "nonpayable", inputs: [], outputs: [] },
] as const;

const erc20Abi = [
	{ type: "function", name: "totalSupply", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
	{
		type: "function",
		name: "balanceOf",
		stateMutability: "view",
		inputs: [{ name: "owner", type: "address" }],
		outputs: [{ type: "uint256" }],
	},
] as const;

function required(name: string): string {
	const value = process.env[name];
	if (!value) throw new Error(`Missing required environment variable ${name}`);
	return value;
}

const config = {
	pendulumWs: required("PENDULUM_WS"),
	baseRpcUrl: required("BASE_RPC_URL"),
	vaultAddress: required("VAULT_ADDRESS") as `0x${string}`,
	pollIntervalMs: Number(process.env.POLL_INTERVAL_MS ?? "60000"),
	/** Seconds a migration may stay unreleased before a liveness alert (M4). */
	graceSeconds: Number(process.env.GRACE_SECONDS ?? "1800"),
	alertWebhookUrl: process.env.ALERT_WEBHOOK_URL,
	/** Optional: enables auto-pause on a conservation violation (M3). */
	guardianPrivateKey: process.env.GUARDIAN_PRIVATE_KEY as `0x${string}` | undefined,
	baseChainId: Number(process.env.BASE_CHAIN_ID ?? "8453"),
};

const baseChain = defineChain({
	id: config.baseChainId,
	name: "base",
	nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
	rpcUrls: { default: { http: [config.baseRpcUrl] } },
});
const publicClient = createPublicClient({ chain: baseChain, transport: http(config.baseRpcUrl) });

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
			body: JSON.stringify({ service: "pen-monitor", subject, detail }),
		});
	} catch (webhookError) {
		console.error("alert webhook failed", webhookError);
	}
}

async function pauseVault(): Promise<void> {
	if (!config.guardianPrivateKey) {
		await alert("AUTO-PAUSE UNAVAILABLE", "no GUARDIAN_PRIVATE_KEY configured; pause manually NOW");
		return;
	}
	const guardian = privateKeyToAccount(config.guardianPrivateKey);
	const walletClient = createWalletClient({ account: guardian, chain: baseChain, transport: http(config.baseRpcUrl) });
	try {
		const txHash = await walletClient.writeContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "pause",
		});
		await alert("vault auto-paused", `tx=${txHash}`);
	} catch (pauseError) {
		await alert("AUTO-PAUSE FAILED", `${pauseError}; pause manually NOW`);
	}
}

/** Timestamps (ms) at which the monitor first saw each pallet nonce count. */
const nonceFirstSeen = new Map<bigint, number>();

async function check(api: ApiPromise): Promise<void> {
	// --- Pendulum side, at the finalized head ---
	const finalizedHash = await api.rpc.chain.getFinalizedHead();
	const apiAt = await api.at(finalizedHash);
	const totalMigrated = BigInt((await apiAt.query.tokenMigration.totalMigrated()).toString());
	const nextNonce = BigInt((await apiAt.query.tokenMigration.nextNonce()).toString());

	// --- Base side ---
	// All reads are pinned to one block: a release landing between unpinned
	// reads would skew totalReleased vs. vaultBalance and trigger a false
	// conservation alert (and auto-pause).
	const blockNumber = await publicClient.getBlockNumber();
	const [totalReleased, conversionFactor, tokenAddress] = await Promise.all([
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "totalReleased", blockNumber }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "conversionFactor", blockNumber }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "token", blockNumber }),
	]);
	const [totalSupply, vaultBalance] = await Promise.all([
		publicClient.readContract({ address: tokenAddress, abi: erc20Abi, functionName: "totalSupply", blockNumber }),
		publicClient.readContract({
			address: tokenAddress,
			abi: erc20Abi,
			functionName: "balanceOf",
			args: [config.vaultAddress],
			blockNumber,
		}),
	]);

	// (M2a) Nothing may leave the vault that was not burned on Pendulum.
	// totalMigrated lags totalReleased only via finality delay, never the
	// other way around: releases require attestations of finalized burns.
	const migratedInTokenUnits = totalMigrated * conversionFactor;
	if (totalReleased > migratedInTokenUnits) {
		await alert(
			"CONSERVATION VIOLATION",
			`released ${totalReleased} > migrated ${migratedInTokenUnits} (token units)`,
		);
		await pauseVault();
		return;
	}

	// (M2b) Vault-internal conservation.
	if (vaultBalance + totalReleased !== totalSupply) {
		await alert(
			"VAULT BALANCE MISMATCH",
			`balance ${vaultBalance} + released ${totalReleased} != supply ${totalSupply}`,
		);
		await pauseVault();
		return;
	}

	// (M4) Liveness: nonces the monitor has known about for longer than the
	// grace period must be consumed on Base.
	const now = Date.now();
	for (let nonce = 0n; nonce < nextNonce; nonce++) {
		if (!nonceFirstSeen.has(nonce)) nonceFirstSeen.set(nonce, now);
	}
	for (const [nonce, firstSeen] of nonceFirstSeen) {
		const consumed = await publicClient.readContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "nonceConsumed",
			args: [nonce],
		});
		if (consumed) {
			nonceFirstSeen.delete(nonce);
		} else if (now - firstSeen > config.graceSeconds * 1000) {
			await alert(
				"LIVENESS: migration not released",
				`nonce ${nonce} unreleased for over ${config.graceSeconds}s — attestor outage, cap deferral or pause?`,
			);
		}
	}

	log(
		`ok: migrated=${totalMigrated} released=${totalReleased} pending=${nonceFirstSeen.size} ` +
			`vaultBalance=${vaultBalance}`,
	);
}

async function main(): Promise<void> {
	const api = await ApiPromise.create({ provider: new WsProvider(config.pendulumWs) });
	log(`monitor started, polling every ${config.pollIntervalMs}ms`);
	for (;;) {
		try {
			await check(api);
		} catch (checkError) {
			await alert("monitor check failed", `${checkError}`);
		}
		await new Promise((resolve) => setTimeout(resolve, config.pollIntervalMs));
	}
}

main().catch(async (error) => {
	await alert("monitor startup failed", `${error}`);
	process.exit(1);
});
