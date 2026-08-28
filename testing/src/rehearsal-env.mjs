/**
 * Environment, clients and guardrails for the rehearsal.
 *
 * The single most important thing in this file is `assertTestnet`. This script
 * exists to be run casually and repeatedly, with real keys in a real .env, and
 * it deploys contracts and moves tokens. The cost of it ever pointing at Base
 * mainnet or at real Pendulum is unbounded, so both are checked explicitly
 * before anything is deployed or signed.
 */

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createPublicClient, createWalletClient, defineChain, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";

export const TESTING = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const ROOT = path.resolve(TESTING, "..");

export const BASE_SEPOLIA_CHAIN_ID = 84532;
export const BASE_MAINNET_CHAIN_ID = 8453;
/** Canonical Multicall3, deployed at the same address on Base Sepolia. The
 *  monitor and releaser batch their reads through it and viem refuses to use
 *  it unless the chain definition names it. */
export const MULTICALL3 = "0xcA11bde05977b3631167028862bE2a173976CA11";

export const PEN_18 = 10n ** 18n; // Base side
export const PEN_12 = 10n ** 12n; // pallet side
export const CONVERSION_FACTOR = 1_000_000n;

const REQUIRED = [
	"BASE_SEPOLIA_RPC_URL",
	"DEPLOYER_PRIVATE_KEY",
	"ATTESTOR_1_PRIVATE_KEY",
	"ATTESTOR_2_PRIVATE_KEY",
	"ATTESTOR_3_PRIVATE_KEY",
	"ATTESTOR_4_PRIVATE_KEY",
	"GUARDIAN_PRIVATE_KEY",
	"ADMIN_PRIVATE_KEY",
	"RELEASER_PRIVATE_KEY",
];

/** Parse a dotenv-style file without taking on a dependency. */
function parseEnvFile(file) {
	const out = {};
	let text;
	try {
		text = readFileSync(file, "utf8");
	} catch {
		throw new Error(
			`missing ${file}\n\n  cp testing/.env.rehearsal.example testing/.env.rehearsal\n\nthen fill in the throwaway keys.`,
		);
	}
	for (const raw of text.split("\n")) {
		const line = raw.trim();
		if (!line || line.startsWith("#")) continue;
		const eq = line.indexOf("=");
		if (eq === -1) continue;
		out[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
	}
	return out;
}

export function loadEnv() {
	const file = path.join(TESTING, ".env.rehearsal");
	const env = parseEnvFile(file);
	const missing = REQUIRED.filter((k) => !env[k]);
	if (missing.length) {
		throw new Error(`${file} is missing values for:\n  ${missing.join("\n  ")}`);
	}
	for (const key of REQUIRED) {
		if (key.endsWith("PRIVATE_KEY") && !/^0x[0-9a-fA-F]{64}$/.test(env[key])) {
			throw new Error(`${key} is not a 32-byte hex private key`);
		}
	}
	return env;
}

export function buildContext(env) {
	const chain = defineChain({
		id: BASE_SEPOLIA_CHAIN_ID,
		name: "base-sepolia",
		nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
		rpcUrls: { default: { http: [env.BASE_SEPOLIA_RPC_URL] } },
		contracts: { multicall3: { address: MULTICALL3 } },
	});
	const key = (name) => env[name];
	const account = (name) => privateKeyToAccount(key(name));

	const roles = {
		deployer: account("DEPLOYER_PRIVATE_KEY"),
		attestors: [1, 2, 3, 4].map((i) => account(`ATTESTOR_${i}_PRIVATE_KEY`)),
		guardian: account("GUARDIAN_PRIVATE_KEY"),
		admin: account("ADMIN_PRIVATE_KEY"),
		releaser: account("RELEASER_PRIVATE_KEY"),
	};
	const pub = createPublicClient({ chain, transport: http(env.BASE_SEPOLIA_RPC_URL) });
	const wallet = (acct) => createWalletClient({ account: acct, chain, transport: http(env.BASE_SEPOLIA_RPC_URL) });

	return { chain, roles, pub, wallet, keys: { ...env } };
}

/** Send a transaction and wait for it, failing loudly on revert. */
export async function send(ctx, acct, params) {
	const { request } = await ctx.pub.simulateContract({ account: acct, ...params });
	const hash = await ctx.wallet(acct).writeContract(request);
	const receipt = await ctx.pub.waitForTransactionReceipt({ hash });
	if (receipt.status !== "success") throw new Error(`transaction reverted: ${hash}`);
	return receipt;
}

/**
 * Refuse to run anywhere that could cost real money.
 *
 * Checked before any deployment: the EVM side must be Base Sepolia, and the
 * Substrate side must be the local Zombienet chain rather than Pendulum
 * mainnet. `REHEARSAL_ALLOW_CHAIN_ID` exists for a deliberate move to another
 * testnet and still refuses Base mainnet outright.
 */
export async function assertTestnet(ctx, substrateApi) {
	const chainId = await ctx.pub.getChainId();
	const allowed = Number(process.env.REHEARSAL_ALLOW_CHAIN_ID ?? BASE_SEPOLIA_CHAIN_ID);
	if (chainId === BASE_MAINNET_CHAIN_ID) {
		throw new Error("REFUSING TO RUN: the RPC points at Base MAINNET (chain 8453).");
	}
	if (chainId !== allowed) {
		throw new Error(
			`REFUSING TO RUN: expected chain ${allowed} (Base Sepolia), got ${chainId}. ` +
				"Set REHEARSAL_ALLOW_CHAIN_ID only if you mean it.",
		);
	}
	if (substrateApi) {
		const name = (await substrateApi.rpc.system.chain()).toString();
		if (!/local/i.test(name)) {
			throw new Error(
				`REFUSING TO RUN: the Substrate endpoint reports "${name}", which is not the local ` +
					"Zombienet chain. This script burns real balances; point it at the local network.",
			);
		}
	}
	return chainId;
}
