/**
 * PEN migration attestor daemon (PRD §6.4).
 *
 * Watches RELAY-FINALIZED blocks on the operator's own Pendulum node for
 * `tokenMigration.MigrationInitiated` events and submits the matching
 * `approve(nonce, recipient, palletAmount)` transaction to the MigrationVault
 * on Base. The vault releases the tokens on the threshold-th approval.
 *
 * Design invariants:
 * - Only finalized blocks are read; blocks are processed strictly in order.
 * - Processing acts on the LATEST Base state (submission, race detection);
 *   the durable checkpoint advances only once every releasable event of a
 *   block is resolved inside the configured Base `safe`/`finalized` boundary.
 *   Splitting the two keeps throughput at submission latency — while every
 *   lost k-of-n race stays what it is, the most ordinary event in the system,
 *   never an alert — yet a crash can only ever re-process blocks whose
 *   approvals were not durable, which is idempotent (duplicate approvals
 *   revert harmlessly and are skipped by the pre-check).
 * - A decode failure is FATAL by design (PRD A5): the daemon alerts and
 *   exits rather than silently skipping an event; the checkpoint keeps the
 *   failing block next in line for after the operator intervenes.
 */

import { ApiPromise, WsProvider } from "@polkadot/api";
import { createPublicClient, defineChain, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { alertText, pingHeartbeat, postAlert, type Severity } from "./alert.js";
import { advanceCheckpoint, processBlock, viemVaultChain, type Attestor, type UnconfirmedBlock } from "./approval.js";
import { decodeMigrationEvent, isTransientRpcError, type MigrationEvent } from "./checks.js";
import { config } from "./config.js";
import {
	assertBaseChain,
	assertCheckpointIdentity,
	assertPendulumGenesis,
	loadCheckpoint,
	saveCheckpoint,
	type Checkpoint,
} from "./state.js";
import { vaultAbi } from "./vaultAbi.js";

/** Repeated transient failures (a sustained RPC outage) page at most this
 *  often; every occurrence is still logged. */
const TRANSIENT_ALERT_INTERVAL_MS = 60_000;

const baseChain = defineChain({
	id: config.baseChainId,
	name: "base",
	nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
	rpcUrls: { default: { http: [config.baseRpcUrl] } },
});

const account = privateKeyToAccount(config.attestorPrivateKey);
const publicClient = createPublicClient({ chain: baseChain, transport: http(config.baseRpcUrl) });

function log(message: string, extra?: unknown): void {
	console.log(`${new Date().toISOString()} ${message}`, extra ?? "");
}

/** Strip URLs before anything leaves the process: RPC endpoints commonly embed
 *  API keys, and viem error texts quote the endpoint verbatim. */
function redact(text: string): string {
	return text.replace(/https?:\/\/[^\s"')]+/gi, "<url>");
}

async function alert(subject: string, detail: unknown, severity: Severity = "warning"): Promise<void> {
	console.error(`${new Date().toISOString()} ALERT: ${subject}`, detail);
	if (!config.alertWebhookUrl) return;
	const redacted = redact(`${detail}`);
	await postAlert(config.alertWebhookUrl, {
		text: alertText(severity, `pen-attestor ${account.address}`, subject, redacted),
		severity,
		service: "pen-attestor",
		attestor: account.address,
		subject,
		detail: redacted,
	});
}

/** Extract MigrationInitiated events from one finalized Pendulum block. */
async function migrationEventsInBlock(api: ApiPromise, blockNumber: number): Promise<MigrationEvent[]> {
	const blockHash = await api.rpc.chain.getBlockHash(blockNumber);
	const apiAt = await api.at(blockHash);
	const records = (await apiAt.query.system.events()) as unknown as {
		event: { section: string; method: string; data: unknown[] };
	}[];

	const events: MigrationEvent[] = [];
	for (const record of records) {
		const { section, method, data } = record.event;
		if (section !== "tokenMigration" || method !== "MigrationInitiated") continue;
		events.push(decodeMigrationEvent(data, blockNumber));
	}
	return events;
}

const attestor: Attestor = {
	chain: viemVaultChain(publicClient, account, config.vaultAddress, config.baseFinalityTag),
	account,
	baseChainId: config.baseChainId,
	vaultAddress: config.vaultAddress,
	baseFinalityTag: config.baseFinalityTag,
	baseFinalityTimeoutMs: config.baseFinalityTimeoutMs,
	missedApprovalAlertCount: config.missedApprovalAlertCount,
	feeBump: config.feeBump,
	alert,
	log,
	now: Date.now,
	sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
	missedApprovals: 0,
	submissions: new Map(),
	nextNonce: 0,
};

async function checkGasBalance(): Promise<void> {
	const balance = await publicClient.getBalance({ address: account.address });
	if (balance < config.minGasBalanceWei) {
		await alert("gas balance low", `${account.address} holds ${balance} wei`);
	}
}

async function main(): Promise<void> {
	const baseChainId = await publicClient.getChainId();
	assertBaseChain(baseChainId, config.baseChainId);
	const isAttestor = await publicClient.readContract({
		address: config.vaultAddress,
		abi: vaultAbi,
		functionName: "isAttestor",
		args: [account.address],
	});
	if (!isAttestor) {
		throw new Error(`${account.address} is not an attestor of ${config.vaultAddress}`);
	}
	const attestorGeneration = Number(
		await publicClient.readContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "attestorGeneration",
			args: [account.address],
		}),
	);
	await checkGasBalance();

	const api = await ApiPromise.create({ provider: new WsProvider(config.pendulumWs) });
	const pendulumGenesisHash = api.genesisHash.toHex();
	assertPendulumGenesis(pendulumGenesisHash, config.pendulumGenesisHash);
	const checkpoint = loadCheckpoint(config.checkpointFile) ?? {
		version: 1,
		baseChainId,
		vaultAddress: config.vaultAddress,
		pendulumGenesisHash,
		attestorAddress: account.address,
		attestorGeneration,
		lastProcessedBlock: config.startBlock - 1,
	} satisfies Checkpoint;
	assertCheckpointIdentity(checkpoint, {
		...config,
		baseChainId,
		pendulumGenesisHash,
		attestorAddress: account.address,
		attestorGeneration,
	});
	// A checkpoint written before the signer was recorded is bound to it from
	// the next save on.
	checkpoint.attestorAddress ??= account.address;
	checkpoint.attestorGeneration ??= attestorGeneration;
	log(`attestor ${account.address} (generation ${attestorGeneration}) starting after block ${checkpoint.lastProcessedBlock}`);

	/** Blocks processed at the latest Base state, oldest first, that the
	 *  checkpoint has not yet passed. Bounded by the Base finality lag. */
	const unconfirmed: UnconfirmedBlock[] = [];
	let processedThrough = checkpoint.lastProcessedBlock;
	let lastTransientAlertMs = 0;

	let lastHeadAt = Date.now();
	let lastHeadStallAlertMs = 0;
	let processing = Promise.resolve();
	await api.rpc.chain.subscribeFinalizedHeads((head) => {
		lastHeadAt = Date.now();
		const finalized = head.number.toNumber();
		// Serialize: a slow Base transaction must not let block processing overlap.
		processing = processing.then(async () => {
			for (let block = processedThrough + 1; block <= finalized; block++) {
				await processBlock(attestor, block, await migrationEventsInBlock(api, block), unconfirmed);
				processedThrough = block;
			}
			if (await advanceCheckpoint(attestor, unconfirmed, checkpoint)) saveCheckpoint(config.checkpointFile, checkpoint);
			if (config.heartbeatUrl) void pingHeartbeat(config.heartbeatUrl);
		}).catch(async (error) => {
			if (isTransientRpcError(error)) {
				// Not a statement about any event — resume from the in-memory
				// position on the next finalized head. Sustained outages page
				// at a bounded rate instead of once per head.
				if (Date.now() - lastTransientAlertMs >= TRANSIENT_ALERT_INTERVAL_MS) {
					lastTransientAlertMs = Date.now();
					await alert("transient RPC failure, retrying on the next finalized head", error);
				} else {
					log("transient RPC failure, retrying on the next finalized head", error);
				}
				return;
			}
			// PRD A5: never skip an event silently. Alert and exit; the process
			// manager restarts us and the checkpoint retries from the last block
			// whose approvals were durable.
			await alert("fatal error, exiting", error, "critical");
			process.exit(1);
		});
	});

	setInterval(
		() => void checkGasBalance().catch((error) => console.error("gas balance check failed", error)),
		10 * 60 * 1000,
	);
	// Liveness watchdog for the push-driven loop above: silence is the one
	// failure the subscription cannot report on its own.
	setInterval(() => {
		const silentMs = Date.now() - lastHeadAt;
		if (silentMs < config.headStallAlertMs || Date.now() - lastHeadStallAlertMs < config.headStallAlertMs) return;
		lastHeadStallAlertMs = Date.now();
		void alert(
			"no finalized Pendulum head received",
			`${Math.round(silentMs / 1000)}s since the last finalized head; the node may have stopped finalizing or the subscription silently died`,
			"critical",
		);
	}, 60_000);
}

main().catch(async (error) => {
	await alert("startup failed", error, "critical");
	process.exit(1);
});
