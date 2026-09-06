function required(name: string): string {
	const value = process.env[name];
	if (!value) {
		throw new Error(`Missing required environment variable ${name}`);
	}
	return value;
}

/** A numeric environment value, validated at startup: a typo must fail loudly
 *  here, not surface later as NaN-driven behavior. */
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

export const config = {
	/** Base JSON-RPC endpoint. */
	baseRpcUrl: required("BASE_RPC_URL"),
	/** MigrationVault contract address on Base. */
	vaultAddress: required("VAULT_ADDRESS") as `0x${string}`,
	/** Gas-only signing key. `release()` is permissionless and can only pay the
	 *  recipient the attestors already approved, so this key holds NO privilege
	 *  over the vault — it must never be an attestor, guardian or admin key. */
	releaserPrivateKey: required("RELEASER_PRIVATE_KEY") as `0x${string}`,
	/** File persisting the log-scan checkpoint and the pending set. */
	stateFile: process.env.STATE_FILE ?? "./releaser-state.json",
	/** Base block to begin scanning `ReleasePending` from on a first run —
	 *  set to the vault's deployment block. */
	startBlock: BigInt(process.env.START_BLOCK ?? "0"),
	pollIntervalMs: envNumber("POLL_INTERVAL_MS", 60_000),
	/** Max blocks per `eth_getLogs` call; public RPCs cap this. */
	maxBlockRange: BigInt(process.env.MAX_BLOCK_RANGE ?? "10000"),
	/** Alert when the gas wallet drops below this balance (wei). */
	minGasBalanceWei: BigInt(process.env.MIN_GAS_BALANCE_WEI ?? "5000000000000000"),
	alertWebhookUrl: process.env.ALERT_WEBHOOK_URL,
	baseChainId: envNumber("BASE_CHAIN_ID", 8453),
	/** Only logs and state inside this Base confirmation boundary may advance
	 * durable state: ReleasePending ingestion scans up to it, and a pending
	 * entry is dropped only once its nonce is consumed there. */
	baseFinalityTag: finalityTag(),
	/** How often a governance-blocked release (per-release cap exceeded,
	 * under-funded vault) re-pages. It cannot clear in less than the 48h
	 * timelock, so per-poll paging would only bury the signal. */
	blockedAlertIntervalMs: envNumber("BLOCKED_ALERT_INTERVAL_MS", 6 * 60 * 60 * 1000),
	/** Bound both Multicall calldata and individual fallback concurrency. */
	readBatchSize: envNumber("READ_BATCH_SIZE", 100),
};
