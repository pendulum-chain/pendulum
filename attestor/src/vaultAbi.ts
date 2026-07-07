/** Minimal MigrationVault ABI: only what the attestor needs. */
export const vaultAbi = [
	{
		type: "function",
		name: "approve",
		stateMutability: "nonpayable",
		inputs: [
			{ name: "nonce", type: "uint64" },
			{ name: "recipient", type: "address" },
			{ name: "palletAmount", type: "uint256" },
		],
		outputs: [],
	},
	{
		type: "function",
		name: "nonceConsumed",
		stateMutability: "view",
		inputs: [{ name: "nonce", type: "uint64" }],
		outputs: [{ type: "bool" }],
	},
	{
		type: "function",
		name: "hasApproved",
		stateMutability: "view",
		inputs: [
			{ name: "payload", type: "bytes32" },
			{ name: "attestor", type: "address" },
		],
		outputs: [{ type: "bool" }],
	},
	{
		type: "function",
		name: "isAttestor",
		stateMutability: "view",
		inputs: [{ name: "attestor", type: "address" }],
		outputs: [{ type: "bool" }],
	},
] as const;
