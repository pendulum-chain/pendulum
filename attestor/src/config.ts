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
function envNumber(name: string, fallback: number): number {
	const raw = process.env[name];
	if (raw === undefined || raw === "") return fallback;
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

export const config = {
	/** WebSocket endpoint of THIS OPERATOR'S OWN Pendulum full node (PRD A1).
	 *  Never point this at a public RPC: the attestor would inherit its honesty.
	 *  The node must retain historical state (`--state-pruning archive`), or any
	 *  daemon outage longer than the pruning horizon wedges the catch-up. */
	pendulumWs: required("PENDULUM_WS"),
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
	/** Base chain id: 8453 mainnet. */
	baseChainId: envNumber("BASE_CHAIN_ID", 8453),
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
};
