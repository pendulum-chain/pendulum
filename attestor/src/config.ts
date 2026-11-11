import { parseGwei } from "viem";
import { parseHeartbeatUrl } from "./alert.js";
import { MIN_REPLACEMENT_PERCENT } from "./approval.js";

function required(name: string): string {
	const value = process.env[name];
	if (!value) {
		throw new Error(`Missing required environment variable ${name}`);
	}
	return value;
}

/** A numeric environment value, validated at startup: a typo must fail loudly
 *  here, not surface later as NaN-driven behavior (a NaN timeout, for
 *  instance, silently turns a backoff into a busy loop). */
function envNumber(name: string, fallback: number, min = 0, integer = false): number {
	const raw = process.env[name];
	if (raw === undefined || raw === "") return fallback;
	const value = Number(raw);
	if (!Number.isFinite(value) || value < min || (integer && !Number.isInteger(value))) {
		throw new Error(`invalid ${name}: ${raw}`);
	}
	return value;
}

/** A positive gwei amount given as a decimal, e.g. `0.5`. */
function envGwei(name: string, fallback: string): bigint {
	const raw = process.env[name] || fallback;
	if (!/^\d+(\.\d{1,9})?$/.test(raw) || parseGwei(raw) === 0n) throw new Error(`invalid ${name}: ${raw}`);
	return parseGwei(raw);
}

function finalityTag(): "safe" | "finalized" {
	const value = process.env.BASE_FINALITY_TAG ?? "safe";
	if (value !== "safe" && value !== "finalized") throw new Error(`invalid BASE_FINALITY_TAG ${value}`);
	return value;
}

const baseFinalityTag = finalityTag();

const baseChainId = envNumber("BASE_CHAIN_ID", 8453);

/** Pendulum's genesis block hash. */
const PENDULUM_GENESIS_HASH = "0x5d3c298622d5634ed019bf61ea4b71655030015bde9beb0d6a24743714462c86";

function genesisHash(): `0x${string}` {
	const value = process.env.PENDULUM_GENESIS_HASH || PENDULUM_GENESIS_HASH;
	if (!/^0x[0-9a-fA-F]{64}$/.test(value)) throw new Error(`invalid PENDULUM_GENESIS_HASH ${value}`);
	// Another genesis is for rehearsals against a test chain; the mainnet vault
	// only ever pays for burns on Pendulum itself.
	if (baseChainId === 8453 && value.toLowerCase() !== PENDULUM_GENESIS_HASH) {
		throw new Error(`PENDULUM_GENESIS_HASH ${value} is not Pendulum's, which BASE_CHAIN_ID 8453 (Base mainnet) requires`);
	}
	return value as `0x${string}`;
}

export const config = {
	/** WebSocket endpoint of THIS OPERATOR'S OWN Pendulum full node (PRD A1).
	 *  Never point this at a public RPC: the attestor would inherit its honesty.
	 *  The node must retain historical state (`--state-pruning archive`), or any
	 *  daemon outage longer than the pruning horizon wedges the catch-up. */
	pendulumWs: required("PENDULUM_WS"),
	/** The genesis that node must serve; startup refuses any other chain. A
	 *  local test chain sets its own, never on Base mainnet. */
	pendulumGenesisHash: genesisHash(),
	/** Base JSON-RPC endpoint. */
	baseRpcUrl: required("BASE_RPC_URL"),
	/** MigrationVault contract address on Base. */
	vaultAddress: required("VAULT_ADDRESS") as `0x${string}`,
	/** This attestor's transaction-signing key (0x-prefixed, 32 bytes).
	 *  Isolate per operator; fund with Base ETH for gas (PRD A3). */
	attestorPrivateKey: required("ATTESTOR_PRIVATE_KEY") as `0x${string}`,
	/** File persisting the last durably processed finalized block (PRD A2). */
	checkpointFile: process.env.CHECKPOINT_FILE ?? "./checkpoint.json",
	/** Pendulum block to start from on the very first run (the block of the
	 *  runtime upgrade that added the token-migration pallet). */
	startBlock: envNumber("START_BLOCK", 0),
	/** Alert when the gas wallet drops below this balance (wei). */
	minGasBalanceWei: BigInt(process.env.MIN_GAS_BALANCE_WEI ?? "10000000000000000"), // 0.01 ETH
	/** Optional webhook that receives JSON alerts (low gas, fatal errors). */
	alertWebhookUrl: process.env.ALERT_WEBHOOK_URL,
	/** Optional http(s) URL requested after a finalized head processed without
	 *  error, at most once a minute: a dead-man's switch pages when the
	 *  requests stop. */
	heartbeatUrl: parseHeartbeatUrl(process.env.HEARTBEAT_URL),
	/** Base chain id: 8453 mainnet. */
	baseChainId,
	/** Confirmation boundary required before the checkpoint may pass a block. */
	baseFinalityTag,
	/** How long a block's approvals may stay outside the finality boundary
	 *  before they are alerted on and re-submitted (they are idempotent). The
	 *  default tracks the boundary's normal lag: `finalized` trails `safe` by
	 *  L1 finality, so it gets a proportionally longer default. */
	baseFinalityTimeoutMs: envNumber(
		"BASE_FINALITY_TIMEOUT_MS",
		baseFinalityTag === "finalized" ? 2_700_000 : 900_000,
	),
	/** Page when no finalized Pendulum head has arrived for this long: the
	 *  daemon is purely push-driven, so a node that stops finalizing (or a
	 *  subscription that silently died) would otherwise idle undetected. */
	headStallAlertMs: envNumber("HEAD_STALL_ALERT_MS", 300_000),
	/** Page after this many consecutive migrations resolved on-chain without
	 *  this attestor's own approval landing (and again every as many more):
	 *  lost races do not count, so this means the submission path is broken
	 *  while the rest of the fleet carries the releases. */
	missedApprovalAlertCount: envNumber("MISSED_APPROVAL_ALERT_COUNT", 3, 1, true),
	/** An own approval still not mined after `afterMs` is re-sent at the same
	 *  nonce with both fees raised by `percent` (at least the 10 % a replacement
	 *  needs, and at least to the current estimate), at most `max` times and
	 *  never above `maxFeePerGas`; then it pages once. Base mines a block every
	 *  2 s and an approval priced at the estimate lands in the next one or two,
	 *  so 60 s means stuck, not slow. Since each bump also catches up with the
	 *  estimate, one normally suffices; when three do not, the price is not the
	 *  problem and the page (about 4 min after the first send) goes to a person.
	 *  The cap is 200 times Base's usual base fee (about 0.005 gwei): only an
	 *  extreme spike stops a bump, and one approval (at most about 300k gas)
	 *  never costs more than about 0.0003 ETH. */
	feeBump: {
		afterMs: envNumber("FEE_BUMP_AFTER_SECONDS", 60, 1) * 1000,
		percent: envNumber("FEE_BUMP_PERCENT", 20, MIN_REPLACEMENT_PERCENT, true),
		max: envNumber("FEE_BUMP_MAX", 3, 0, true),
		maxFeePerGas: envGwei("MAX_FEE_PER_GAS_GWEI", "1"),
	},
};
