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
	disagreeingAttestors,
	isStale,
	provablyUnsourced,
	releaseMismatch,
	releasedExceedsMigrated,
	riskyCalls,
	type ScheduledCall,
	type Severity,
	shouldAwaitSource,
	slackText,
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
	{
		type: "event",
		name: "RemainderSwept",
		inputs: [
			{ name: "to", type: "address", indexed: true },
			{ name: "amount", type: "uint256", indexed: false },
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

/** A non-negative integer amount from the environment, validated at startup. */
function envAmount(name: string, fallback = "0"): bigint {
	const raw = process.env[name] || fallback;
	if (!/^\d+$/.test(raw)) throw new Error(`invalid ${name}: ${raw}`);
	return BigInt(raw);
}

function optionalAddress(name: string): `0x${string}` | undefined {
	const value = process.env[name];
	if (!value) return undefined;
	if (!/^0x[0-9a-fA-F]{40}$/.test(value)) throw new Error(`invalid ${name}: ${value}`);
	return value as `0x${string}`;
}

/** Comma-separated WebSocket URLs from the environment, at most `most`,
 *  validated at startup. */
function wsUrls(name: string, most = Infinity): string[] {
	const urls = (process.env[name] ?? "").split(",").map((url) => url.trim()).filter(Boolean);
	if (urls.length > most || urls.some((url) => !/^wss?:\/\//i.test(url))) {
		throw new Error(`invalid ${name}: expected ${most === 1 ? "one ws:// or wss:// URL" : "ws:// or wss:// URLs"}`);
	}
	return urls;
}

function finalityTag(): "safe" | "finalized" {
	const value = process.env.BASE_FINALITY_TAG ?? "safe";
	if (value !== "safe" && value !== "finalized") throw new Error(`invalid BASE_FINALITY_TAG ${value}`);
	return value;
}

const baseFinalityTag = finalityTag();

const config = {
	pendulumWs: required("PENDULUM_WS"),
	/** Optional: a second Pendulum node whose block hashes must agree. */
	pendulumWsCrossCheck: wsUrls("PENDULUM_WS_CROSSCHECK", 1)[0] as string | undefined,
	/** Optional: third-party Polkadot RPCs, tried in order, whose record of
	 *  the Pendulum head anchors the source. */
	polkadotWs: wsUrls("POLKADOT_WS"),
	pendulumParaId: envNumber("PENDULUM_PARA_ID", 2094),
	/** How long the source may go without a relay-anchored head before that pages critical. */
	relayAnchorGraceMs: envNumber("RELAY_ANCHOR_GRACE_SECONDS", 3600) * 1000,
	baseRpcUrl: required("BASE_RPC_URL"),
	vaultAddress: required("VAULT_ADDRESS") as `0x${string}`,
	pollIntervalMs: envNumber("POLL_INTERVAL_MS", 60_000),
	graceSeconds: envNumber("GRACE_SECONDS", 1800),
	unmatchedEventGraceSeconds: envNumber("UNMATCHED_EVENT_GRACE_SECONDS", 600),
	alertWebhookUrl: process.env.ALERT_WEBHOOK_URL,
	/** Optional: requested after every poll that ran all checks. */
	heartbeatUrl: process.env.HEARTBEAT_URL,
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
	/** Optional: the only Base address a treasury migration may pay. */
	expectedTreasuryDestination: optionalAddress("EXPECTED_TREASURY_DESTINATION"),
	/** Age of the confirmed Base block (vs. wall clock) past which the monitor
	 *  pages that its Base view stopped advancing. `safe` normally trails by
	 *  about a minute, `finalized` by about 20 minutes. */
	baseStaleAlertMs: envNumber("BASE_STALE_ALERT_SECONDS", baseFinalityTag === "finalized" ? 1800 : 600) * 1000,
	/** Guardian balance (wei) below which the auto-pause counts as not ready. */
	guardianMinBalanceWei: envAmount("GUARDIAN_MIN_BALANCE_WEI", "1000000000000000"),
	/** Excess of released over migrated, in token units, that operators have
	 *  quantified and accepted after a real loss (RB-3); M2a fires beyond it. */
	acknowledgedReleaseExcess: envAmount("ACKNOWLEDGED_RELEASE_EXCESS"),
};

const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";
/** The runtime's treasury account, `PalletId(*b"py/trsry")` as an account
 *  ("modl" + id, zero-padded): the `who` of every treasury migration. */
const PENDULUM_TREASURY_ACCOUNT = `0x6d6f646c70792f7472737279${"00".repeat(20)}`;

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
	/** The hash of `lastPendulumBlock`: the next blocks must descend from it. */
	lastPendulumHash: string | undefined;
	/** The newest Pendulum head the relay chain recorded that the source was
	 *  verified against; the source's chain must keep passing through it. */
	relayAnchor: { number: number; hash: string } | undefined;
	nextExpectedNonce: bigint;
	baseFromBlock: bigint;
	/** The admin scan's own cursor: it only pages, so nothing waits for it. */
	baseAdminFromBlock: bigint;
	migrations: Map<bigint, MigrationRecord>;
	livenessAlertedAt: Map<bigint, number>;
	unmatchedBaseFirstSeenAt: Map<string, number>;
	/** Approvals that disagreed with the finalized burn, or found none, kept
	 *  until their nonce is released (see `disagreeingAttestors`). */
	mismatchedApprovals: Array<MigrationTuple & { attestor: string }>;
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
			lastPendulumHash: undefined,
			relayAnchor: undefined,
			nextExpectedNonce: config.pendulumStartNonce,
			baseFromBlock: config.baseStartBlock,
			baseAdminFromBlock: config.baseStartBlock,
			migrations: new Map(),
			livenessAlertedAt: new Map(),
			unmatchedBaseFirstSeenAt: new Map(),
			mismatchedApprovals: [],
			issuanceAnchor: undefined,
		};
	}
	return {
		baseChainId: persisted.baseChainId,
		vaultAddress: persisted.vaultAddress,
		pendulumGenesisHash: persisted.pendulumGenesisHash,
		lastPendulumBlock: persisted.lastPendulumBlock,
		lastPendulumHash: persisted.lastPendulumHash,
		relayAnchor: persisted.relayAnchor,
		nextExpectedNonce: BigInt(persisted.nextExpectedNonce),
		baseFromBlock: BigInt(persisted.baseFromBlock),
		baseAdminFromBlock: BigInt(persisted.baseAdminFromBlock ?? persisted.baseFromBlock),
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
		mismatchedApprovals: (persisted.mismatchedApprovals ?? []).map((approval) => ({
			...approval,
			nonce: BigInt(approval.nonce),
			palletAmount: BigInt(approval.palletAmount),
		})),
		issuanceAnchor: persisted.issuanceAnchor === undefined ? undefined : BigInt(persisted.issuanceAnchor),
	};
}

function cloneState(source: RuntimeState): RuntimeState {
	return {
		baseChainId: source.baseChainId,
		vaultAddress: source.vaultAddress,
		pendulumGenesisHash: source.pendulumGenesisHash,
		lastPendulumBlock: source.lastPendulumBlock,
		lastPendulumHash: source.lastPendulumHash,
		relayAnchor: source.relayAnchor,
		nextExpectedNonce: source.nextExpectedNonce,
		baseFromBlock: source.baseFromBlock,
		baseAdminFromBlock: source.baseAdminFromBlock,
		migrations: new Map(source.migrations),
		livenessAlertedAt: new Map(source.livenessAlertedAt),
		unmatchedBaseFirstSeenAt: new Map(source.unmatchedBaseFirstSeenAt),
		mismatchedApprovals: [...source.mismatchedApprovals],
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
		lastPendulumHash: state.lastPendulumHash as `0x${string}` | undefined,
		relayAnchor: state.relayAnchor as { number: number; hash: `0x${string}` } | undefined,
		nextExpectedNonce: state.nextExpectedNonce.toString(),
		baseFromBlock: state.baseFromBlock.toString(),
		baseAdminFromBlock: state.baseAdminFromBlock.toString(),
		migrations: [...state.migrations.values()].map((migration) => ({
			nonce: migration.nonce.toString(),
			recipient: migration.recipient as `0x${string}`,
			palletAmount: migration.palletAmount.toString(),
			firstSeenMs: migration.firstSeenMs,
			sourceBlock: migration.sourceBlock,
		})),
		livenessAlertedAt: [...state.livenessAlertedAt].map(([nonce, at]) => [nonce.toString(), at]),
		unmatchedBaseFirstSeenAt: [...state.unmatchedBaseFirstSeenAt],
		mismatchedApprovals: state.mismatchedApprovals.map((approval) => ({
			nonce: approval.nonce.toString(),
			recipient: approval.recipient as `0x${string}`,
			palletAmount: approval.palletAmount.toString(),
			attestor: approval.attestor as `0x${string}`,
		})),
		issuanceAnchor: state.issuanceAnchor?.toString(),
	};
	saveMonitorState(config.stateFile, persisted);
}

let state: RuntimeState;
let multicallUnavailable = false;
let lastSourceStallAlertMs = 0;
let lastSourceFailureAlertMs = 0;
let lastSourceIntegrityAlertMs = 0;
let lastCrossCheckAlertMs = 0;
let lastRelayAlertMs = 0;
let lastRelayDisagreementAlertMs = 0;
let lastRelayStaleAlertMs = 0;
/** When the source last verified against a new relay-chain record (or the start). */
let lastRelayAnchorMs = Date.now();
let lastBaseStallAlertMs = 0;
let lastReadinessAlertMs = 0;
let lastAdminScanFailureAlertMs = 0;
/** Since when the Base admin scan fails, while it does. */
let adminScanFailingSinceMs: number | undefined;
let lastLogScanAlertMs = 0;
/** Since when the Base log scan holds a range, while it does. */
let logScanHeldSinceMs: number | undefined;
let lastIssuanceAlertMs = 0;
/** Once-only pages by key, each held until the webhook accepts it. */
// ponytail: in memory; a restart re-pages what it finds again (Base admin events after the persisted cursor, pending governance items) and drops undelivered pages it does not find again.
const onceOnlyPages = new Map<string, { subject: string; detail: string; delivery: Promise<boolean> }>();
let lastGovernanceWatchFailureMs = 0;

function log(message: string): void {
	console.log(`${new Date().toISOString()} ${message}`);
}

/** Strip URLs before anything leaves the process: RPC endpoints commonly embed
 *  API keys, and viem and polkadot.js error texts quote the endpoint verbatim. */
function redact(text: string): string {
	return text.replace(/(?:https?|wss?):\/\/[^\s"')]+/gi, "<url>");
}

/** Sends one page, as a Slack incoming-webhook message, and resolves to
 *  whether it was delivered: the webhook must answer 2xx within the timeout.
 *  Without a webhook the log line is the page. */
async function alert(
	subject: string,
	detail: string,
	severity: Severity = subject.startsWith("CRITICAL") ? "critical" : "warning",
): Promise<boolean> {
	console.error(`${new Date().toISOString()} ALERT: ${subject} — ${detail}`);
	if (!config.alertWebhookUrl) return true;
	try {
		const response = await fetch(config.alertWebhookUrl, {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify({ text: slackText("pen-monitor", severity, subject, redact(detail)) }),
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

/** The pause attempt in flight, which a concurrent violation joins. */
let pausing: Promise<boolean> | undefined;

function pauseVault(): Promise<boolean> {
	pausing ??= pauseOnce().finally(() => (pausing = undefined));
	return pausing;
}

async function pauseOnce(): Promise<boolean> {
	if (!config.guardianPrivateKey) {
		await alert("AUTO-PAUSE UNAVAILABLE", "no GUARDIAN_PRIVATE_KEY configured; pause manually NOW", "critical");
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
		await alert("AUTO-PAUSE FAILED", `${pauseError}; pause manually NOW`, "critical");
		return false;
	}
}

/** What would make an auto-pause fail now: an RPC on another chain, or a key
 *  that does not parse, is not the vault's guardian or cannot pay for the
 *  pause. Pages, throttled, so it surfaces before a violation needs it. A read
 *  that fails is a problem too: it must not stop the poll's other checks. */
async function checkAutoPauseReadiness(): Promise<string[]> {
	const problems: string[] = [];
	try {
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
	} catch (readError) {
		problems.push(`the readiness reads failed: ${readError}`);
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
	const notified = alert(subject, detail, "critical");
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

interface BlockRef {
	number: number;
	hash: string;
}

/** The Pendulum source contradicts the chain it must be on: the one ingested
 *  so far, the second node's or the relay chain's. Handled as unreadable. */
class SourceIntegrityError extends Error {}

/** The source holds at a block whose violation must pause the vault first;
 *  the violation's own page says so. */
class PauseHold extends Error {}

/** Bound on connecting to, and on each read from, the nodes of the integrity checks. */
const NODE_TIMEOUT_MS = 20_000;

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
	let timer: NodeJS.Timeout | undefined;
	const timeout = new Promise<never>((_, reject) => {
		timer = setTimeout(() => reject(new Error(`no answer within ${ms / 1000}s`)), ms);
	});
	return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

/** Connects to a node of the integrity checks; tests replace it. */
let connectNode = async (url: string): Promise<ApiPromise> => {
	const provider = new WsProvider(url, 2_500, {}, NODE_TIMEOUT_MS);
	try {
		return await withTimeout(ApiPromise.create({ provider, throwOnConnect: true, noInitWarn: true }), NODE_TIMEOUT_MS);
	} catch (connectError) {
		await provider.disconnect().catch(() => {});
		throw connectError;
	}
};
const nodeConnections = new Map<string, Promise<ApiPromise>>();

/** Runs `read` against the node at `url` over a kept connection, bounded in
 *  time; a failure drops the connection, so the next poll connects afresh. */
async function onNode<T>(url: string, read: (node: ApiPromise) => Promise<T>): Promise<T> {
	let connection = nodeConnections.get(url);
	if (!connection) nodeConnections.set(url, (connection = connectNode(url)));
	try {
		return await withTimeout(connection.then(read), NODE_TIMEOUT_MS);
	} catch (nodeError) {
		nodeConnections.delete(url);
		connection.then((node) => node.disconnect()).catch(() => {});
		// A failed WebSocket connection rejects with an event, not an Error.
		throw nodeError instanceof Error ? nodeError : new Error(String((nodeError as { message?: unknown })?.message ?? nodeError));
	}
}

interface MigrationTotals {
	nextNonce: bigint;
	totalMigrated: bigint;
}

async function migrationTotals(apiAt: ApiAt): Promise<MigrationTotals> {
	const [nextNonce, totalMigrated] = await Promise.all([
		apiAt.query.tokenMigration.nextNonce(),
		apiAt.query.tokenMigration.totalMigrated(),
	]);
	return {
		nextNonce: BigInt(nextNonce.toString().replaceAll(",", "")),
		totalMigrated: BigInt(totalMigrated.toString().replaceAll(",", "")),
	};
}

/** The height up to which the second node confirms the source's chain
 *  `hashes`, walked down from `top` to `bottom`: the hashes must agree at its
 *  finalized head, or at `top` once it has finalized past it. Undefined while
 *  it cannot be read or its finalized head is older than
 *  SOURCE_STALE_ALERT_SECONDS, which only warns, throttled: it is a check, not
 *  a source. */
async function crossCheckSource(hashes: Map<number, string>, top: BlockRef, bottom: number): Promise<number | undefined> {
	const url = config.pendulumWsCrossCheck;
	if (!url) return undefined;
	let other: BlockRef & { ageMs: number };
	try {
		other = await onNode(url, async (node) => {
			const finalizedHash = await node.rpc.chain.getFinalizedHead();
			const finalized = (await node.rpc.chain.getHeader(finalizedHash)).number.toNumber();
			const timestamp = await (await node.at(finalizedHash)).query.timestamp.now();
			// Above its finalized head, a node's block at a height may still lose.
			const number = Math.min(finalized, top.number);
			const hash = number === finalized ? finalizedHash.toHex() : (await node.rpc.chain.getBlockHash(number)).toHex();
			return { number, hash, ageMs: Date.now() - Number(timestamp.toString().replaceAll(",", "")) };
		});
	} catch (crossCheckError) {
		await crossCheckUnavailable(`cannot be read: ${crossCheckError}`);
		return undefined;
	}
	if (other.ageMs > config.sourceStaleAlertMs) {
		await crossCheckUnavailable(`has a finalized head ${Math.round(other.ageMs / 1000)}s old`);
		return undefined;
	}
	if (other.number >= bottom && hashes.get(other.number) !== other.hash) {
		throw new SourceIntegrityError(
			`the second Pendulum node has ${other.hash} at block ${other.number}, the source ${hashes.get(other.number)}`,
		);
	}
	return other.number;
}

/** The second node must have the source's migration totals at `blockHash`, a
 *  block it confirmed. One that cannot answer only warns, throttled. */
async function crossCheckTotals(block: number, blockHash: string, totals: MigrationTotals): Promise<void> {
	let other: MigrationTotals;
	try {
		other = await onNode(config.pendulumWsCrossCheck as string, async (node) => migrationTotals(await node.at(blockHash)));
	} catch (crossCheckError) {
		return crossCheckUnavailable(`cannot be read: ${crossCheckError}`);
	}
	if (other.nextNonce !== totals.nextNonce || other.totalMigrated !== totals.totalMigrated) {
		throw new SourceIntegrityError(
			`at Pendulum block ${block} the second node has nextNonce ${other.nextNonce} and ` +
				`totalMigrated ${other.totalMigrated}, the source ${totals.nextNonce} and ${totals.totalMigrated}`,
		);
	}
}

type EventRecords = Array<{ event: { section: string; method: string; data: unknown[] } }>;

/** The migration pallet's events in a block, as one text; empty without any. */
function migrationEventText(records: EventRecords): string {
	return records
		.filter(({ event }) => event.section === "tokenMigration")
		.map(({ event }) => `${event.method}(${event.data.join(", ")})`)
		.join("; ");
}

/** The second node must report the same migration events in the block at
 *  `blockHash`. One that cannot answer only warns, throttled. */
async function crossCheckEvents(block: number, blockHash: string, events: string): Promise<void> {
	const url = config.pendulumWsCrossCheck;
	if (!url) return;
	let other: string;
	try {
		other = await onNode(url, async (node) =>
			migrationEventText((await (await node.at(blockHash)).query.system.events()) as unknown as EventRecords)
		);
	} catch (crossCheckError) {
		return crossCheckUnavailable(`cannot be read: ${crossCheckError}`);
	}
	if (other !== events) {
		throw new SourceIntegrityError(
			`in Pendulum block ${block} the second node reports the migration events ${other || "none"}, the source ${events}`,
		);
	}
}

/** Pages, throttled, that the source cannot be compared with the second node. */
async function crossCheckUnavailable(reason: string): Promise<void> {
	if (Date.now() - lastCrossCheckAlertMs <= config.graceSeconds * 1000) return;
	lastCrossCheckAlertMs = Date.now();
	await alert(
		"PENDULUM CROSS-CHECK UNAVAILABLE",
		`the second Pendulum node (PENDULUM_WS_CROSSCHECK) ${reason}; the source's blocks are not compared ` +
			"with it until it answers — restore it",
	);
}

/** Walks parent hashes down from `top` to `bottom` through headers fetched
 *  from the node, recomputing each hash here, so that the result is one
 *  chain the node cannot fake: block hashes by height, `bottom` included. */
async function walkHeaders(api: ApiPromise, top: BlockRef, bottom: number): Promise<Map<number, string>> {
	// ponytail: one pass, held in memory and started over after a failed read; make it resumable if catch-ups grow to weeks.
	const hashes = new Map([[top.number, top.hash]]);
	let hash = top.hash;
	for (let height = top.number; height > bottom; height--) {
		const header = await api.rpc.chain.getHeader(hash);
		if (header.hash.toHex() !== hash || header.number.toNumber() !== height) {
			throw new SourceIntegrityError(`the source's header for Pendulum block ${height} does not hash to ${hash}`);
		}
		hash = header.parentHash.toHex();
		hashes.set(height - 1, hash);
	}
	return hashes;
}

/** The Pendulum head that the relay chain behind the RPC at `url` recorded
 *  at its finalized block. */
function relayHead(api: ApiPromise, url: string): Promise<BlockRef> {
	return onNode(url, async (relay) => {
		const relayFinalized = await relay.rpc.chain.getFinalizedHead();
		const head = (await (await relay.at(relayFinalized)).query.paras.heads(config.pendulumParaId)) as unknown as {
			isSome: boolean;
			unwrap(): { toU8a(bare: boolean): Uint8Array };
		};
		if (!head.isSome) throw new Error(`no head recorded for para ${config.pendulumParaId}`);
		// The head data is the Pendulum header; its hash is computed here.
		const header = api.createType("Header", head.unwrap().toU8a(true)) as unknown as {
			number: { toNumber(): number };
			hash: { toHex(): string };
		};
		return { number: header.number.toNumber(), hash: header.hash.toHex() };
	});
}

/** How far below the block ingested last the relay chain's record may lie and
 *  still be walked down to; a deeper one is not used. */
const RELAY_HEAD_MAX_DEPTH = 100;

/** Why the relay chain's record `head` does not anchor the source this poll,
 *  or undefined when it does. Throws when it contradicts the source's chain:
 *  `hashes`, walked down from `top` to `bottom`. */
async function relayVerdict(
	api: ApiPromise,
	draft: RuntimeState,
	hashes: Map<number, string>,
	top: BlockRef,
	bottom: BlockRef,
	head: BlockRef,
): Promise<string | undefined> {
	const known = draft.relayAnchor;
	if (known && head.number <= known.number) {
		if (head.number === known.number && head.hash !== known.hash) {
			throw new SourceIntegrityError(
				`the relay chain now records ${head.hash} as Pendulum block ${head.number}, where it recorded ${known.hash}`,
			);
		}
		return `the relay chain's Pendulum head ${head.number} is not newer`;
	}
	if (head.number > top.number) {
		// Past the finalized head, the source's best chain may still hold another
		// block at that height. The record anchors the source once it holds the
		// record, whose chain must then pass through `top`.
		if ((await api.rpc.chain.getBlockHash(head.number)).toHex() !== head.hash) {
			return `the source has not imported block ${head.number}, the relay chain's Pendulum head, yet`;
		}
		const hash = (await walkHeaders(api, head, top.number)).get(top.number);
		if (hash !== top.hash) {
			throw new SourceIntegrityError(
				`the relay chain records ${head.hash} as Pendulum block ${head.number}, whose chain has ${hash} at block ` +
					`${top.number}, where the source has ${top.hash}`,
			);
		}
		return undefined;
	}
	if (head.number < bottom.number - RELAY_HEAD_MAX_DEPTH) {
		return `the relay chain's Pendulum head ${head.number} lies more than ${RELAY_HEAD_MAX_DEPTH} blocks below block ${bottom.number}`;
	}
	const hash = head.number >= bottom.number
		? hashes.get(head.number)
		: (await walkHeaders(api, bottom, head.number)).get(head.number);
	if (hash !== head.hash) {
		throw new SourceIntegrityError(`the relay chain records ${head.hash} as Pendulum block ${head.number}, the source's chain has ${hash}`);
	}
	return undefined;
}

/** Anchors the source to the relay chain's record of the Pendulum head, kept
 *  as `draft.relayAnchor`, from the first relay RPC in POLKADOT_WS whose
 *  record anchors it; while a record does not, the next RPC is asked. A
 *  record that contradicts the source counts only when another relay RPC's
 *  record contradicts it too, or when POLKADOT_WS lists one RPC. A record that
 *  anchors the source at or above the contradicting record's height outvotes
 *  it; any other answer leaves it unconfirmed. Resolves to the height up to
 *  which the relay chain confirms the source's chain `hashes`, walked down
 *  from `top` to `bottom`: the newest anchored head, or no higher than
 *  `bottom` before the first; undefined while no relay RPC answers. Pages
 *  while no new head is anchored and the source's blocks wait for one, a
 *  contradiction stays unconfirmed or no relay RPC answers: a warning,
 *  critical once none was anchored for RELAY_ANCHOR_GRACE_SECONDS; and when
 *  relay RPCs disagree. All throttled. */
async function anchorToRelay(
	api: ApiPromise,
	draft: RuntimeState,
	hashes: Map<number, string>,
	top: BlockRef,
	bottom: BlockRef,
): Promise<number | undefined> {
	if (!config.polkadotWs.length) return undefined;
	const answers: string[] = [];
	let answered = false;
	let contradiction: { rpc: number; height: number; error: SourceIntegrityError } | undefined;
	for (const [index, url] of config.polkadotWs.entries()) {
		let head: BlockRef;
		try {
			head = await relayHead(api, url);
		} catch (relayError) {
			answers.push(`relay RPC ${index + 1}: ${relayError}`);
			continue;
		}
		answered = true;
		let verdict: string | undefined;
		try {
			verdict = await relayVerdict(api, draft, hashes, top, bottom, head);
		} catch (verdictError) {
			if (!(verdictError instanceof SourceIntegrityError)) throw verdictError;
			if (contradiction || config.polkadotWs.length === 1) throw contradiction?.error ?? verdictError;
			contradiction = { rpc: index + 1, height: head.number, error: verdictError };
			continue;
		}
		if (verdict === undefined && contradiction && head.number < contradiction.height) {
			verdict = `its Pendulum head ${head.number} lies below the contradicting record`;
		}
		if (verdict !== undefined) {
			answers.push(`relay RPC ${index + 1}: ${verdict}`);
			continue;
		}
		draft.relayAnchor = head;
		lastRelayAnchorMs = Date.now();
		if (contradiction && Date.now() - lastRelayDisagreementAlertMs > config.graceSeconds * 1000) {
			lastRelayDisagreementAlertMs = Date.now();
			await alert(
				"RELAY RPCS DISAGREE",
				`relay RPC ${contradiction.rpc} reports that ${contradiction.error.message}; relay RPC ${index + 1} records ` +
					`${head.hash} as Pendulum block ${head.number}, on the source's chain, and the source is anchored to ` +
					"that record — one of the two relay RPCs is wrong, check both",
			);
		}
		return head.number;
	}
	const reason = contradiction
		? `relay RPC ${contradiction.rpc} reports that ${contradiction.error.message}, and no other relay RPC confirms ` +
			`or outvotes it${answers.length ? ` (${answers.join("; ")})` : ""}`
		: `${answered ? "" : "the relay chain cannot be read: "}${answers.join("; ")}`;
	const confirmed = draft.relayAnchor?.number ?? bottom.number;
	const consequence = answered
		? `no Pendulum block after ${Math.max(confirmed, draft.lastPendulumBlock)} is ingested until a relay RPC anchors a newer head`
		: "the Pendulum source is used without the relay-chain anchor until a relay RPC answers";
	if (
		(contradiction || !answered || top.number > confirmed) &&
		Date.now() - lastRelayAlertMs > config.graceSeconds * 1000
	) {
		lastRelayAlertMs = Date.now();
		await alert(
			"RELAY ANCHOR UNAVAILABLE",
			`${reason}; ${consequence}, and this turns critical after ${config.relayAnchorGraceMs / 1000}s`,
		);
	}
	const unanchoredMs = Date.now() - lastRelayAnchorMs;
	if (unanchoredMs > config.relayAnchorGraceMs && Date.now() - lastRelayStaleAlertMs > config.unmatchedEventGraceSeconds * 1000) {
		lastRelayStaleAlertMs = Date.now();
		await alert(
			"CRITICAL: RELAY ANCHOR STALE",
			`no Pendulum head was checked against the relay chain for ${Math.round(unanchoredMs / 1000)}s (${reason}); ` +
				`${consequence} — restore a relay RPC, or the source node if it lags`,
		);
	}
	return answered ? confirmed : undefined;
}

/** The hashes of the finalized blocks to ingest, on one chain with every
 *  block ingested before and with the relay-chain record kept from an earlier
 *  poll, and the newest of them that every checker that answers confirms
 *  (`confirmed`, at most the source's finalized head): the second node up to
 *  `crossChecked`, and the relay chain, whose newest record anchors the
 *  source. A checker that does not answer confirms nothing and holds nothing
 *  back. */
async function sourceHashes(
	api: ApiPromise,
	draft: RuntimeState,
	finalized: BlockRef,
): Promise<{ hashes: Map<number, string>; confirmed: number; crossChecked: number | undefined }> {
	// A kept record past the finalized head binds the source too: the walk starts there.
	const known = draft.relayAnchor;
	const top = known && known.number > finalized.number ? known : finalized;
	const hashes = await walkHeaders(api, top, draft.lastPendulumBlock);
	const ingested = draft.lastPendulumHash ? { number: draft.lastPendulumBlock, hash: draft.lastPendulumHash } : undefined;
	const expected: Array<[BlockRef | undefined, string]> = [
		[finalized, "the source's finalized head is"],
		[ingested, "the monitor ingested"],
		[known, "the relay chain recorded"],
	];
	for (const [block, what] of expected) {
		const hash = block && hashes.get(block.number);
		if (block && hash !== undefined && hash !== block.hash) {
			throw new SourceIntegrityError(`the source's chain has ${hash} at Pendulum block ${block.number}, where ${what} ${block.hash}`);
		}
	}
	const lowest = Math.min(top.number, draft.lastPendulumBlock);
	const crossChecked = await crossCheckSource(hashes, top, lowest);
	const anchored = await anchorToRelay(api, draft, hashes, top, { number: lowest, hash: hashes.get(lowest) as string });
	return { hashes, confirmed: Math.min(finalized.number, crossChecked ?? Infinity, anchored ?? Infinity), crossChecked };
}

/** The migration pallet's admin events, each with what to check. */
const PALLET_ADMIN_EVENTS = new Map([
	["MigrationPauseSet", "migrations were paused or resumed; check that it was intended"],
	[
		"TreasuryDestinationSet",
		"treasury migrations are released to this Base address from now on; check that it is the governance timelock",
	],
]);

/** Ingests the blocks after the cursor up to `lastBlock`, each read at its
 *  hash in `hashes`: the chain that `sourceHashes` verified. Their migration
 *  events are compared with the second node's while it answers. */
async function ingestPendulumEvents(
	api: ApiPromise,
	draft: RuntimeState,
	lastBlock: number,
	hashes: Map<number, string>,
	crossChecked: boolean,
): Promise<void> {
	for (let block = draft.lastPendulumBlock + 1; block <= lastBlock; block++) {
		const blockHash = hashes.get(block);
		if (!blockHash) throw new Error(`no verified hash for Pendulum block ${block}`);
		const apiAt = await api.at(blockHash);
		const firstSeenMs = Number((await apiAt.query.timestamp.now()).toString().replaceAll(",", ""));
		const records = (await apiAt.query.system.events()) as unknown as EventRecords;
		const migrationEvents = migrationEventText(records);
		if (migrationEvents && crossChecked) await crossCheckEvents(block, blockHash, migrationEvents);
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
			const [nonceValue, whoValue, recipientValue, amountValue] = data as [
				{ toBigInt(): bigint },
				{ toHex(): string },
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
			const expectedTreasury = config.expectedTreasuryDestination;
			if (
				expectedTreasury &&
				whoValue.toHex().toLowerCase() === PENDULUM_TREASURY_ACCOUNT &&
				recipient.toLowerCase() !== expectedTreasury.toLowerCase()
			) {
				// The vault is paused before the attestors can release it; while
				// the pause is not confirmed, the monitor does not move past it.
				const paused = await securityViolation(
					"TREASURY DESTINATION VIOLATION",
					`treasury migration nonce ${nonce} in finalized Pendulum block ${block} pays ${amountValue.toBigInt()} ` +
						`pallet units to ${recipient}, not EXPECTED_TREASURY_DESTINATION ${expectedTreasury}; the monitor ` +
						"holds at that block until the vault is paused",
				);
				if (!paused) throw new PauseHold(`holding at Pendulum block ${block} until the vault is paused`);
			}
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
		draft.lastPendulumHash = blockHash;
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

/** A vault total at `blockNumber`: zero before the vault existed. */
async function vaultTotalAt(functionName: "totalReleased" | "totalSwept", blockNumber: bigint): Promise<bigint> {
	if (blockNumber < 0n) return 0n;
	try {
		return await publicClient.readContract({ address: config.vaultAddress, abi: vaultAbi, functionName, blockNumber });
	} catch (readError) {
		if (!(await publicClient.getBytecode({ address: config.vaultAddress, blockNumber }))) return 0n;
		throw readError;
	}
}

/** The vault's events in one block range, and `gap` when they do not account
 *  for the change of its running totals over the range: the Released amounts
 *  must add up to the growth of totalReleased, the RemainderSwept amounts to
 *  that of totalSwept. A backend that serves a truncated log result fails it. */
async function readVaultRange(start: bigint, end: bigint) {
	const [events, releasedBefore, sweptBefore, releasedAfter, sweptAfter] = await Promise.all([
		publicClient.getContractEvents({ address: config.vaultAddress, abi: vaultAbi, fromBlock: start, toBlock: end }),
		vaultTotalAt("totalReleased", start - 1n),
		vaultTotalAt("totalSwept", start - 1n),
		vaultTotalAt("totalReleased", end),
		vaultTotalAt("totalSwept", end),
	]);
	const approvals = events.filter((event) => event.eventName === "Approved");
	const releases = events.filter((event) => event.eventName === "Released");
	const released = releases.reduce((sum, event) => sum + (event.args as { tokenAmount: bigint }).tokenAmount, 0n);
	const swept = events
		.filter((event) => event.eventName === "RemainderSwept")
		.reduce((sum, event) => sum + (event.args as { amount: bigint }).amount, 0n);
	const gap = released !== releasedAfter - releasedBefore || swept !== sweptAfter - sweptBefore
		? `in Base blocks ${start}-${end} the Released events add up to ${released} and the RemainderSwept events to ` +
			`${swept}, but totalReleased grew by ${releasedAfter - releasedBefore} and totalSwept by ${sweptAfter - sweptBefore}`
		: undefined;
	return { approvals, releases, gap };
}

async function reconcileBaseEvents(
	draft: RuntimeState,
	toBlock: bigint,
	conversionFactor: bigint,
	sourceFinalizedTsMs: number,
	threshold: bigint,
): Promise<{ violation?: string; awaitingSource?: string; incomplete?: string }> {
	const releasedInScan = new Set<bigint>();
	let firstViolation: string | undefined;
	for (const [start, end] of blockRanges(draft.baseFromBlock, toBlock, config.baseMaxBlockRange)) {
		// Probe the range's end block first. Against a load-balanced endpoint,
		// `eth_getLogs` can be served by a node that has not reached `end` yet
		// and some providers then silently truncate rather than error — which
		// would advance the cursor past events this monitor never reconciled. A
		// lagging node fails this probe loudly instead, and the cycle replays.
		await publicClient.getBlock({ blockNumber: end });
		// The probe and the log query can still reach different backends, so the
		// logs must also account for the vault's totals; a range whose logs do
		// not is read once more, then held for the next poll.
		let range = await readVaultRange(start, end);
		if (range.gap) range = await readVaultRange(start, end);
		if (range.gap) {
			const heldMs = logScanHeldSinceMs === undefined ? 0 : Date.now() - logScanHeldSinceMs;
			logScanHeldSinceMs ??= Date.now();
			if (Date.now() - lastLogScanAlertMs > config.unmatchedEventGraceSeconds * 1000) {
				lastLogScanAlertMs = Date.now();
				// Held past the grace, no later Base event has been matched against its burn for that long.
				const critical = heldMs > config.unmatchedEventGraceSeconds * 1000;
				void alert(
					`${critical ? "CRITICAL: " : ""}BASE LOG SCAN INCOMPLETE`,
					`${range.gap}; the Base RPC served logs that miss events, so the range is read again every poll ` +
						`and the Base cursor stays at block ${start}` +
						(critical ? ` (for ${Math.round(heldMs / 1000)}s): no later Base event is matched against its burn` : "") +
						" — check the Base RPC",
				);
			}
			return { violation: firstViolation, incomplete: range.gap };
		}
		logScanHeldSinceMs = undefined;
		const { approvals, releases } = range;
		const observedAt = Date.now();
		let awaitingSource: string | undefined;

		// Preflight the complete range before deleting released migrations. If a
		// source RPC is merely behind, the range must be replayed intact later.
		for (const event of approvals) {
			const actual = event.args as { nonce: bigint; recipient: `0x${string}`; palletAmount: bigint; attestor: string };
			const expected = draft.migrations.get(actual.nonce);
			let mismatch = approvalMismatch(expected, actual);
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
				mismatch += disposition === "proven"
					? " (source view has passed the event's timestamp — fabricated)"
					: " (source view still behind after the full grace period)";
			}
			// Approvals that disagree with the burn pause the vault once they come
			// from two attestors, or from one at a threshold of two or less, where
			// one more approval releases. One attestor's approval alone pages.
			const { nonce, recipient, palletAmount, attestor } = actual;
			const seen = draft.mismatchedApprovals.some((approval) =>
				approval.nonce === nonce && approval.attestor.toLowerCase() === attestor.toLowerCase() &&
				!approvalMismatch(approval, actual)
			);
			if (!seen) draft.mismatchedApprovals.push({ nonce, recipient, palletAmount, attestor });
			const attestors = disagreeingAttestors(expected, nonce, draft.mismatchedApprovals);
			if (attestors.length >= (threshold > 2n ? 2 : 1)) {
				firstViolation ??= `Approved at Base block ${event.blockNumber}: ${mismatch}; attestor` +
					`${attestors.length > 1 ? "s" : ""} ${attestors.join(", ")} approved tuples for nonce ${nonce} that ` +
					"disagree with the finalized Pendulum view";
				continue;
			}
			void pageOnce(
				`approval:${event.transactionHash}:${event.logIndex}`,
				"CRITICAL: MISMATCHED APPROVAL",
				`Approved at Base block ${event.blockNumber} by attestor ${attestor}: ${mismatch}; one attestor's approval ` +
					"cannot release, so the vault is not paused — check that attestor's key and Pendulum node; a second " +
					"attestor approving this nonce against its burn pauses the vault",
			);
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
			draft.mismatchedApprovals = draft.mismatchedApprovals.filter((approval) => approval.nonce !== actual.nonce);
		}
		draft.baseFromBlock = end + 1n;
	}
	return { violation: firstViolation };
}

/** Page every vault admin event, and every operation the governance Timelock
 *  scheduled, up to `toBlock`, the poll's `safe`/`finalized` Base block.
 *  Before the governance handover the vault admin acts without a delay, so
 *  its events are the only trace of a guardian, attestor or cap change. The
 *  scan keeps its own cursor in `state`, so a restart resumes where it stopped. */
async function watchBaseAdmin(toBlock: bigint): Promise<void> {
	const timelock = config.timelockAddress;
	/** Scheduled calls by operation id and block: a cancelled operation can be
	 *  scheduled again with the same id, and then pages again. */
	const operations = new Map<string, { id: `0x${string}`; calls: ScheduledCall[] }>();
	const pages: Promise<boolean>[] = [];
	const fromBlock = state.baseAdminFromBlock;
	for (const [start, end] of blockRanges(fromBlock, toBlock, config.baseMaxBlockRange)) {
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
			const key = `timelock:${id}:${event.blockNumber}`;
			const operation = operations.get(key) ?? { id, calls: [] };
			operation.calls.push({ index, target, value, data });
			operations.set(key, operation);
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
		for (const [key, { id, calls }] of operations) {
			if (onceOnlyPages.has(key)) continue;
			const readyAt = await publicClient.readContract({
				address: timelock,
				abi: timelockAbi,
				functionName: "getTimestamp",
				args: [id],
				blockNumber: toBlock,
			});
			pages.push(pageOnce(
				key,
				"GOVERNANCE OPERATION SCHEDULED",
				describeTimelockOperation(id, calls, readyAt, Date.now(), labels, KNOWN_FUNCTIONS),
			));
		}
	}
	await Promise.all(pages);
	if (toBlock >= fromBlock) state.baseAdminFromBlock = toBlock + 1n;
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

/** The length of a noted preimage, which a legacy call reference leaves out.
 *  One noted before the pallet's status migration stays in the deprecated
 *  `statusFor` until something touches it. */
async function notedLength(apiAt: ApiAt, hash: { toHex(): string }): Promise<number | undefined> {
	const status = (await apiAt.query.preimage.requestStatusFor(hash)).toJSON() as {
		unrequested?: { len: number };
		requested?: { maybeLen: number | null };
	} | null;
	if (status) return status.unrequested?.len ?? status.requested?.maybeLen ?? undefined;
	const old = (await apiAt.query.preimage.statusFor(hash)).toJSON() as {
		unrequested?: { len: number };
		requested?: { len: number | null };
	} | null;
	return old?.unrequested?.len ?? old?.requested?.len ?? undefined;
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
 * every poll. Alert-only; it runs after the poll's conservation checks, so it
 * cannot stop or delay them, and decodes within fixed bounds, so its cost to
 * the next poll is bounded.
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
	const [count, lowest] = (await Promise.all([democracy.referendumCount(), democracy.lowestUnbaked()])).map((value) =>
		Number(value.toString())
	);
	/** The newest ongoing referendum of each call. */
	const referendumOf = new Map<string, number>();
	for (let index = lowest; index < count; index++) {
		const info = (await democracy.referendumInfoOf(index)) as unknown as {
			isSome: boolean;
			unwrap(): { isOngoing: boolean; asOngoing: { proposal: BoundedCall; end: { toNumber(): number } } };
		};
		if (!info.isSome || !info.unwrap().isOngoing) continue;
		const ongoing = info.unwrap().asOngoing;
		referendumOf.set(ongoing.proposal.toString(), index);
		items.push({
			key: `referendum:${index}`,
			label: `referendum #${index}`,
			hint: `voting ends at block ${ongoing.end.toNumber()}; a hostile one needs nay votes or an emergency cancel`,
			read: () => boundedCall(apiAt, ongoing.proposal),
		});
	}
	const external = (await democracy.nextExternal()) as unknown as { isSome: boolean; unwrap(): [BoundedCall, unknown] };
	if (external.isSome) {
		const [bounded] = external.unwrap();
		// Only its call identifies an external proposal. A tabled one becomes a
		// referendum with the same call, so that referendum keys the page of the
		// same call proposed again.
		items.push({
			key: `external:${referendumOf.get(bounded.toString()) ?? "-"}:${bounded.toString()}`,
			label: "the council's external proposal",
			hint: `it can become a referendum at block ${nextLaunch}`,
			read: () => boundedCall(apiAt, bounded),
		});
	} else {
		// Once it has left the queue, the same call proposed again pages again.
		for (const key of onceOnlyPages.keys()) if (key.startsWith("external:")) onceOnlyPages.delete(key);
	}
	for (const collective of ["council", "technicalCommittee"] as const) {
		const motions = (await apiAt.query[collective].proposals()) as unknown as Array<{ toHex(): string }>;
		for (const hash of motions) {
			// A closed motion can be proposed again with the same call and hash,
			// but never with the same index, so the index keys the page.
			const votes = (await apiAt.query[collective].voting(hash)).toJSON() as { index: number } | null;
			items.push({
				key: `${collective}:${votes?.index}:${hash.toHex()}`,
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
	const finalized = { number: finalizedHeader.number.toNumber(), hash: finalizedHash.toHex() };
	const { hashes, confirmed, crossChecked } = await sourceHashes(api, draft, finalized);
	// The view: the newest block the checkers confirm, or the block ingested
	// last while they confirm none after it. Every value read from Pendulum
	// below is read there.
	const viewBlock = Math.max(confirmed, draft.lastPendulumBlock);
	const viewHash = hashes.get(viewBlock) ?? draft.lastPendulumHash;
	if (!viewHash) throw new Error(`no verified hash for Pendulum block ${viewBlock}`);
	const apiAt = await api.at(viewHash);
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
			`verified Pendulum block ${viewBlock} is ${Math.round((Date.now() - sourceFinalizedTsMs) / 1000)}s old` +
				(finalized.number > viewBlock ? ` (the source finalized ${finalized.number}, not confirmed yet)` : "") +
				`; the monitor cannot verify new Base events against a stalled source — restore the node before ` +
				`the ${config.unmatchedEventGraceSeconds}s unmatched-event grace forces a pause`,
		);
	}
	const [totals, totalIssuanceValue] = await Promise.all([migrationTotals(apiAt), apiAt.query.balances.totalIssuance()]);
	const { nextNonce: chainNextNonce, totalMigrated } = totals;
	if (crossChecked !== undefined && crossChecked >= viewBlock) await crossCheckTotals(viewBlock, viewHash, totals);
	await ingestPendulumEvents(api, draft, confirmed, hashes, crossChecked !== undefined);
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
	return { finalizedBlock: viewBlock, timestampMs: sourceFinalizedTsMs, totalMigrated };
}

/** When the vault balance check last started, and whether a poll is running. */
let lastBalanceCheckMs = 0;
let polling = false;

/** The vault's totals at the confirmed Base block `blockNumber`, after the
 *  vault balance check (M2b) on them, which pauses the vault on a deficit. */
async function checkVaultBalance(blockNumber: bigint) {
	lastBalanceCheckMs = Date.now();
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
	if (vaultConservationDeficit(vaultBalance, totalReleased, totalSwept, totalSupply)) {
		await securityViolation(
			"VAULT BALANCE DEFICIT",
			`balance ${vaultBalance} + released ${totalReleased} + swept ${totalSwept} < supply ${totalSupply}`,
		);
	}
	return { totalReleased, totalSwept, conversionFactor, vaultBalance };
}

/** Runs the vault balance check on its own while a poll is still busy a poll
 *  interval after its last one started (a slow Pendulum source, a long
 *  catch-up). main() calls this every poll interval, so a busy poll delays
 *  the check by two poll intervals at most. */
export async function watchVaultBalance(): Promise<void> {
	if (!polling || Date.now() - lastBalanceCheckMs < config.pollIntervalMs) return;
	lastBalanceCheckMs = Date.now();
	try {
		const confirmed = await publicClient.getBlock({ blockTag: config.baseFinalityTag });
		if (confirmed.number === null) throw new Error(`${config.baseFinalityTag} Base block has no number`);
		await checkVaultBalance(confirmed.number);
	} catch (balanceError) {
		await alert("monitor check failed", `the vault balance check failed: ${balanceError}`);
	}
}

/** One pass of the checks; resolves to whether every one of them ran. */
async function check(api: ApiPromise): Promise<boolean> {
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

	// The vault balance check (M2b) reads Base only, so it runs before anything
	// that waits on Pendulum or scans Base logs: a failing source or log scan
	// cannot skip it, and while the rest of a poll takes longer than a poll
	// interval, `watchVaultBalance` runs it on its own. Its pause does not end
	// the poll.
	const { totalReleased, totalSwept, conversionFactor, vaultBalance } = await checkVaultBalance(baseBlock);

	// The admin scan only pages and keeps its own cursor, so nothing else waits
	// for it, and its failure skips no other check.
	let adminScanned = true;
	try {
		await watchBaseAdmin(baseBlock);
		adminScanFailingSinceMs = undefined;
	} catch (adminError) {
		adminScanned = false;
		const failingMs = adminScanFailingSinceMs === undefined ? 0 : Date.now() - adminScanFailingSinceMs;
		adminScanFailingSinceMs ??= Date.now();
		if (Date.now() - lastAdminScanFailureAlertMs > config.unmatchedEventGraceSeconds * 1000) {
			lastAdminScanFailureAlertMs = Date.now();
			// Meanwhile the veto window of any operation scheduled since runs unseen.
			const critical = failingMs > config.unmatchedEventGraceSeconds * 1000;
			await alert(
				`${critical ? "CRITICAL: " : ""}BASE ADMIN SCAN FAILED`,
				`${adminError}; vault admin events and the governance queue are not paged until the scan reads again` +
					(critical ? ` (failing for ${Math.round(failingMs / 1000)}s)` : "") +
					", and it then resumes where it stopped; every other check still runs",
			);
		}
	}

	// A Pendulum failure must not switch off the Base-side checks. Base events
	// are then reconciled against the last view read in full, as against a
	// lagging source but with no fabrication proof: a mismatch on a known nonce
	// is judged at once, an unknown nonce after the grace. The vault balance
	// check needs no source; M2a and liveness wait for it.
	let draft = cloneState(state);
	let source: SourceView | undefined;
	try {
		source = await readSource(api, draft);
	} catch (sourceError) {
		draft = cloneState(state); // drops a half-ingested block; the 200-block checkpoints already moved `state`
		const contradicted = sourceError instanceof SourceIntegrityError;
		const consequence = `Base events are checked against the last Pendulum view read in full: one whose nonce ` +
			`that view lacks pauses the vault ${config.unmatchedEventGraceSeconds}s after its first sighting, and M2a ` +
			"and liveness are skipped";
		if (sourceError instanceof PauseHold) log(sourceError.message);
		else if (contradicted && Date.now() - lastSourceIntegrityAlertMs > config.unmatchedEventGraceSeconds * 1000) {
			lastSourceIntegrityAlertMs = Date.now();
			await alert(
				"CRITICAL: PENDULUM SOURCE CONTRADICTED",
				`${sourceError}; nothing from the source is ingested until it verifies again, and until then ` +
					`${consequence} — stop trusting the source node and compare it with other nodes and the relay chain`,
			);
		} else if (!contradicted && Date.now() - lastSourceFailureAlertMs > config.unmatchedEventGraceSeconds * 1000) {
			lastSourceFailureAlertMs = Date.now();
			await alert("PENDULUM SOURCE UNREADABLE", `${sourceError}; until the source reads again, ${consequence}`);
		}
	}

	const threshold = await publicClient.readContract({
		address: config.vaultAddress,
		abi: vaultAbi,
		functionName: "threshold",
		blockNumber: baseBlock,
	});
	const reconciliation = await reconcileBaseEvents(draft, baseBlock, conversionFactor, source?.timestampMs ?? 0, threshold);
	if (reconciliation.violation) {
		if (await securityViolation("MIGRATION TUPLE VIOLATION", reconciliation.violation)) {
			state = draft;
			persistState(state);
		}
		return false;
	}
	if (!source) {
		state = draft; // reconciled against the last view read in full
		persistState(state);
		return false;
	}
	const { finalizedBlock, totalMigrated } = source;
	// M2a needs only the totals, so a held log range does not stop it. A Base
	// event whose burn the source view does not show yet would make it fire
	// falsely, so it waits for that.
	if (
		!reconciliation.awaitingSource &&
		releasedExceedsMigrated(totalReleased, totalMigrated, conversionFactor, config.acknowledgedReleaseExcess)
	) {
		if (await securityViolation(
			"CONSERVATION VIOLATION",
			`released ${totalReleased} > migrated ${totalMigrated * conversionFactor} + acknowledged excess ` +
				`${config.acknowledgedReleaseExcess} (token units)`,
		)) {
			state = draft;
			persistState(state);
		}
		return false;
	}
	if (reconciliation.awaitingSource || reconciliation.incomplete) {
		state = draft;
		persistState(state);
		log(
			reconciliation.incomplete
				? `holding the Base cursor at ${draft.baseFromBlock}: ${reconciliation.incomplete}`
				: `waiting up to ${config.unmatchedEventGraceSeconds}s for Pendulum RPC to catch up with ` +
					reconciliation.awaitingSource,
		);
		return false;
	}

	const pending = [...draft.migrations.values()];
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
	return adminScanned;
}

/** Tells an external dead-man's switch (Healthchecks.io style) that a poll
 *  ran every check; it pages when these requests stop. Never fatal. */
async function heartbeat(): Promise<void> {
	if (!config.heartbeatUrl) return;
	try {
		const response = await fetch(config.heartbeatUrl, { signal: AbortSignal.timeout(10_000) });
		if (!response.ok) console.error(`heartbeat answered ${response.status}`);
	} catch (heartbeatError) {
		console.error(`heartbeat failed: ${redact(`${heartbeatError}`)}`);
	}
}

/** Startup once the Pendulum API is connected: durable state, the multicall
 *  probe, the state identity check and the auto-pause readiness. `connect`
 *  replaces how the nodes of the integrity checks are reached (tests). */
export async function start(api: ApiPromise, connect?: (url: string) => Promise<ApiPromise>): Promise<void> {
	if (connect) connectNode = connect;
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
		await pageOnce(
			"autopause-disabled",
			"AUTO-PAUSE DISABLED",
			"no GUARDIAN_PRIVATE_KEY configured: violations page, but nothing pauses the vault automatically",
		);
	}
	const readiness = await checkAutoPauseReadiness();
	log(
		`monitor started; Pendulum after ${state.lastPendulumBlock}, Base from ${state.baseFromBlock}, ` +
			`polling every ${config.pollIntervalMs}ms; governance queue ` +
			(config.timelockAddress ? `watched at ${config.timelockAddress}` : "not watched (TIMELOCK_ADDRESS unset)") +
			`; second Pendulum node ${config.pendulumWsCrossCheck ? "cross-checked" : "not set"}` +
			`; auto-pause ${!config.guardianPrivateKey ? "disabled" : readiness.length ? "NOT READY" : "ready"}` +
			`; acknowledged release excess ${config.acknowledgedReleaseExcess} token units`,
	);
}

/** One poll; a failure pages and the next poll starts afresh. */
export async function poll(api: ApiPromise): Promise<void> {
	polling = true;
	let complete = false;
	try {
		complete = await check(api);
	} catch (checkError) {
		await alert("monitor check failed", `${checkError}`);
	}
	try {
		await watchPendulumGovernance(api);
	} catch (governanceError) {
		complete = false;
		if (Date.now() - lastGovernanceWatchFailureMs > config.graceSeconds * 1000) {
			lastGovernanceWatchFailureMs = Date.now();
			await alert("pendulum governance watch failed", `${governanceError}`);
		}
	}
	await retryUndeliveredPages();
	if (complete) await heartbeat();
	polling = false;
}

async function main(): Promise<void> {
	const api = await ApiPromise.create({ provider: new WsProvider(config.pendulumWs) });
	await start(api);
	setInterval(() => void watchVaultBalance(), config.pollIntervalMs);
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
