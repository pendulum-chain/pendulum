function required(name: string): string {
	const value = process.env[name];
	if (!value) {
		throw new Error(`Missing required environment variable ${name}`);
	}
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
	pollIntervalMs: Number(process.env.POLL_INTERVAL_MS ?? "60000"),
	/** Max blocks per `eth_getLogs` call; public RPCs cap this. */
	maxBlockRange: BigInt(process.env.MAX_BLOCK_RANGE ?? "10000"),
	/** Alert when the gas wallet drops below this balance (wei). */
	minGasBalanceWei: BigInt(process.env.MIN_GAS_BALANCE_WEI ?? "5000000000000000"),
	alertWebhookUrl: process.env.ALERT_WEBHOOK_URL,
	baseChainId: Number(process.env.BASE_CHAIN_ID ?? "8453"),
};
