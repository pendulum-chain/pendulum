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
	parseAbi,
	toFunctionSelector,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import {
	approvalMismatch,
	blockRanges,
	type CallNode,
	chunks,
	describeCall,
	describeTimelockOperation,
	isStale,
	provablyUnsourced,
	releaseMismatch,
	releasedExceedsMigrated,
	riskyCalls,
	type ScheduledCall,
	shouldAwaitSource,
	toCallNode,
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
	{ type: "function", name: "guardian", stateMutability: "view", inputs: [], outputs: [{ type: "address" }] },
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

/** The vault events of its admin (and of the guardian's pause). */
const vaultAdminEventsAbi = parseAbi([
	"event TokenSet(address indexed token)",
	"event Paused(address indexed by)",
	"event Unpaused(address indexed by)",
	"event AttestorAdded(address indexed attestor)",
	"event AttestorRemoved(address indexed attestor)",
	"event ThresholdUpdated(uint256 threshold)",
	"event CapsUpdated(uint256 perReleaseCap, uint256 dailyCap)",
	"event GuardianUpdated(address indexed guardian)",
	"event AdminTransferStarted(address indexed pendingAdmin)",
	"event AdminTransferred(address indexed newAdmin)",
	"event RemainderSwept(address indexed to, uint256 amount)",
	"event StalePendingCleared(bytes32 indexed payload, uint256 tokenAmount)",
]);

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

const timelockAbi = [
	{
		type: "function",
		name: "getTimestamp",
		stateMutability: "view",
		inputs: [{ name: "id", type: "bytes32" }],
		outputs: [{ type: "uint256" }],
	},
	{
		type: "event",
		name: "CallScheduled",
		inputs: [
			{ name: "id", type: "bytes32", indexed: true },
			{ name: "index", type: "uint256", indexed: true },
			{ name: "target", type: "address", indexed: false },
			{ name: "value", type: "uint256", indexed: false },
			{ name: "data", type: "bytes", indexed: false },
			{ name: "predecessor", type: "bytes32", indexed: false },
			{ name: "delay", type: "uint256", indexed: false },
		],
	},
] as const;

/** Calls governance can be expected to schedule, named in the page. Anything
 *  else still pages, with its raw selector. */
const KNOWN_FUNCTIONS = Object.fromEntries([
	// MigrationVault
	"acceptAdmin()", "transferAdmin(address)", "setCaps(uint256,uint256)", "setGuardian(address)",
	"setThreshold(uint256)", "addAttestor(address)", "removeAttestor(address)", "pause()", "unpause()",
	"sweepRemainder(address,uint256)", "clearStalePending(uint64,address,uint256)", "setToken(address)",
	// TimelockController and Governor self-administration
	"updateDelay(uint256)", "grantRole(bytes32,address)", "revokeRole(bytes32,address)",
	"setVotingDelay(uint48)", "setVotingPeriod(uint32)", "setProposalThreshold(uint256)",
	"updateQuorumNumerator(uint256)", "updateTimelock(address)", "relay(address,uint256,bytes)",
	// Treasury spends
	"transfer(address,uint256)", "approve(address,uint256)",
].map((signature) => [toFunctionSelector(signature), signature]));

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

function optionalAddress(name: string): `0x${string}` | undefined {
	const value = process.env[name];
	if (!value) return undefined;
	if (!/^0x[0-9a-fA-F]{40}$/.test(value)) throw new Error(`invalid ${name}: ${value}`);
	return value as `0x${string}`;
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
	/** Optional: the governance TimelockController; every operation it
	 *  schedules pages, because the canceller must veto inside the delay. */
	timelockAddress: optionalAddress("TIMELOCK_ADDRESS"),
	/** Age of the confirmed Base block (vs. wall clock) past which the monitor
	 *  pages that its Base view stopped advancing. `safe` normally trails by
	 *  about a minute, `finalized` by about 20 minutes. */
	baseStaleAlertMs: envNumber("BASE_STALE_ALERT_SECONDS", baseFinalityTag === "finalized" ? 1800 : 600) * 1000,
	/** Guardian balance (wei) below which the auto-pause counts as not ready. */
	guardianMinBalanceWei: BigInt(process.env.GUARDIAN_MIN_BALANCE_WEI ?? "1000000000000000"),
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
let lastSourceFailureAlertMs = 0;
let lastBaseStallAlertMs = 0;
let lastReadinessAlertMs = 0;
let lastIssuanceAlertMs = 0;
/** The Base admin scan (vault admin events and the governance queue) runs
 *  ahead of the vault checks in a poll and throws on failure, so the persisted
 *  Base cursor never passes a range it has not scanned; a restart resumes from
 *  that cursor. */
let baseAdminFromBlock: bigint | undefined;
/** Once-only pages by key, each held until the webhook accepts it. */
// ponytail: in memory; a restart re-pages what it finds again (Base admin events after the persisted cursor, pending governance items) and drops undelivered pages it does not find again.
const onceOnlyPages = new Map<string, { subject: string; detail: string; delivery: Promise<boolean> }>();
let lastGovernanceWatchFailureMs = 0;

function log(message: string): void {
	console.log(`${new Date().toISOString()} ${message}`);
}

/** Strip URLs before anything leaves the process: RPC endpoints commonly embed
 *  API keys, and viem error texts quote the endpoint verbatim. */
function redact(text: string): string {
	return text.replace(/https?:\/\/[^\s"')]+/gi, "<url>");
}

/** Sends one page and resolves to whether it was delivered: the webhook must
 *  answer 2xx within the timeout. Without a webhook the log line is the page. */
async function alert(subject: string, detail: string): Promise<boolean> {
	console.error(`${new Date().toISOString()} ALERT: ${subject} — ${detail}`);
	if (!config.alertWebhookUrl) return true;
	try {
		const response = await fetch(config.alertWebhookUrl, {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify({ service: "pen-monitor", subject, detail: redact(detail) }),
			// The webhook is an external dependency; it must never stall the
			// check loop (or the pause that follows a violation) indefinitely.
			signal: AbortSignal.timeout(10_000),
		});
		if (response.ok) return true;
		console.error(`alert webhook answered ${response.status} for: ${subject}`);
	} catch (webhookError) {
		console.error(`alert webhook failed for: ${subject}: ${redact(`${webhookError}`)}`);
	}
	return false;
}

/** Pages `key` once. A page the webhook does not accept stays queued, and
 *  every poll retries it until it is delivered. */
function pageOnce(key: string, subject: string, detail: string): Promise<boolean> {
	const queued = onceOnlyPages.get(key);
	if (queued) return queued.delivery;
	const delivery = alert(subject, detail);
	onceOnlyPages.set(key, { subject, detail, delivery });
	return delivery;
}

async function retryUndeliveredPages(): Promise<void> {
	await Promise.all([...onceOnlyPages.values()].map(async (page) => {
		if (await page.delivery) return;
		page.delivery = alert(page.subject, page.detail);
		await page.delivery;
	}));
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

/** What would make an auto-pause fail now: an RPC on another chain, or a key
 *  that does not parse, is not the vault's guardian or cannot pay for the
 *  pause. Pages, throttled, so it surfaces before a violation needs it. */
async function checkAutoPauseReadiness(): Promise<string[]> {
	const problems: string[] = [];
	const chainId = await publicClient.getChainId();
	if (chainId !== config.baseChainId) {
		problems.push(`the Base RPC serves chain ${chainId}, not BASE_CHAIN_ID ${config.baseChainId}`);
	}
	if (config.guardianPrivateKey) {
		let guardian: `0x${string}` | undefined;
		try {
			guardian = privateKeyToAccount(config.guardianPrivateKey).address;
		} catch {
			problems.push("GUARDIAN_PRIVATE_KEY is not a valid private key");
		}
		if (guardian) {
			const [vaultGuardian, balance] = await Promise.all([
				publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "guardian" }),
				publicClient.getBalance({ address: guardian }),
			]);
			if (vaultGuardian.toLowerCase() !== guardian.toLowerCase()) {
				problems.push(`the key's address ${guardian} is not the vault guardian ${vaultGuardian}`);
			}
			if (balance < config.guardianMinBalanceWei) {
				problems.push(`guardian ${guardian} holds ${balance} wei, less than GUARDIAN_MIN_BALANCE_WEI ${config.guardianMinBalanceWei}`);
			}
		}
	}
	if (problems.length && Date.now() - lastReadinessAlertMs > config.graceSeconds * 1000) {
		lastReadinessAlertMs = Date.now();
		await alert("AUTO-PAUSE NOT READY", `${problems.join("; ")} — fix it before a violation needs the pause`);
	}
	return problems;
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
 * to repair a lagging source node before "expired" forces the pause. The page
 * is not awaited: nothing may sit between detecting a violation and the pause.
 */
function unmatchedDisposition(
	kind: "Approved" | "Released",
	key: string,
	nonce: bigint,
	eventBlockNumber: bigint,
	draft: RuntimeState,
	observedAt: number,
	sourceFinalizedTsMs: number,
): "proven" | "await" | "expired" {
	const alreadyKnown = draft.unmatchedBaseFirstSeenAt.has(key);
	const firstSeen = draft.unmatchedBaseFirstSeenAt.get(key) ?? observedAt;
	draft.unmatchedBaseFirstSeenAt.set(key, firstSeen);
	if (!alreadyKnown) {
		void pageOnce(
			`unverified:${key}`,
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

/** The migration pallet's admin events, each with what to check. */
const PALLET_ADMIN_EVENTS = new Map([
	["MigrationPauseSet", "migrations were paused or resumed; check that it was intended"],
	[
		"TreasuryDestinationSet",
		"treasury migrations are released to this Base address from now on; check that it is the governance timelock",
	],
]);

async function ingestPendulumEvents(api: ApiPromise, draft: RuntimeState, finalizedBlock: number): Promise<void> {
	for (let block = draft.lastPendulumBlock + 1; block <= finalizedBlock; block++) {
		const blockHash = await api.rpc.chain.getBlockHash(block);
		const apiAt = await api.at(blockHash);
		const firstSeenMs = Number((await apiAt.query.timestamp.now()).toString().replaceAll(",", ""));
		const records = (await apiAt.query.system.events()) as unknown as {
			event: { section: string; method: string; data: unknown[] };
		}[];
		for (const [index, record] of records.entries()) {
			const { section, method, data } = record.event;
			if (section !== "tokenMigration") continue;
			const adminCheck = PALLET_ADMIN_EVENTS.get(method);
			if (adminCheck) {
				// Every admin change, however fast the motion behind it passed.
				await pageOnce(
					`pallet:${block}:${index}`,
					"MIGRATION PALLET ADMIN EVENT",
					`tokenMigration.${method}(${data.join(", ")}) in finalized Pendulum block ${block}: ${adminCheck}`,
				);
				continue;
			}
			if (method !== "MigrationInitiated") continue;
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
				await pageOnce(
					`unreleasable:${nonce}`,
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
				const disposition = unmatchedDisposition(
					"Approved",
					key,
					actual.nonce,
					event.blockNumber,
					draft,
					observedAt,
					sourceFinalizedTsMs,
				);
				if (disposition !== "proven") awaitingSource ??= `Approved nonce ${actual.nonce} at Base block ${event.blockNumber}`;
				if (disposition === "await") continue;
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
				const disposition = unmatchedDisposition(
					"Released",
					key,
					actual.nonce,
					event.blockNumber,
					draft,
					observedAt,
					sourceFinalizedTsMs,
				);
				if (disposition !== "proven") awaitingSource ??= `Released nonce ${actual.nonce} at Base block ${event.blockNumber}`;
				if (disposition === "await") continue;
				firstViolation ??= `Released at Base block ${event.blockNumber}: ${mismatch} (` +
					(disposition === "proven"
						? "source view has passed the event's timestamp — fabricated"
						: "source view still behind after the full grace period") + ")";
				continue;
			}
			firstViolation ??= `Released at Base block ${event.blockNumber}: ${mismatch}`;
		}
		// A range holding an event the source may still reveal (awaiting it, or
		// past the grace) is replayed later even after a violation: passing it
		// would lose a legitimate release for good.
		if (awaitingSource) return { violation: firstViolation, awaitingSource };

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

/** Page every vault admin event, and every operation the governance Timelock
 *  scheduled, up to `toBlock`, the poll's `safe`/`finalized` Base block.
 *  Before the governance handover the vault admin acts without a delay, so
 *  its events are the only trace of a guardian, attestor or cap change. */
async function watchBaseAdmin(toBlock: bigint): Promise<void> {
	const timelock = config.timelockAddress;
	const operations = new Map<`0x${string}`, ScheduledCall[]>();
	const pages: Promise<boolean>[] = [];
	for (const [start, end] of blockRanges(baseAdminFromBlock ?? state.baseFromBlock, toBlock, config.baseMaxBlockRange)) {
		await publicClient.getBlock({ blockNumber: end }); // same lagging-node probe as the vault scan
		const adminEvents = await publicClient.getContractEvents({
			address: config.vaultAddress,
			abi: vaultAdminEventsAbi,
			fromBlock: start,
			toBlock: end,
		});
		for (const event of adminEvents) {
			const args = Object.entries(event.args).map(([name, value]) => `${name}=${value}`).join(", ");
			pages.push(pageOnce(
				`vault:${event.transactionHash}:${event.logIndex}`,
				"VAULT ADMIN EVENT",
				`${event.eventName}(${args}) at Base block ${event.blockNumber}, tx ${event.transactionHash}`,
			));
		}
		if (!timelock) continue;
		const scheduled = await publicClient.getContractEvents({
			address: timelock,
			abi: timelockAbi,
			eventName: "CallScheduled",
			fromBlock: start,
			toBlock: end,
		});
		for (const event of scheduled) {
			const { id, index, target, value, data } = event.args as {
				id: `0x${string}`;
				index: bigint;
				target: `0x${string}`;
				value: bigint;
				data: `0x${string}`;
			};
			const calls = operations.get(id) ?? [];
			calls.push({ index, target, value, data });
			operations.set(id, calls);
		}
	}
	if (timelock && operations.size > 0) {
		const token = await publicClient.readContract({
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "token",
			blockNumber: toBlock,
		});
		const labels = {
			[config.vaultAddress.toLowerCase()]: "vault",
			[token.toLowerCase()]: "token",
			[timelock.toLowerCase()]: "timelock",
		};
		for (const [id, calls] of operations) {
			if (onceOnlyPages.has(`timelock:${id}`)) continue;
			const readyAt = await publicClient.readContract({
				address: timelock,
				abi: timelockAbi,
				functionName: "getTimestamp",
				args: [id],
				blockNumber: toBlock,
			});
			pages.push(pageOnce(
				`timelock:${id}`,
				"GOVERNANCE OPERATION SCHEDULED",
				describeTimelockOperation(id, calls, readyAt, Date.now(), labels, KNOWN_FUNCTIONS),
			));
		}
	}
	await Promise.all(pages);
	baseAdminFromBlock = toBlock + 1n;
}

type ApiAt = Awaited<ReturnType<ApiPromise["at"]>>;

/** A democracy `Bounded<Call>`, as much of it as the watcher reads. */
interface BoundedCall {
	isInline: boolean;
	asInline: Uint8Array;
	isLookup: boolean;
	asLookup: { hash_: { toHex(): string }; len: { toNumber(): number } };
	asLegacy: { hash_: { toHex(): string } };
	toString(): string;
}

/** A governance item's call: decoded, `pending` while it cannot be read yet
 *  (re-checked every poll), or neither when it is unreadable for good. */
interface CallRead {
	node?: CallNode;
	text: string;
	pending?: boolean;
}

/**
 * Largest governance call the watcher fetches and decodes. Anyone can note a
 * preimage and propose it, and the decoding cost grows with its size; 64 KiB
 * decodes in about 0.15 s even when packed with the smallest calls.
 * Legitimate calls stay far below it; a bigger one is never fetched and pages
 * as unreadable.
 */
const MAX_GOVERNANCE_CALL_BYTES = 64 * 1024;

function readCall(decode: () => unknown): CallRead {
	try {
		const node = toCallNode(decode() as Parameters<typeof toCallNode>[0]);
		return { node, text: describeCall(node) };
	} catch (decodeError) {
		return { text: `the call cannot be decoded: ${String(decodeError).slice(0, 300)}` };
	}
}

const oversizedCall = (len: number): CallRead => ({
	text: `the call is ${len} bytes, over the ${MAX_GOVERNANCE_CALL_BYTES} bytes the monitor decodes`,
});

/** The length of a noted preimage, which a legacy call reference leaves out. */
async function notedLength(apiAt: ApiAt, hash: { toHex(): string }): Promise<number | undefined> {
	const status = (await apiAt.query.preimage.requestStatusFor(hash)).toJSON() as {
		unrequested?: { len: number };
		requested?: { maybeLen: number | null };
	} | null;
	return status?.unrequested?.len ?? status?.requested?.maybeLen ?? undefined;
}

/** Reads a proposal's call. Its preimage may be noted at any time before
 *  enactment, so until then the call is pending, not unreadable. */
async function boundedCall(apiAt: ApiAt, bounded: BoundedCall): Promise<CallRead> {
	if (bounded.isInline) return readCall(() => apiAt.registry.createType("Call", bounded.asInline));
	const hash = bounded.isLookup ? bounded.asLookup.hash_ : bounded.asLegacy.hash_;
	const len = bounded.isLookup ? bounded.asLookup.len.toNumber() : await notedLength(apiAt, hash);
	if (len !== undefined && len > MAX_GOVERNANCE_CALL_BYTES) return oversizedCall(len);
	const preimage = len === undefined
		? undefined
		: (await apiAt.query.preimage.preimageFor([hash, len])) as unknown as { isSome: boolean; unwrap(): Uint8Array };
	if (!preimage?.isSome) return { text: `preimage ${hash.toHex()} is not noted yet`, pending: true };
	return readCall(() => apiAt.registry.createType("Call", preimage.unwrap()));
}

/** Reads a council or technical-committee motion's call, sized before it is fetched. */
async function motionCall(apiAt: ApiAt, collective: "council" | "technicalCommittee", hash: { toHex(): string }): Promise<CallRead> {
	const proposalOf = apiAt.query[collective].proposalOf;
	const len = Number((await proposalOf.size(hash)).toString());
	if (len > MAX_GOVERNANCE_CALL_BYTES) return oversizedCall(len);
	const call = (await proposalOf(hash)) as unknown as { isSome: boolean; unwrap(): unknown };
	if (!call.isSome) return { text: "the motion's call is not stored", pending: true };
	return readCall(() => call.unwrap());
}

/**
 * Page every pending Pendulum governance item once: public proposals, the
 * council's external proposal, ongoing referenda and open council and
 * technical-committee motions, with the call decoded. Calls that mint, move
 * or unlock balances or change the runtime or the migration pallet page as
 * CRITICAL: minted PEN burned through the migration drains the fixed-supply
 * vault, and only Pendulum governance can stop the call before enactment.
 * So does a call the watcher cannot read; until it can, it is re-checked
 * every poll. Alert-only; it runs after the conservation checks and decodes
 * within fixed bounds, so it can neither stop nor delay them.
 */
async function watchPendulumGovernance(api: ApiPromise): Promise<void> {
	const finalizedHash = await api.rpc.chain.getFinalizedHead();
	const finalizedBlock = (await api.rpc.chain.getHeader(finalizedHash)).number.toNumber();
	const apiAt = await api.at(finalizedHash);
	const democracy = apiAt.query.democracy;
	const launchPeriod = Number(apiAt.consts.democracy.launchPeriod.toString());
	const nextLaunch = (Math.floor(finalizedBlock / launchPeriod) + 1) * launchPeriod;
	const items: Array<{ key: string; label: string; hint: string; read(): Promise<CallRead> }> = [];
	const proposals = (await democracy.publicProps()) as unknown as Array<
		[{ toNumber(): number }, BoundedCall, { toString(): string }]
	>;
	for (const [index, bounded, proposer] of proposals) {
		items.push({
			key: `proposal:${index.toNumber()}`,
			label: `public proposal #${index.toNumber()} from ${proposer.toString()}`,
			hint: `the best-backed public proposal becomes a referendum at block ${nextLaunch}; ` +
				"a hostile one needs a council or technical-committee cancel before then, or nay votes after",
			read: () => boundedCall(apiAt, bounded),
		});
	}
	const external = (await democracy.nextExternal()) as unknown as { isSome: boolean; unwrap(): [BoundedCall, unknown] };
	if (external.isSome) {
		const [bounded] = external.unwrap();
		items.push({
			key: `external:${bounded.toString()}`,
			label: "the council's external proposal",
			hint: `it can become a referendum at block ${nextLaunch}`,
			read: () => boundedCall(apiAt, bounded),
		});
	}
	const [count, lowest] = (await Promise.all([democracy.referendumCount(), democracy.lowestUnbaked()])).map((value) =>
		Number(value.toString())
	);
	for (let index = lowest; index < count; index++) {
		const info = (await democracy.referendumInfoOf(index)) as unknown as {
			isSome: boolean;
			unwrap(): { isOngoing: boolean; asOngoing: { proposal: BoundedCall; end: { toNumber(): number } } };
		};
		if (!info.isSome || !info.unwrap().isOngoing) continue;
		const ongoing = info.unwrap().asOngoing;
		items.push({
			key: `referendum:${index}`,
			label: `referendum #${index}`,
			hint: `voting ends at block ${ongoing.end.toNumber()}; a hostile one needs nay votes or an emergency cancel`,
			read: () => boundedCall(apiAt, ongoing.proposal),
		});
	}
	for (const collective of ["council", "technicalCommittee"] as const) {
		const motions = (await apiAt.query[collective].proposals()) as unknown as Array<{ toHex(): string }>;
		for (const hash of motions) {
			items.push({
				key: `${collective}:${hash.toHex()}`,
				label: `${collective === "council" ? "council" : "technical committee"} motion ${hash.toHex()}`,
				hint: "it executes as soon as enough members vote aye; a hostile one needs nay votes",
				read: () => motionCall(apiAt, collective, hash),
			});
		}
	}
	// Pages go out together, so a burst of new items costs one webhook round trip.
	const pages: Promise<boolean>[] = [];
	for (const item of items) {
		if (onceOnlyPages.has(item.key)) continue;
		let call: CallRead;
		try {
			call = await item.read();
		} catch (readError) {
			// One item must never hide the items after it; a failed read is retried.
			call = { text: `reading the call failed: ${String(readError).slice(0, 300)}`, pending: true };
		}
		if (!call.node) {
			// A call nobody can review is critical. One that cannot be read yet
			// is re-checked every poll and pages again once it can be read.
			pages.push(pageOnce(
				call.pending ? `${item.key}:unread` : item.key,
				"CRITICAL: Pendulum governance item cannot be read",
				`${item.label}: ${call.text}; ` +
					`${call.pending ? "the monitor pages its call once it can read it" : "review it manually"}; ${item.hint}`,
			));
			continue;
		}
		const risky = riskyCalls(call.node);
		pages.push(pageOnce(
			item.key,
			risky.length
				? "CRITICAL: Pendulum governance item mints, moves or unlocks balances, or changes the runtime"
				: "PENDULUM GOVERNANCE ITEM",
			`${item.label}: ${call.text}${risky.length ? ` (flagged: ${risky.join(", ")})` : ""}; ${item.hint}`,
		));
	}
	await Promise.all(pages);
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

interface SourceView {
	finalizedBlock: number;
	timestampMs: number;
	totalMigrated: bigint;
}

/** Reads finalized Pendulum and ingests its new migrations into `draft`.
 *  Throws on any read, decode or continuity failure. */
async function readSource(api: ApiPromise, draft: RuntimeState): Promise<SourceView> {
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
	return { finalizedBlock, timestampMs: sourceFinalizedTsMs, totalMigrated };
}

async function check(api: ApiPromise): Promise<void> {
	const confirmedBase = await publicClient.getBlock({ blockTag: config.baseFinalityTag });
	if (confirmedBase.number === null) throw new Error(`${config.baseFinalityTag} Base block has no number`);
	const baseBlock = confirmedBase.number;
	// A Base view that stops advancing hides every vault event after it while
	// all checks keep passing at the old block; page it like a stalled source.
	const baseAgeMs = Date.now() - Number(confirmedBase.timestamp) * 1000;
	if (
		baseAgeMs > config.baseStaleAlertMs &&
		Date.now() - lastBaseStallAlertMs > config.unmatchedEventGraceSeconds * 1000
	) {
		lastBaseStallAlertMs = Date.now();
		await alert(
			"BASE VIEW STALLED",
			`${config.baseFinalityTag} block ${baseBlock} is ${Math.round(baseAgeMs / 1000)}s old; vault events after it ` +
				`are not being checked — restore the Base node, or check whether Base's ${config.baseFinalityTag} head stalled`,
		);
	}
	await checkAutoPauseReadiness();
	await watchBaseAdmin(baseBlock);

	// A Pendulum failure must not switch off the Base-side checks: the vault
	// balance check and its pause run without the source. Reconciliation, M2a
	// and liveness need the source and wait for it.
	let draft = cloneState(state);
	let source: SourceView | undefined;
	try {
		source = await readSource(api, draft);
	} catch (sourceError) {
		draft = cloneState(state); // drops a half-ingested block; the 200-block checkpoints already moved `state`
		if (Date.now() - lastSourceFailureAlertMs > config.unmatchedEventGraceSeconds * 1000) {
			lastSourceFailureAlertMs = Date.now();
			await alert(
				"PENDULUM SOURCE UNREADABLE",
				`${sourceError}; the vault balance check continues, but Base events are not reconciled until the source reads again`,
			);
		}
	}

	const [totalReleased, totalSwept, conversionFactor, tokenAddress] = await Promise.all([
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "totalReleased", blockNumber: baseBlock }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "totalSwept", blockNumber: baseBlock }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "conversionFactor", blockNumber: baseBlock }),
		publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName: "token", blockNumber: baseBlock }),
	]);

	const reconciliation = source
		? await reconcileBaseEvents(draft, baseBlock, conversionFactor, source.timestampMs)
		: { violation: undefined, awaitingSource: undefined };
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
	if (!source) return;
	const { finalizedBlock, totalMigrated } = source;
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
	// One page per kind and poll however long the backlog, with the detail
	// per nonce in the log: a page per nonce floods the receiver and, awaited
	// in turn, delays the next poll's checks.
	const quorum: MigrationRecord[] = [];
	const belowQuorum: MigrationRecord[] = [];
	for (let index = 0; index < pending.length; index++) {
		const migration = pending[index];
		if (!isStale(migration.firstSeenMs, now, config.graceSeconds)) continue;
		const lastAlerted = draft.livenessAlertedAt.get(migration.nonce);
		if (lastAlerted !== undefined && now - lastAlerted < config.graceSeconds * 1000) continue;
		log(`liveness: nonce ${migration.nonce} has ${approvals[index]}/${threshold} active approvals and is unreleased`);
		(approvals[index] >= threshold ? quorum : belowQuorum).push(migration);
	}
	const pageLiveness = async (migrations: MigrationRecord[], subject: string, detail: string) => {
		if (!migrations.length) return;
		const nonces = migrations.slice(0, 10).map((migration) => migration.nonce).join(", ") +
			(migrations.length > 10 ? ` and ${migrations.length - 10} more` : "");
		if (await alert(subject, `${migrations.length} migration(s) (nonce ${nonces}) ${detail}`)) {
			for (const migration of migrations) draft.livenessAlertedAt.set(migration.nonce, now);
		}
	};
	// Quorum is present but nothing released it: cap-deferred and starved of
	// daily allowance, paused, or made releasable by a threshold cut without
	// ever emitting ReleasePending (so the releaser cannot see it).
	await pageLiveness(
		quorum,
		"LIVENESS: quorum reached but not released",
		`have ${threshold} or more active approvals and are still unreleased after ${config.graceSeconds}s — check the ` +
			`daily allowance / pause state, or call release() directly if one qualified through a threshold cut (RB-6)`,
	);
	await pageLiveness(
		belowQuorum,
		"LIVENESS: migration below approval quorum",
		`have fewer than ${threshold} active approvals after ${config.graceSeconds}s`,
	);

	state = draft;
	persistState(state);
	log(
		`ok: migrated=${totalMigrated} released=${totalReleased} swept=${totalSwept} ` +
			`pending=${state.migrations.size} vaultBalance=${vaultBalance} base=${baseBlock} pendulum=${finalizedBlock}`,
	);
}

/** Startup once the Pendulum API is connected: durable state, the multicall
 *  probe, the state identity check and the auto-pause readiness. */
export async function start(api: ApiPromise): Promise<void> {
	state = hydrateState();
	const multicallCode = await publicClient.getBytecode({ address: MULTICALL3_ADDRESS });
	if (!multicallCode || multicallCode === "0x") {
		multicallUnavailable = true;
		await alert("multicall unavailable, using bounded individual reads", `no contract code at ${MULTICALL3_ADDRESS}`);
	}
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
	if (!config.guardianPrivateKey) {
		await alert(
			"AUTO-PAUSE DISABLED",
			"no GUARDIAN_PRIVATE_KEY configured: violations page, but nothing pauses the vault automatically",
		);
	}
	const readiness = await checkAutoPauseReadiness();
	log(
		`monitor started; Pendulum after ${state.lastPendulumBlock}, Base from ${state.baseFromBlock}, ` +
			`polling every ${config.pollIntervalMs}ms; governance queue ` +
			(config.timelockAddress ? `watched at ${config.timelockAddress}` : "not watched (TIMELOCK_ADDRESS unset)") +
			`; auto-pause ${!config.guardianPrivateKey ? "disabled" : readiness.length ? "NOT READY" : "ready"}`,
	);
}

/** One poll; a failure pages and the next poll starts afresh. */
export async function poll(api: ApiPromise): Promise<void> {
	try {
		await check(api);
	} catch (checkError) {
		await alert("monitor check failed", `${checkError}`);
	}
	try {
		await watchPendulumGovernance(api);
	} catch (governanceError) {
		if (Date.now() - lastGovernanceWatchFailureMs > config.graceSeconds * 1000) {
			lastGovernanceWatchFailureMs = Date.now();
			await alert("pendulum governance watch failed", `${governanceError}`);
		}
	}
	await retryUndeliveredPages();
}

async function main(): Promise<void> {
	const api = await ApiPromise.create({ provider: new WsProvider(config.pendulumWs) });
	await start(api);
	for (;;) {
		await poll(api);
		await new Promise((resolve) => setTimeout(resolve, config.pollIntervalMs));
	}
}

// main.test.ts sets this before importing the module, to drive single polls.
if (!(globalThis as { penMonitorUnderTest?: boolean }).penMonitorUnderTest) {
	main().catch(async (error) => {
		await alert("monitor startup failed", `${error}`);
		process.exit(1);
	});
}
