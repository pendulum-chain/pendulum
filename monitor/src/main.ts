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
import { isStale, releasedExceedsMigrated, vaultConservationDeficit } from "./checks.js";

// Canonical Multicall3 deployment (same address on Base and every major chain),
// used to batch the per-nonce liveness reads into a handful of RPC round-trips.
const MULTICALL3_ADDRESS = "0xcA11bde05977b3631167028862bE2a173976CA11" as const;

const vaultAbi = [
	{ type: "function", name: "totalReleased", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
	{ type: "function", name: "totalSwept", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
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
	contracts: { multicall3: { address: MULTICALL3_ADDRESS } },
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

let multicallUnavailable = false;

/** Read `nonceConsumed` for many nonces, batched through Multicall3.
 *
 *  Falls back to plain concurrent reads if the batch call fails — e.g. on a
 *  chain where Multicall3 is not deployed at the canonical address, or a
 *  provider that rejects the batch. The fallback still works (just chattier),
 *  so a misconfigured multicall degrades liveness detection rather than
 *  crashing the whole check cycle and blinding the conservation alerts. */
async function readNonceConsumed(nonces: bigint[]): Promise<boolean[]> {
	if (!multicallUnavailable) {
		try {
			return (await publicClient.multicall({
				allowFailure: false,
				contracts: nonces.map((nonce) => ({
					address: config.vaultAddress,
					abi: vaultAbi,
					functionName: "nonceConsumed",
					args: [nonce],
				})),
			})) as boolean[];
		} catch (multicallError) {
			// Latch so we do not re-attempt (and re-log) the batch every poll.
			multicallUnavailable = true;
			await alert(
				"multicall unavailable, using per-nonce reads",
				`liveness reads fall back to individual calls; verify Multicall3 at ${MULTICALL3_ADDRESS}: ${multicallError}`,
			);
		}
	}

	// Fallback: read in bounded concurrent batches to avoid a request storm.
	const CHUNK = 100;
	const flags: boolean[] = [];
	for (let start = 0; start < nonces.length; start += CHUNK) {
		const chunk = nonces.slice(start, start + CHUNK);
		const chunkFlags = await Promise.all(
			chunk.map((nonce) =>
				publicClient.readContract({
					address: config.vaultAddress,
					abi: vaultAbi,
					functionName: "nonceConsumed",
					args: [nonce],
				}),
			),
		);
		flags.push(...chunkFlags);
	}
	return flags;
}

async function check(api: ApiPromise): Promise<void> {
	// --- Base side, pinned to one block ---
	// Read Base FIRST, then Pendulum's monotonically-growing totalMigrated at a
	// strictly-later snapshot: this guarantees totalMigrated >= what any Base
	// release could have been attested against, so the M2a check can never
	// false-positive on a burn that finalized between the two reads. Pinning
	// every Base read to one block keeps totalReleased and vaultBalance from
	// skewing against each other (a release landing mid-cycle).
	const blockNumber = await publicClient.getBlockNumber();
	const [totalReleased, totalSwept, conversionFactor, tokenAddress] = await Promise.all([
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "totalReleased", blockNumber }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "totalSwept", blockNumber }),
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

	// --- Pendulum side, at the finalized head (read after Base, see above) ---
	const finalizedHash = await api.rpc.chain.getFinalizedHead();
	const apiAt = await api.at(finalizedHash);
	const totalMigrated = BigInt((await apiAt.query.tokenMigration.totalMigrated()).toString());
	const nextNonce = BigInt((await apiAt.query.tokenMigration.nextNonce()).toString());

	// (M2a) Nothing may leave the vault that was not burned on Pendulum.
	if (releasedExceedsMigrated(totalReleased, totalMigrated, conversionFactor)) {
		await alert(
			"CONSERVATION VIOLATION",
			`released ${totalReleased} > migrated ${totalMigrated * conversionFactor} (token units)`,
		);
		await pauseVault();
		return;
	}

	// (M2b) Vault-internal conservation. Only a DEFICIT signals real loss; a
	// surplus is a harmless inbound transfer (donation, or a migration whose
	// recipient is the vault) and must not trip the check — otherwise a dust
	// transfer would pause the vault every poll until the window-close sweep.
	// totalSwept accounts for the intended end-of-window sweep.
	if (vaultConservationDeficit(vaultBalance, totalReleased, totalSwept, totalSupply)) {
		await alert(
			"VAULT BALANCE DEFICIT",
			`balance ${vaultBalance} + released ${totalReleased} + swept ${totalSwept} < supply ${totalSupply}`,
		);
		await pauseVault();
		return;
	}

	// (M4) Liveness: nonces the monitor has known about for longer than the
	// grace period must be consumed on Base. Batch the per-nonce reads through
	// Multicall3 so a large release backlog (e.g. during a pause) cannot make a
	// cycle outrun the poll interval and starve the conservation checks above.
	const now = Date.now();
	for (let nonce = 0n; nonce < nextNonce; nonce++) {
		if (!nonceFirstSeen.has(nonce)) nonceFirstSeen.set(nonce, now);
	}
	const pendingNonces = [...nonceFirstSeen.keys()];
	if (pendingNonces.length > 0) {
		const consumedFlags = await readNonceConsumed(pendingNonces);
		for (let i = 0; i < pendingNonces.length; i++) {
			const nonce = pendingNonces[i];
			if (consumedFlags[i]) {
				nonceFirstSeen.delete(nonce);
			} else if (isStale(nonceFirstSeen.get(nonce) ?? now, now, config.graceSeconds)) {
				await alert(
					"LIVENESS: migration not released",
					`nonce ${nonce} unreleased for over ${config.graceSeconds}s — attestor outage, cap deferral or pause?`,
				);
			}
		}
	}

	log(
		`ok: migrated=${totalMigrated} released=${totalReleased} swept=${totalSwept} ` +
			`pending=${nonceFirstSeen.size} vaultBalance=${vaultBalance}`,
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
