/** Minimal MigrationVault ABI: only what the releaser needs. */
export const vaultAbi = [
	{ type: "error", name: "NonceAlreadyConsumed", inputs: [{ name: "nonce", type: "uint64" }] },
	{
		type: "error",
		name: "NotEnoughApprovals",
		inputs: [
			{ name: "active", type: "uint256" },
			{ name: "required", type: "uint256" },
		],
	},
	{ type: "error", name: "EnforcedPause", inputs: [] },
	{
		type: "error",
		name: "ExceedsPerReleaseCap",
		inputs: [
			{ name: "amount", type: "uint256" },
			{ name: "cap", type: "uint256" },
		],
	},
	{
		type: "error",
		name: "ExceedsDailyCap",
		inputs: [
			{ name: "requested", type: "uint256" },
			{ name: "available", type: "uint256" },
		],
	},
	{ type: "error", name: "InsufficientVaultBalance", inputs: [] },
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
	{
		type: "function",
		name: "dailyCap",
		stateMutability: "view",
		inputs: [],
		outputs: [{ type: "uint256" }],
	},
] as const;
