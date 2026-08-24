/** Minimal MigrationVault ABI: only what the releaser needs. */
export const vaultAbi = [
	{
		type: "event",
		name: "ReleasePending",
		inputs: [
			{ name: "nonce", type: "uint64", indexed: true },
			{ name: "recipient", type: "address", indexed: true },
			{ name: "tokenAmount", type: "uint256", indexed: false },
		],
	},
	{
		type: "function",
		name: "release",
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
		name: "conversionFactor",
		stateMutability: "view",
		inputs: [],
		outputs: [{ type: "uint256" }],
	},
] as const;
