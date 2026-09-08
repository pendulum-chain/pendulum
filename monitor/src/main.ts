/**
 * Independent PEN migration invariant monitor (PRD §6.5).
 *
 * The monitor owns durable cursors on both chains. Finalized Pendulum
 * MigrationInitiated events are the canonical nonce/recipient/amount records;
 * every safe Base Approved and Released event must match one of those records
 * exactly. Aggregate conservation remains a second, independent defence.
 */

import { ApiPromise, WsProvider } from "@polkadot/api";
import {
	createPublicClient,
	createWalletClient,
	defineChain,
	encodeAbiParameters,
	http,
	keccak256,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import {
	approvalMismatch,
	blockRanges,
	chunks,
	isStale,
	provablyUnsourced,
	releaseMismatch,
	releasedExceedsMigrated,
	shouldAwaitSource,
	type MigrationTuple,
	vaultConservationDeficit,
} from "./checks.js";
import {
	assertMonitorStateIdentity,
	loadMonitorState,
	saveMonitorState,
	type PersistedMonitorState,
} from "./state.js";

const MULTICALL3_ADDRESS = "0xcA11bde05977b3631167028862bE2a173976CA11" as const;

const vaultAbi = [
	{ type: "function", name: "totalReleased", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
	{ type: "function", name: "totalSwept", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
	{ type: "function", name: "conversionFactor", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
	{ type: "function", name: "token", stateMutability: "view", inputs: [], outputs: [{ type: "address" }] },
	{ type: "function", name: "paused", stateMutability: "view", inputs: [], outputs: [{ type: "bool" }] },
	{ type: "function", name: "threshold", stateMutability: "view", inputs: [], outputs: [{ type: "uint256" }] },
	{
		type: "function",
		name: "activeApprovals",
		stateMutability: "view",
		inputs: [{ name: "payload", type: "bytes32" }],
		outputs: [{ type: "uint256" }],
	},
	{ type: "function", name: "pause", stateMutability: "nonpayable", inputs: [], outputs: [] },
	{
		type: "event",
		name: "Approved",
		inputs: [
			{ name: "nonce", type: "uint64", indexed: true },
			{ name: "recipient", type: "address", indexed: true },
			{ name: "palletAmount", type: "uint256", indexed: false },
			{ name: "attestor", type: "address", indexed: true },
		],
	},
	{
		type: "event",
		name: "Released",
		inputs: [
			{ name: "nonce", type: "uint64", indexed: true },
			{ name: "recipient", type: "address", indexed: true },
			{ name: "palletAmount", type: "uint256", indexed: false },
			{ name: "tokenAmount", type: "uint256", indexed: false },
		],
	},
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

/** A numeric environment value, validated at startup: a typo must fail loudly
 *  here, not surface later as NaN-driven behavior (a NaN grace, for instance,
 *  would silently disable the very checks this daemon exists for). */
function envNumber(name: string, fallback: number | undefined): number {
	const raw = process.env[name];
	if (raw === undefined || raw === "") {
		if (fallback === undefined) throw new Error(`Missing required environment variable ${name}`);
		return fallback;
	}
	const value = Number(raw);
	if (!Number.isFinite(value) || value < 0) throw new Error(`invalid ${name}: ${raw}`);
	return value;
}

function finalityTag(): "safe" | "finalized" {
	const value = process.env.BASE_FINALITY_TAG ?? "safe";
	if (value !== "safe" && value !== "finalized") throw new Error(`invalid BASE_FINALITY_TAG ${value}`);
	return value;
}

const baseFinalityTag = finalityTag();

const config = {
	pendulumWs: required("PENDULUM_WS"),
	baseRpcUrl: required("BASE_RPC_URL"),
	vaultAddress: required("VAULT_ADDRESS") as `0x${string}`,
	pollIntervalMs: envNumber("POLL_INTERVAL_MS", 60_000),
	graceSeconds: envNumber("GRACE_SECONDS", 1800),
	unmatchedEventGraceSeconds: envNumber("UNMATCHED_EVENT_GRACE_SECONDS", 600),
	alertWebhookUrl: process.env.ALERT_WEBHOOK_URL,
	guardianPrivateKey: process.env.GUARDIAN_PRIVATE_KEY as `0x${string}` | undefined,
	baseChainId: envNumber("BASE_CHAIN_ID", 8453),
	baseFinalityTag,
	baseFinalityPollMs: envNumber("BASE_FINALITY_POLL_MS", 2000),
	/** Bound on confirming the auto-pause inside the finality boundary;
	 *  `finalized` trails `safe` by L1 finality, hence the longer default. */
	baseFinalityTimeoutMs: envNumber(
		"BASE_FINALITY_TIMEOUT_MS",
		baseFinalityTag === "finalized" ? 2_700_000 : 900_000,
	),
	pendulumStartBlock: envNumber("PENDULUM_START_BLOCK", undefined),
	pendulumStartNonce: BigInt(process.env.PENDULUM_START_NONCE ?? "0"),
	baseStartBlock: BigInt(required("BASE_START_BLOCK")),
	baseMaxBlockRange: BigInt(process.env.BASE_MAX_BLOCK_RANGE ?? "10000"),
	readBatchSize: envNumber("READ_BATCH_SIZE", 100),
	stateFile: process.env.STATE_FILE ?? "./monitor-state.json",
	/** Cross-chain clock-skew allowance for `provablyUnsourced`: how far the
	 *  finalized Pendulum view must pass a Base event's timestamp before a
	 *  missing nonce counts as proof of fabrication rather than lag. */
	sourceClockSkewMarginMs: envNumber("SOURCE_CLOCK_SKEW_MARGIN_SECONDS", 120) * 1000,
	/** Age of the finalized Pendulum view (vs. wall clock) past which the
	 *  monitor pages that it is going blind — BEFORE any unmatched event can
	 *  reach the grace deadline, so operators get the whole grace window to
	 *  restore the node ahead of a cannot-verify auto-pause. */
	sourceStaleAlertMs: envNumber("SOURCE_STALE_ALERT_SECONDS", 300) * 1000,
	/** Growth of (Pendulum totalIssuance + TotalMigrated) over its first-seen
	 *  anchor, in 12-decimal pallet units, tolerated before paging. */
	issuanceToleranceRaw: BigInt(process.env.ISSUANCE_TOLERANCE ?? "0"),
};

const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";

const baseChain = defineChain({
	id: config.baseChainId,
	name: "base",
	nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
	rpcUrls: { default: { http: [config.baseRpcUrl] } },
	contracts: { multicall3: { address: MULTICALL3_ADDRESS } },
});
const publicClient = createPublicClient({ chain: baseChain, transport: http(config.baseRpcUrl) });

interface MigrationRecord extends MigrationTuple {
	firstSeenMs: number;
	sourceBlock: number;
}

interface RuntimeState {
	baseChainId: number;
	vaultAddress: `0x${string}`;
	pendulumGenesisHash: `0x${string}` | undefined;
	lastPendulumBlock: number;
	nextExpectedNonce: bigint;
	baseFromBlock: bigint;
	migrations: Map<bigint, MigrationRecord>;
	livenessAlertedAt: Map<bigint, number>;
	unmatchedBaseFirstSeenAt: Map<string, number>;
	/** Pendulum totalIssuance + TotalMigrated at first observation. Under every
	 *  legitimate flow this sum can only fall (teleport-out) and later recover
	 *  to at most its original value (teleport-in); growth means PEN was minted
	 *  at the source, which honest attestation would then release from the
	 *  fixed-supply vault. Alert-only: the response is a human decision. */
	issuanceAnchor: bigint | undefined;
}

function hydrateState(): RuntimeState {
	const persisted = loadMonitorState(config.stateFile);
	if (!persisted) {
		return {
			baseChainId: config.baseChainId,
			vaultAddress: config.vaultAddress,
			pendulumGenesisHash: undefined,
			lastPendulumBlock: config.pendulumStartBlock - 1,
			nextExpectedNonce: config.pendulumStartNonce,
			baseFromBlock: config.baseStartBlock,
			migrations: new Map(),
			livenessAlertedAt: new Map(),
			unmatchedBaseFirstSeenAt: new Map(),
			issuanceAnchor: undefined,
		};
	}
	return {
		baseChainId: persisted.baseChainId,
		vaultAddress: persisted.vaultAddress,
		pendulumGenesisHash: persisted.pendulumGenesisHash,
		lastPendulumBlock: persisted.lastPendulumBlock,
		nextExpectedNonce: BigInt(persisted.nextExpectedNonce),
		baseFromBlock: BigInt(persisted.baseFromBlock),
		migrations: new Map(persisted.migrations.map((migration) => [
			BigInt(migration.nonce),
			{
				nonce: BigInt(migration.nonce),
				recipient: migration.recipient,
				palletAmount: BigInt(migration.palletAmount),
				firstSeenMs: migration.firstSeenMs,
				sourceBlock: migration.sourceBlock,
			},
		])),
		livenessAlertedAt: new Map(persisted.livenessAlertedAt.map(([nonce, at]) => [BigInt(nonce), at])),
		unmatchedBaseFirstSeenAt: new Map(persisted.unmatchedBaseFirstSeenAt),
		issuanceAnchor: persisted.issuanceAnchor === undefined ? undefined : BigInt(persisted.issuanceAnchor),
	};
}

function cloneState(source: RuntimeState): RuntimeState {
	return {
		baseChainId: source.baseChainId,
		vaultAddress: source.vaultAddress,
		pendulumGenesisHash: source.pendulumGenesisHash,
		lastPendulumBlock: source.lastPendulumBlock,
		nextExpectedNonce: source.nextExpectedNonce,
		baseFromBlock: source.baseFromBlock,
		migrations: new Map(source.migrations),
		livenessAlertedAt: new Map(source.livenessAlertedAt),
		unmatchedBaseFirstSeenAt: new Map(source.unmatchedBaseFirstSeenAt),
		issuanceAnchor: source.issuanceAnchor,
	};
}

function persistState(state: RuntimeState): void {
	if (!state.pendulumGenesisHash) throw new Error("cannot persist monitor state before Pendulum identity is known");
	const persisted: PersistedMonitorState = {
		version: 1,
		baseChainId: state.baseChainId,
		vaultAddress: state.vaultAddress,
		pendulumGenesisHash: state.pendulumGenesisHash,
		lastPendulumBlock: state.lastPendulumBlock,
		nextExpectedNonce: state.nextExpectedNonce.toString(),
		baseFromBlock: state.baseFromBlock.toString(),
		migrations: [...state.migrations.values()].map((migration) => ({
			nonce: migration.nonce.toString(),
			recipient: migration.recipient as `0x${string}`,
			palletAmount: migration.palletAmount.toString(),
			firstSeenMs: migration.firstSeenMs,
			sourceBlock: migration.sourceBlock,
		})),
		livenessAlertedAt: [...state.livenessAlertedAt].map(([nonce, at]) => [nonce.toString(), at]),
		unmatchedBaseFirstSeenAt: [...state.unmatchedBaseFirstSeenAt],
		issuanceAnchor: state.issuanceAnchor?.toString(),
	};
	saveMonitorState(config.stateFile, persisted);
}

let state: RuntimeState;
let multicallUnavailable = false;
let lastSourceStallAlertMs = 0;
let lastIssuanceAlertMs = 0;

function log(message: string): void {
	console.log(`${new Date().toISOString()} ${message}`);
}

/** Strip URLs before anything leaves the process: RPC endpoints commonly embed
 *  API keys, and viem error texts quote the endpoint verbatim. */
function redact(text: string): string {
	return text.replace(/https?:\/\/[^\s"')]+/gi, "<url>");
}

async function alert(subject: string, detail: string): Promise<void> {
	console.error(`${new Date().toISOString()} ALERT: ${subject} — ${detail}`);
	if (!config.alertWebhookUrl) return;
	try {
		await fetch(config.alertWebhookUrl, {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify({ service: "pen-monitor", subject, detail: redact(detail) }),
			// The webhook is an external dependency; it must never stall the
			// check loop (or the pause that follows a violation) indefinitely.
			signal: AbortSignal.timeout(10_000),
		});
	} catch (webhookError) {
		console.error("alert webhook failed", webhookError);
	}
}

async function waitForBaseFinality(
	hash: `0x${string}`,
	receiptBlock: bigint,
	receiptBlockHash: `0x${string}`,
): Promise<bigint> {
	const deadline = Date.now() + config.baseFinalityTimeoutMs;
	for (;;) {
		const confirmed = await publicClient.getBlock({ blockTag: config.baseFinalityTag });
		if (confirmed.number !== null && confirmed.number >= receiptBlock) {
			const canonicalReceipt = await publicClient.getTransactionReceipt({ hash });
			if (canonicalReceipt.status !== "success" || canonicalReceipt.blockHash !== receiptBlockHash) {
				throw new Error(`pause transaction ${hash} was reorged before becoming ${config.baseFinalityTag}`);
			}
			return confirmed.number;
		}
		if (Date.now() >= deadline) {
			throw new Error(`timed out waiting for pause ${hash} to become ${config.baseFinalityTag}`);
		}
		await new Promise((resolve) => setTimeout(resolve, config.baseFinalityPollMs));
	}
}

async function pauseVault(): Promise<boolean> {
	if (!config.guardianPrivateKey) {
		await alert("AUTO-PAUSE UNAVAILABLE", "no GUARDIAN_PRIVATE_KEY configured; pause manually NOW");
		return false;
	}
	try {
		const confirmed = await publicClient.getBlock({ blockTag: config.baseFinalityTag });
		if (confirmed.number === null) throw new Error(`${config.baseFinalityTag} block has no number`);
		const pausedConfirmed = await publicClient.readContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "paused",
			blockNumber: confirmed.number,
		});
		if (pausedConfirmed) {
			await alert("vault pause confirmed", `already paused in Base ${config.baseFinalityTag} block ${confirmed.number}`);
			return true;
		}
		const pausedLatest = await publicClient.readContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "paused",
		});
		if (pausedLatest) {
			await alert("AUTO-PAUSE PENDING FINALITY", `pause is mined but not yet ${config.baseFinalityTag}`);
			return false;
		}

		const guardian = privateKeyToAccount(config.guardianPrivateKey);
		const walletClient = createWalletClient({ account: guardian, chain: baseChain, transport: http(config.baseRpcUrl) });
		const { request } = await publicClient.simulateContract({
			account: guardian,
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "pause",
		});
		const txHash = await walletClient.writeContract(request);
		const receipt = await publicClient.waitForTransactionReceipt({ hash: txHash });
		if (receipt.status !== "success") throw new Error(`pause transaction reverted: ${txHash}`);
		const safeBlock = await waitForBaseFinality(txHash, receipt.blockNumber, receipt.blockHash);
		const paused = await publicClient.readContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "paused",
			blockNumber: safeBlock,
		});
		if (!paused) throw new Error(`pause transaction ${txHash} is confirmed but paused() is false`);
		await alert("vault auto-pause confirmed", `tx=${txHash} ${config.baseFinalityTag}Block=${safeBlock}`);
		return true;
	} catch (pauseError) {
		await alert("AUTO-PAUSE FAILED", `${pauseError}; pause manually NOW`);
		return false;
	}
}

async function securityViolation(subject: string, detail: string): Promise<boolean> {
	// Pause FIRST. The alert webhook is an external dependency with its own
	// latency; nothing may sit between detecting a violation and the pause.
	const notified = alert(subject, detail);
	const paused = await pauseVault();
	await notified;
	return paused;
}

/**
 * Decide what to do with a safe Base event whose nonce the finalized Pendulum
 * view does not (yet) know.
 *
 * Three outcomes:
 *  - "proven":  the source view has passed the moment this event was first
 *               observed and the nonce still does not exist — fabricated,
 *               pause immediately (the RPC-lag grace must not extend an
 *               attacker's dwell time);
 *  - "await":   the source view still trails the event and the grace has not
 *               expired — legitimate node lag looks exactly like this;
 *  - "expired": the grace ran out with the source still behind — the monitor
 *               has been unable to verify releases for the whole window.
 *
 * The first observation of an unmatched event pages immediately (deduplicated
 * via the persisted first-seen map), so operators get the entire grace window
 * to repair a lagging source node before "expired" forces the pause.
 */
async function unmatchedDisposition(
	kind: "Approved" | "Released",
	key: string,
	nonce: bigint,
	eventBlockNumber: bigint,
	draft: RuntimeState,
	observedAt: number,
	sourceFinalizedTsMs: number,
): Promise<"proven" | "await" | "expired"> {
	const alreadyKnown = draft.unmatchedBaseFirstSeenAt.has(key);
	const firstSeen = draft.unmatchedBaseFirstSeenAt.get(key) ?? observedAt;
	draft.unmatchedBaseFirstSeenAt.set(key, firstSeen);
	if (!alreadyKnown) {
		await alert(
			"UNVERIFIED BASE EVENT",
			`${kind} nonce ${nonce} at Base block ${eventBlockNumber} has no finalized Pendulum source yet; ` +
				`waiting up to ${config.unmatchedEventGraceSeconds}s for the source view to reveal its burn`,
		);
	}
	if (provablyUnsourced(firstSeen, sourceFinalizedTsMs, observedAt, config.sourceClockSkewMarginMs)) return "proven";
	return shouldAwaitSource(nonce, draft.nextExpectedNonce, firstSeen, observedAt, config.unmatchedEventGraceSeconds)
		? "await"
		: "expired";
}

async function ingestPendulumEvents(api: ApiPromise, draft: RuntimeState, finalizedBlock: number): Promise<void> {
	for (let block = draft.lastPendulumBlock + 1; block <= finalizedBlock; block++) {
		const blockHash = await api.rpc.chain.getBlockHash(block);
		const apiAt = await api.at(blockHash);
		const firstSeenMs = Number((await apiAt.query.timestamp.now()).toString().replaceAll(",", ""));
		const records = (await apiAt.query.system.events()) as unknown as {
			event: { section: string; method: string; data: unknown[] };
		}[];
		for (const record of records) {
			const { section, method, data } = record.event;
			if (section !== "tokenMigration" || method !== "MigrationInitiated") continue;
			if (data.length !== 4) throw new Error(`MigrationInitiated in block ${block} has ${data.length} fields, expected 4`);
			const [nonceValue, , recipientValue, amountValue] = data as [
				{ toBigInt(): bigint },
				unknown,
				{ toHex(): string },
				{ toBigInt(): bigint },
			];
			const nonce = nonceValue.toBigInt();
			const recipient = recipientValue.toHex() as `0x${string}`;
			if (!/^0x[0-9a-fA-F]{40}$/.test(recipient)) {
				throw new Error(`cannot decode base_address in block ${block}: ${recipient}`);
			}
			if (nonce !== draft.nextExpectedNonce) {
				throw new Error(`migration nonce gap: finalized event ${nonce}, expected ${draft.nextExpectedNonce}`);
			}
			draft.nextExpectedNonce++;
			if (recipient === ZERO_ADDRESS || recipient.toLowerCase() === config.vaultAddress.toLowerCase()) {
				// The vault deterministically rejects this tuple (ZeroAddress /
				// RecipientIsVault) and the attestors skip it, so it can never be
				// approved or released. Tracking it would keep the pending count
				// non-zero forever and page liveness every grace period (and make
				// RB-7's "zero outstanding nonces" unsatisfiable). Record it once
				// and leave it out of reconciliation.
				await alert(
					"CRITICAL: unreleasable migration burned on Pendulum",
					`nonce ${nonce} in block ${block} burned ${amountValue.toBigInt()} to ${recipient}; the vault can never release it`,
				);
				continue;
			}
			draft.migrations.set(nonce, {
				nonce,
				recipient,
				palletAmount: amountValue.toBigInt(),
				firstSeenMs,
				sourceBlock: block,
			});
		}
		draft.lastPendulumBlock = block;
		// Checkpoint a long catch-up as it goes: the Pendulum-side cursor is
		// self-consistent on its own (the Base cursor only ever trails it), so a
		// transient RPC error hours into a replay must not discard the progress
		// and restart the whole replay — against a flaky endpoint that never
		// converges, leaving the monitor blind indefinitely.
		if (block % 200 === 0) {
			state = cloneState(draft);
			persistState(state);
		}
	}
}

async function reconcileBaseEvents(
	draft: RuntimeState,
	toBlock: bigint,
	conversionFactor: bigint,
	sourceFinalizedTsMs: number,
): Promise<{ violation?: string; awaitingSource?: string }> {
	const releasedInScan = new Set<bigint>();
	let firstViolation: string | undefined;
	for (const [start, end] of blockRanges(draft.baseFromBlock, toBlock, config.baseMaxBlockRange)) {
		// Probe the range's end block first. Against a load-balanced endpoint,
		// `eth_getLogs` can be served by a node that has not reached `end` yet
		// and some providers then silently truncate rather than error — which
		// would advance the cursor past events this monitor never reconciled. A
		// lagging node fails this probe loudly instead, and the cycle replays.
		await publicClient.getBlock({ blockNumber: end });
		const approvals = await publicClient.getContractEvents({
			address: config.vaultAddress,
			abi: vaultAbi,
			eventName: "Approved",
			fromBlock: start,
			toBlock: end,
		});
		const releases = await publicClient.getContractEvents({
			address: config.vaultAddress,
			abi: vaultAbi,
			eventName: "Released",
			fromBlock: start,
			toBlock: end,
		});
		const observedAt = Date.now();
		let awaitingSource: string | undefined;

		// Preflight the complete range before deleting released migrations. If a
		// source RPC is merely behind, the range must be replayed intact later.
		for (const event of approvals) {
			const actual = event.args as { nonce: bigint; recipient: `0x${string}`; palletAmount: bigint };
			const expected = draft.migrations.get(actual.nonce);
			const mismatch = approvalMismatch(expected, actual);
			const key = `Approved:${actual.nonce}`;
			if (!mismatch) {
				draft.unmatchedBaseFirstSeenAt.delete(key);
				continue;
			}
			if (!expected) {
				const disposition = await unmatchedDisposition(
					"Approved",
					key,
					actual.nonce,
					event.blockNumber,
					draft,
					observedAt,
					sourceFinalizedTsMs,
				);
				if (disposition === "await") {
					awaitingSource ??= `Approved nonce ${actual.nonce} at Base block ${event.blockNumber}`;
					continue;
				}
				firstViolation ??= `Approved at Base block ${event.blockNumber}: ${mismatch} (` +
					(disposition === "proven"
						? "source view has passed the event's timestamp — fabricated"
						: "source view still behind after the full grace period") + ")";
				continue;
			}
			firstViolation ??= `Approved at Base block ${event.blockNumber}: ${mismatch}`;
		}
		for (const event of releases) {
			const actual = event.args as {
				nonce: bigint;
				recipient: `0x${string}`;
				palletAmount: bigint;
				tokenAmount: bigint;
			};
			const expected = draft.migrations.get(actual.nonce);
			const mismatch = releaseMismatch(expected, actual, conversionFactor);
			const key = `Released:${actual.nonce}`;
			if (!mismatch) {
				draft.unmatchedBaseFirstSeenAt.delete(key);
				continue;
			}
			if (!expected) {
				const disposition = await unmatchedDisposition(
					"Released",
					key,
					actual.nonce,
					event.blockNumber,
					draft,
					observedAt,
					sourceFinalizedTsMs,
				);
				if (disposition === "await") {
					awaitingSource ??= `Released nonce ${actual.nonce} at Base block ${event.blockNumber}`;
					continue;
				}
				firstViolation ??= `Released at Base block ${event.blockNumber}: ${mismatch} (` +
					(disposition === "proven"
						? "source view has passed the event's timestamp — fabricated"
						: "source view still behind after the full grace period") + ")";
				continue;
			}
			firstViolation ??= `Released at Base block ${event.blockNumber}: ${mismatch}`;
		}
		if (awaitingSource && !firstViolation) return { awaitingSource };

		for (const event of releases) {
			const actual = event.args as {
				nonce: bigint;
				recipient: `0x${string}`;
				palletAmount: bigint;
				tokenAmount: bigint;
			};
			if (releasedInScan.has(actual.nonce)) {
				firstViolation ??= `duplicate Released event for nonce ${actual.nonce}`;
				continue;
			}
			const mismatch = releaseMismatch(draft.migrations.get(actual.nonce), actual, conversionFactor);
			if (mismatch) {
				firstViolation ??= `Released at Base block ${event.blockNumber}: ${mismatch}`;
				continue;
			}
			releasedInScan.add(actual.nonce);
			draft.migrations.delete(actual.nonce);
			draft.livenessAlertedAt.delete(actual.nonce);
		}
		draft.baseFromBlock = end + 1n;
	}
	return { violation: firstViolation };
}

function payloadHash(migration: MigrationRecord): `0x${string}` {
	return keccak256(encodeAbiParameters(
		[{ type: "uint64" }, { type: "address" }, { type: "uint256" }],
		[migration.nonce, migration.recipient as `0x${string}`, migration.palletAmount],
	));
}

async function activeApprovalCounts(migrations: MigrationRecord[], blockNumber: bigint): Promise<bigint[]> {
	const counts: bigint[] = [];
	for (const batch of chunks(migrations, config.readBatchSize)) {
		const single = (migration: MigrationRecord) => publicClient.readContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "activeApprovals",
			args: [payloadHash(migration)],
			blockNumber,
		});
		if (!multicallUnavailable) {
			try {
				const result = await publicClient.multicall({
					allowFailure: false,
					blockNumber,
					contracts: batch.map((migration) => ({
						address: config.vaultAddress,
						abi: vaultAbi,
						functionName: "activeApprovals" as const,
						args: [payloadHash(migration)] as const,
					})),
				});
				counts.push(...(result as bigint[]));
				continue;
			} catch (error) {
				await alert("multicall batch failed, using bounded reads for this batch", `${error}`);
			}
		}
		counts.push(...(await Promise.all(batch.map(single))));
	}
	return counts;
}

async function check(api: ApiPromise): Promise<void> {
	const confirmedBase = await publicClient.getBlock({ blockTag: config.baseFinalityTag });
	if (confirmedBase.number === null) throw new Error(`${config.baseFinalityTag} Base block has no number`);
	const baseBlock = confirmedBase.number;

	// Read Pendulum after choosing the Base snapshot. TotalMigrated can only
	// grow, so this ordering cannot false-positive if a burn finalizes mid-poll.
	const finalizedHash = await api.rpc.chain.getFinalizedHead();
	const finalizedHeader = await api.rpc.chain.getHeader(finalizedHash);
	const finalizedBlock = finalizedHeader.number.toNumber();
	const apiAt = await api.at(finalizedHash);
	const sourceFinalizedTsMs = Number((await apiAt.query.timestamp.now()).toString().replaceAll(",", ""));
	// A stalling source view is this daemon going blind: it cannot verify any
	// new Base event, and once the unmatched-event grace expires that forces a
	// cannot-verify pause. Page as soon as the view goes stale, so operators
	// get the entire grace window to restore the node before that happens.
	if (
		Date.now() - sourceFinalizedTsMs > config.sourceStaleAlertMs &&
		Date.now() - lastSourceStallAlertMs > config.unmatchedEventGraceSeconds * 1000
	) {
		lastSourceStallAlertMs = Date.now();
		await alert(
			"PENDULUM SOURCE VIEW STALLED",
			`finalized head ${finalizedBlock} is ${Math.round((Date.now() - sourceFinalizedTsMs) / 1000)}s old; ` +
				`the monitor cannot verify new Base events against a stalled source — restore the node before ` +
				`the ${config.unmatchedEventGraceSeconds}s unmatched-event grace forces a pause`,
		);
	}
	const draft = cloneState(state);
	await ingestPendulumEvents(api, draft, finalizedBlock);
	const [totalMigratedValue, chainNextNonceValue, totalIssuanceValue] = await Promise.all([
		apiAt.query.tokenMigration.totalMigrated(),
		apiAt.query.tokenMigration.nextNonce(),
		apiAt.query.balances.totalIssuance(),
	]);
	const totalMigrated = BigInt(totalMigratedValue.toString().replaceAll(",", ""));
	const chainNextNonce = BigInt(chainNextNonceValue.toString().replaceAll(",", ""));
	if (chainNextNonce !== draft.nextExpectedNonce) {
		throw new Error(`finalized nextNonce ${chainNextNonce} != monitor event cursor ${draft.nextExpectedNonce}`);
	}
	// Source-chain supply guard. Every burn of freshly MINTED Pendulum PEN (a
	// passed setBalance referendum, an unexpected teleport-in) is a genuine
	// finalized burn: attestors approve it honestly, every tuple matches, and
	// M2a/M2b stay satisfied — while the fixed-supply vault drains ahead of
	// honest late migrators. issuance + migrated cannot grow under any
	// legitimate flow, so growth over the first-seen anchor is the one signal
	// for this. Alert only: pausing is a human decision here.
	const totalIssuance = BigInt(totalIssuanceValue.toString().replaceAll(",", ""));
	const sourceSupply = totalIssuance + totalMigrated;
	if (draft.issuanceAnchor === undefined) {
		draft.issuanceAnchor = sourceSupply;
		log(`anchored source supply: issuance ${totalIssuance} + migrated ${totalMigrated} = ${sourceSupply}`);
	} else if (
		sourceSupply > draft.issuanceAnchor + config.issuanceToleranceRaw &&
		Date.now() - lastIssuanceAlertMs > config.graceSeconds * 1000
	) {
		lastIssuanceAlertMs = Date.now();
		await alert(
			"SOURCE SUPPLY GREW",
			`Pendulum issuance ${totalIssuance} + migrated ${totalMigrated} = ${sourceSupply} exceeds the anchor ` +
				`${draft.issuanceAnchor} by ${sourceSupply - draft.issuanceAnchor} pallet units: PEN was minted at the ` +
				`source and can be burned against the fixed-supply vault — investigate before it migrates (RB-3)`,
		);
	}

	const [totalReleased, totalSwept, conversionFactor, tokenAddress] = await Promise.all([
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "totalReleased", blockNumber: baseBlock }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "totalSwept", blockNumber: baseBlock }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "conversionFactor", blockNumber: baseBlock }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "token", blockNumber: baseBlock }),
	]);

	const reconciliation = await reconcileBaseEvents(draft, baseBlock, conversionFactor, sourceFinalizedTsMs);
	if (reconciliation.violation) {
		if (await securityViolation("MIGRATION TUPLE VIOLATION", reconciliation.violation)) {
			state = draft;
			persistState(state);
		}
		return;
	}

	const [totalSupply, vaultBalance] = await Promise.all([
		publicClient.readContract({ address: tokenAddress, abi: erc20Abi, functionName: "totalSupply", blockNumber: baseBlock }),
		publicClient.readContract({
			address: tokenAddress,
			abi: erc20Abi,
			functionName: "balanceOf",
			args: [config.vaultAddress],
			blockNumber: baseBlock,
		}),
	]);
	if (vaultConservationDeficit(vaultBalance, totalReleased, totalSwept, totalSupply)) {
		if (await securityViolation(
			"VAULT BALANCE DEFICIT",
			`balance ${vaultBalance} + released ${totalReleased} + swept ${totalSwept} < supply ${totalSupply}`,
		)) {
			state = draft;
			persistState(state);
		}
		return;
	}
	if (reconciliation.awaitingSource) {
		state = draft;
		persistState(state);
		log(
			`waiting up to ${config.unmatchedEventGraceSeconds}s for Pendulum RPC to catch up with ` +
				reconciliation.awaitingSource,
		);
		return;
	}
	if (releasedExceedsMigrated(totalReleased, totalMigrated, conversionFactor)) {
		if (await securityViolation(
			"CONSERVATION VIOLATION",
			`released ${totalReleased} > migrated ${totalMigrated * conversionFactor} (token units)`,
		)) {
			state = draft;
			persistState(state);
		}
		return;
	}

	const pending = [...draft.migrations.values()];
	const threshold = await publicClient.readContract({
		address: config.vaultAddress,
		abi: vaultAbi,
		functionName: "threshold",
		blockNumber: baseBlock,
	});
	const approvals = await activeApprovalCounts(pending, baseBlock);
	const now = Date.now();
	for (let index = 0; index < pending.length; index++) {
		const migration = pending[index];
		if (!isStale(migration.firstSeenMs, now, config.graceSeconds)) continue;
		const lastAlerted = draft.livenessAlertedAt.get(migration.nonce);
		if (lastAlerted !== undefined && now - lastAlerted < config.graceSeconds * 1000) continue;
		draft.livenessAlertedAt.set(migration.nonce, now);
		if (approvals[index] >= threshold) {
			// Quorum is present but nothing released it: cap-deferred and
			// starved of daily allowance, paused, or made releasable by a
			// threshold cut without ever emitting ReleasePending (so the
			// releaser cannot see it). Silent indefinitely before round 9.
			await alert(
				"LIVENESS: quorum reached but not released",
				`nonce ${migration.nonce} has ${approvals[index]}/${threshold} active approvals and is still unreleased ` +
					`after ${config.graceSeconds}s — check the daily allowance / pause state, or call release() directly ` +
					`if it qualified through a threshold cut (RB-6)`,
			);
			continue;
		}
		await alert(
			"LIVENESS: migration below approval quorum",
			`nonce ${migration.nonce} has ${approvals[index]}/${threshold} active approvals after ${config.graceSeconds}s`,
		);
	}

	state = draft;
	persistState(state);
	log(
		`ok: migrated=${totalMigrated} released=${totalReleased} swept=${totalSwept} ` +
			`pending=${state.migrations.size} vaultBalance=${vaultBalance} base=${baseBlock} pendulum=${finalizedBlock}`,
	);
}

async function main(): Promise<void> {
	state = hydrateState();
	const multicallCode = await publicClient.getBytecode({ address: MULTICALL3_ADDRESS });
	if (!multicallCode || multicallCode === "0x") {
		multicallUnavailable = true;
		await alert("multicall unavailable, using bounded individual reads", `no contract code at ${MULTICALL3_ADDRESS}`);
	}
	const api = await ApiPromise.create({ provider: new WsProvider(config.pendulumWs) });
	const pendulumGenesisHash = api.genesisHash.toHex();
	if (state.pendulumGenesisHash) {
		assertMonitorStateIdentity(
			{
				baseChainId: state.baseChainId,
				vaultAddress: state.vaultAddress,
				pendulumGenesisHash: state.pendulumGenesisHash,
			},
			{ ...config, pendulumGenesisHash },
		);
	} else {
		state.pendulumGenesisHash = pendulumGenesisHash;
	}
	log(
		`monitor started; Pendulum after ${state.lastPendulumBlock}, Base from ${state.baseFromBlock}, ` +
			`polling every ${config.pollIntervalMs}ms`,
	);
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
