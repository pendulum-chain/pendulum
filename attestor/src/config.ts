function required(name: string): string {
	const value = process.env[name];
	if (!value) {
		throw new Error(`Missing required environment variable ${name}`);
	}
	return value;
}

export const config = {
	/** WebSocket endpoint of THIS OPERATOR'S OWN Pendulum full node (PRD A1).
	 *  Never point this at a public RPC: the attestor would inherit its honesty. */
	pendulumWs: required("PENDULUM_WS"),
	/** Base JSON-RPC endpoint. */
	baseRpcUrl: required("BASE_RPC_URL"),
	/** MigrationVault contract address on Base. */
	vaultAddress: required("VAULT_ADDRESS") as `0x${string}`,
	/** This attestor's transaction-signing key (0x-prefixed, 32 bytes).
	 *  Isolate per operator; fund with Base ETH for gas (PRD A3). */
	attestorPrivateKey: required("ATTESTOR_PRIVATE_KEY") as `0x${string}`,
	/** File persisting the last fully processed finalized block (PRD A2). */
	checkpointFile: process.env.CHECKPOINT_FILE ?? "./checkpoint.json",
	/** Pendulum block to start from on the very first run (the block of the
	 *  runtime upgrade that added the token-migration pallet). */
	startBlock: Number(process.env.START_BLOCK ?? "0"),
	/** Alert when the gas wallet drops below this balance (wei). */
	minGasBalanceWei: BigInt(process.env.MIN_GAS_BALANCE_WEI ?? "10000000000000000"), // 0.01 ETH
	/** Optional webhook that receives JSON alerts (low gas, fatal errors). */
	alertWebhookUrl: process.env.ALERT_WEBHOOK_URL,
	/** Base chain id: 8453 mainnet. */
	baseChainId: Number(process.env.BASE_CHAIN_ID ?? "8453"),
};
