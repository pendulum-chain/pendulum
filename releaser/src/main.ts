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

import { createPublicClient, createWalletClient, defineChain, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { alertText, postAlert, throttledHeartbeat, type Severity } from "./alert.js";
import { chunks, readInRanges, toPalletAmount } from "./checks.js";
import { config } from "./config.js";
import { drainPending, type PendingRelease, type Releaser } from "./release.js";
import {
	assertStateIdentity,
	loadState as readPersistedState,
	saveState as writePersistedState,
	type PersistedState,
} from "./state.js";
import { vaultAbi } from "./vaultAbi.js";

/** Canonical Multicall3, deployed at the same address on Base and every major
 *  chain. viem refuses to batch unless the chain definition declares it, even
 *  when the contract is present on-chain, so it has to be named here. */
const MULTICALL3 = "0xcA11bde05977b3631167028862bE2a173976CA11" as const;

const baseChain = defineChain({
	id: config.baseChainId,
	name: "base",
	nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
	rpcUrls: { default: { http: [config.baseRpcUrl] } },
	contracts: { multicall3: { address: MULTICALL3 } },
});

/** Set only after an on-chain code check proves Multicall3 is absent. A
 * transient provider error must never permanently change the scaling regime. */
let multicallUnavailable = false;

const account = privateKeyToAccount(config.releaserPrivateKey);
const publicClient = createPublicClient({ chain: baseChain, transport: http(config.baseRpcUrl) });
const walletClient = createWalletClient({ account, chain: baseChain, transport: http(config.baseRpcUrl) });

const pending = new Map<bigint, PendingRelease>();
let fromBlock = config.startBlock;
const blockedAlertedAt = new Map<bigint, number>();

function log(message: string): void {
	console.log(`${new Date().toISOString()} ${message}`);
}

/** Strip URLs before anything leaves the process: RPC endpoints commonly embed
 *  API keys, and viem error texts quote the endpoint verbatim. */
function redact(text: string): string {
	return text.replace(/https?:\/\/[^\s"')]+/gi, "<url>");
}

async function alert(subject: string, detail: string, severity: Severity = "warning"): Promise<void> {
	console.error(`${new Date().toISOString()} ALERT: ${subject} — ${detail}`);
	if (!config.alertWebhookUrl) return;
	const redacted = redact(detail);
	await postAlert(config.alertWebhookUrl, {
		text: alertText(severity, `pen-releaser ${account.address}`, subject, redacted),
		severity,
		service: "pen-releaser",
		releaser: account.address,
		subject,
		detail: redacted,
	});
}

function loadState(): void {
	const state = readPersistedState(config.stateFile);
	if (!state) return;
	assertStateIdentity(state, config);
	fromBlock = BigInt(state.fromBlock);
	for (const p of state.pending) {
		pending.set(BigInt(p.nonce), {
			nonce: BigInt(p.nonce),
			recipient: p.recipient as `0x${string}`,
			palletAmount: BigInt(p.palletAmount),
		});
	}
}

function saveState(): void {
	const state: PersistedState = {
		version: 1,
		baseChainId: config.baseChainId,
		vaultAddress: config.vaultAddress,
		fromBlock: fromBlock.toString(),
		pending: [...pending.values()].map((p) => ({
			nonce: p.nonce.toString(),
			recipient: p.recipient,
			palletAmount: p.palletAmount.toString(),
		})),
	};
	writePersistedState(config.stateFile, state);
}

/** The `eth_getLogs` span, MAX_BLOCK_RANGE at most: it shrinks while the
 *  endpoint refuses ranges as too large (see `readInRanges`). */
const logSpan = { size: config.maxBlockRange, max: config.maxBlockRange, streak: 0 };

/** Scan for newly deferred releases and add them to the pending set. */
async function ingestNewPending(toBlock: bigint, conversionFactor: bigint): Promise<void> {
	await readInRanges(fromBlock, toBlock, logSpan, async (start, end) => {
		// Probe the range's end block first. Against a load-balanced endpoint,
		// `eth_getLogs` can be served by a node that has not reached `end` yet
		// and some providers then silently truncate rather than error — which
		// would advance the cursor past a ReleasePending we never saw, orphaning
		// that deferral until a human notices the monitor's liveness alert. A
		// lagging node fails this probe loudly instead, and the cycle replays.
		await publicClient.getBlock({ blockNumber: end });
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
	});
}

/** Read `nonceConsumed` for many nonces, batched where possible.
 *
 *  Batching is an optimisation, never a requirement: a chain without the
 *  Multicall3 predeploy must degrade to individual reads rather than failing
 *  the cycle. Before this fallback existed a missing predeploy threw on every
 *  cycle that had anything pending -- which is precisely when the releaser
 *  matters -- so it silently never drained a single deferred release. */
async function readConsumed(nonces: bigint[], blockNumber: bigint): Promise<boolean[]> {
	const single = (nonce: bigint) =>
		publicClient.readContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "nonceConsumed",
			args: [nonce],
			blockNumber,
		});

	const flags: boolean[] = [];
	for (const batch of chunks(nonces, config.readBatchSize)) {
		if (!multicallUnavailable) {
			try {
				const results = await publicClient.multicall({
					contracts: batch.map((nonce) => ({
						address: config.vaultAddress,
						abi: vaultAbi,
						functionName: "nonceConsumed" as const,
						args: [nonce] as const,
					})),
					allowFailure: false,
					blockNumber,
				});
				flags.push(...(results as boolean[]));
				continue;
			} catch (error) {
				await alert(
					"multicall batch failed, using bounded per-nonce reads for this batch",
					`${error}`,
				);
			}
		}
		flags.push(...(await Promise.all(batch.map(single))));
	}
	return flags;
}

/** Drop entries the vault has already consumed (by us, a peer, or a rival
 *  tuple) — read at the finality boundary, so an entry only ever leaves the
 *  durable pending set once its consumption cannot be reorged away. */
async function pruneConsumed(blockNumber: bigint): Promise<void> {
	const entries = [...pending.values()];
	if (entries.length === 0) return;
	const consumed = await readConsumed(entries.map((p) => p.nonce), blockNumber);
	entries.forEach((entry, i) => {
		if (consumed[i]) {
			pending.delete(entry.nonce);
			blockedAlertedAt.delete(entry.nonce);
		}
	});
}

const releaser: Releaser = {
	publicClient,
	walletClient,
	account,
	vaultAddress: config.vaultAddress,
	baseFinalityTag: config.baseFinalityTag,
	blockedAlertIntervalMs: config.blockedAlertIntervalMs,
	pending,
	blockedAlertedAt,
	alert,
	log,
};

async function checkGasBalance(): Promise<void> {
	const balance = await publicClient.getBalance({ address: account.address });
	if (balance < config.minGasBalanceWei) {
		await alert("gas balance low", `${account.address} holds ${balance} wei`);
	}
}

async function main(): Promise<void> {
	// The state is bound to BASE_CHAIN_ID: an endpoint serving another chain
	// would scan its logs into this state.
	const baseChainId = await publicClient.getChainId();
	if (baseChainId !== config.baseChainId) {
		throw new Error(`BASE_RPC_URL serves chain ${baseChainId}, but BASE_CHAIN_ID is ${config.baseChainId}`);
	}
	loadState();
	const conversionFactor = await publicClient.readContract({
		address: config.vaultAddress,
		abi: vaultAbi,
		functionName: "conversionFactor",
	});
	const multicallCode = await publicClient.getBytecode({ address: MULTICALL3 });
	if (!multicallCode || multicallCode === "0x") {
		multicallUnavailable = true;
		await alert(
			"multicall unavailable, using bounded per-nonce reads",
			`no contract code at ${MULTICALL3}`,
		);
	}
	log(`releaser ${account.address} started; scanning from block ${fromBlock}, ${pending.size} pending`);
	await checkGasBalance();
	const heartbeat = throttledHeartbeat(config.heartbeatUrl);

	for (;;) {
		try {
			const confirmed = await publicClient.getBlock({ blockTag: config.baseFinalityTag });
			if (confirmed.number === null) throw new Error(`${config.baseFinalityTag} Base block has no number`);
			await ingestNewPending(confirmed.number, conversionFactor);
			await pruneConsumed(confirmed.number);
			const dailyCap = await publicClient.readContract({
				address: config.vaultAddress,
				abi: vaultAbi,
				functionName: "dailyCap",
				blockNumber: confirmed.number,
			});
			await drainPending(releaser, conversionFactor, dailyCap);
			saveState();
			log(
				`ok: ${pending.size} pending, scanned ${config.baseFinalityTag} through block ${fromBlock - 1n} ` +
					`(eth_getLogs span ${logSpan.size})`,
			);
			void heartbeat();
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
	await alert("releaser startup failed", `${error}`, "critical");
	process.exit(1);
});
