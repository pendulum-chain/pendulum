/**
 * Shared Anvil/Base helpers: well-known dev accounts and a viem client pair.
 *
 * These are Anvil's deterministic development keys. They are public knowledge
 * and exist only to drive a throwaway local chain — never reuse them anywhere
 * that holds value.
 */

import { createPublicClient, createWalletClient, defineChain, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";

export const RPC = process.env.BASE_RPC_URL ?? "http://127.0.0.1:8545";

/** Explicit gas for every harness transaction. Anvil fills a missing limit
 *  from eth_estimateGas, which runs at the current wall-clock second; the
 *  vault delegates its votes to the sink, so a PEN transfer touching the vault
 *  writes an ERC20Votes checkpoint keyed by block timestamp — an OVERWRITE at
 *  estimation time becomes a NEW entry when the block lands one second later,
 *  and the exact estimate runs out of gas in the checkpoint write. */
const HARNESS_GAS = 1_000_000n;

const KEYS = [
	"0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80", // 0 deployer
	"0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d", // 1 attestor A
	"0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a", // 2 attestor B
	"0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6", // 3 attestor C
	"0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a", // 4 attestor D
	"0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba", // 5 guardian
	"0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e", // 6 admin
	"0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356", // 7 releaser (gas only)
];

export const accounts = KEYS.map((k) => privateKeyToAccount(k));
export const keys = KEYS;
export const [deployer, attA, attB, attC, attD, guardian, admin, releaser] = accounts;
export const attestors = [attA, attB, attC, attD];

export const anvilChain = defineChain({
	id: Number(process.env.BASE_CHAIN_ID ?? 31337),
	name: "anvil",
	nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
	rpcUrls: { default: { http: [RPC] } },
});

export const pub = createPublicClient({ chain: anvilChain, transport: http(RPC) });

// The daemons pin their durability reads to Base `safe`/`finalized`. Anvil
// resolves those tags to GENESIS until the chain is an epoch (32 blocks) deep,
// so a default Anvil hands every attestor a pre-vault block on its first
// checkpoint and the fleet dies silently (found the hard way). Refuse to run
// against an Anvil started without `--slots-in-an-epoch 0`.
{
	const [latest, safe] = await Promise.all([pub.getBlock({ blockTag: "latest" }), pub.getBlock({ blockTag: "safe" })]);
	if (safe.number !== latest.number) {
		throw new Error(
			`Anvil at ${RPC} resolves "safe" to block ${safe.number} while latest is ${latest.number}; ` +
				"start it with `anvil --port 8545 --slots-in-an-epoch 0` so the daemons' finality-boundary reads see the vault",
		);
	}
}
export const wallet = (account) => createWalletClient({ account, chain: anvilChain, transport: http(RPC) });

/** Send a contract call and wait for it to land, returning the receipt. */
export async function send(account, params) {
	const { request } = await pub.simulateContract({ account, ...params });
	const hash = await wallet(account).writeContract({ ...request, gas: HARNESS_GAS });
	return pub.waitForTransactionReceipt({ hash });
}

/** Send as an arbitrary address on Anvil. This is intentionally isolated to
 * local failure drills; it lets the harness model an impossible token outflow
 * from the vault and prove the independent monitor catches it. */
export async function sendImpersonated(address, params) {
	await pub.request({ method: "anvil_impersonateAccount", params: [address] });
	await pub.request({ method: "anvil_setBalance", params: [address, "0x8ac7230489e80000"] }); // 10 ETH
	const impersonated = createWalletClient({ account: address, chain: anvilChain, transport: http(RPC) });
	try {
		const hash = await impersonated.writeContract({ gas: HARNESS_GAS, ...params, account: address });
		return await pub.waitForTransactionReceipt({ hash });
	} finally {
		await pub.request({ method: "anvil_stopImpersonatingAccount", params: [address] });
	}
}

/** Attempt a call and return the revert reason instead of throwing. */
export async function expectRevert(account, params) {
	try {
		await pub.simulateContract({ account, ...params });
		return null; // no revert
	} catch (error) {
		return error instanceof Error ? error.message : String(error);
	}
}

/** Advance the chain clock (Anvil only). */
export async function warp(seconds) {
	await pub.request({ method: "evm_increaseTime", params: [seconds] });
	await pub.request({ method: "evm_mine", params: [] });
}
