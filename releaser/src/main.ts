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
import { blockRanges, chunks, classifyReleaseFailure, contractErrorName, toPalletAmount } from "./checks.js";
import { config } from "./config.js";
import {
	assertStateIdentity,
	loadState as readPersistedState,
	saveState as writePersistedState,
	type PersistedState,
} from "./state.js";
import { vaultAbi } from "./vaultAbi.js";

interface PendingRelease {
	nonce: bigint;
	recipient: `0x${string}`;
	palletAmount: bigint;
}

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

/** nonce -> pending release. Keyed by nonce: the vault consumes a nonce once. */
const pending = new Map<bigint, PendingRelease>();
let fromBlock = config.startBlock;

/** nonce -> when its "blocked" state was last alerted. A blocked release needs
 *  a timelocked governance action (>= 48h) to clear, so re-paging it every
 *  poll would emit thousands of identical alerts and train on-call to ignore
 *  them; re-alert at a bounded interval instead. */
const blockedAlertedAt = new Map<bigint, number>();

function log(message: string): void {
	console.log(`${new Date().toISOString()} ${message}`);
}

/** Strip URLs before anything leaves the process: RPC endpoints commonly embed
 *  API keys, and viem error texts quote the endpoint verbatim. */
function redact(text: string): string {
	return text.replace(/https?:\/\/[^\s"')]+/gi, "<url>");
}

/** A release transaction that was mined and reverted. The receipt carries no
 *  revert data, so the reason is unknowable here; the next cycle's simulation
 *  classifies it properly (and pruneConsumed drops it if a peer won). */
class MinedRevert extends Error {}

async function alert(subject: string, detail: string): Promise<void> {
	console.error(`${new Date().toISOString()} ALERT: ${subject} — ${detail}`);
	if (!config.alertWebhookUrl) return;
	try {
		await fetch(config.alertWebhookUrl, {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify({ service: "pen-releaser", releaser: account.address, subject, detail: redact(detail) }),
			signal: AbortSignal.timeout(10_000),
		});
	} catch (webhookError) {
		console.error("alert webhook failed", webhookError);
	}
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

/** Scan for newly deferred releases and add them to the pending set. */
async function ingestNewPending(toBlock: bigint, conversionFactor: bigint): Promise<void> {
	for (const [start, end] of blockRanges(fromBlock, toBlock, config.maxBlockRange)) {
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
	}
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

/** Try to push each pending release through; classify what comes back. */
async function drainPending(conversionFactor: bigint, dailyCap: bigint): Promise<void> {
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
			// Pad the gas limit: a release writes an ERC20Votes checkpoint for the
			// vault's vote sink, and whether that is an overwrite or a new entry
			// depends on the block timestamp the transaction lands in — a state
			// the estimate cannot know. An exact estimate can run out of gas in
			// that write (seen in phase 3); doubling it is cheap insurance.
			const gasLimit = (await publicClient.estimateContractGas({
				account,
				address: config.vaultAddress,
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
			log(`released (awaiting ${config.baseFinalityTag}): ${label} tx=${txHash}`);
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
			const confirmed = await publicClient.getBlock({ blockTag: config.baseFinalityTag });
			if (confirmed.number === null) throw new Error(`${config.baseFinalityTag} Base block has no number`);
			const consumedConfirmed = await publicClient.readContract({
				address: config.vaultAddress,
				abi: vaultAbi,
				functionName: "nonceConsumed",
				args: [p.nonce],
				blockNumber: confirmed.number,
			});
			const consumedLatest = consumedConfirmed ? true : await publicClient.readContract({
				address: config.vaultAddress,
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
					blockedAlertedAt.delete(p.nonce);
					log(`skip (already consumed): ${label}`);
					break;
				case "retry":
					// Expected while the daily bucket refills — stay quiet.
					break;
				case "blocked": {
					// Clearing this needs a timelocked governance action, so the
					// condition persists for days; page at a bounded interval.
					const lastAlerted = blockedAlertedAt.get(p.nonce) ?? 0;
					if (Date.now() - lastAlerted >= config.blockedAlertIntervalMs) {
						blockedAlertedAt.set(p.nonce, Date.now());
						await alert(
							"release blocked and needs operator action",
							`${label} — ${errorName}; it cannot self-heal`,
						);
					}
					break;
				}
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
			await drainPending(conversionFactor, dailyCap);
			saveState();
			log(`ok: ${pending.size} pending, scanned ${config.baseFinalityTag} through block ${fromBlock - 1n}`);
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
