/**
 * RB-5 drill — a runtime upgrade lands while the attestor fleet is running.
 *
 * The property under test: attestors keep decoding `MigrationInitiated` across
 * an upgrade that does not change the event (the shape-change half of RB-5 —
 * fail loudly rather than skip — is unit-tested in the attestor via its
 * 4-field assertion). This is the one drill that cannot run on Zombienet or
 * Sepolia: swapping a runtime needs either root (no sudo on Pendulum) or
 * storage access, so it runs on a Chopsticks fork, where `:code` can be
 * written directly — which is also exactly what a real enacted upgrade does.
 *
 * Prerequisites:
 *   - Chopsticks on :8000 (testing/chopsticks-e2e.yml, wasm-override with the
 *     CURRENT runtime), Anvil on :8545, services built.
 *   - UPGRADE_WASM: path to a runtime wasm with a HIGHER spec_version and an
 *     unchanged event shape (build one by bumping spec_version and rebuilding).
 *
 * Usage: UPGRADE_WASM=/path/to/spec26.wasm node src/drill-rb5-upgrade.mjs
 */

import { readFileSync } from "node:fs";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { erc20Abi, vaultAbi } from "./abi.mjs";
import { alive, clearState, killMatching, logs, start, stopAll, waitFor } from "./daemons.mjs";
import { assert, assertEq, check, section, summarise } from "./harness.mjs";
import { keys, pub } from "./anvil.mjs";
import { deployStack } from "./deploy.mjs";

const CHOPSTICKS = process.env.CHOPSTICKS_WS ?? "ws://127.0.0.1:8000";
const upgradeWasmPath = process.env.UPGRADE_WASM;
if (!upgradeWasmPath) {
	console.error("UPGRADE_WASM is required — a runtime wasm with a higher spec_version");
	process.exit(2);
}

console.log("RB-5 drill — runtime upgrade under a live fleet");
const { vault, pen } = deployStack();
console.log(`  vault ${vault}`);
const read = (fn, args) => pub.readContract({ address: vault, abi: vaultAbi, functionName: fn, args });

const api = await ApiPromise.create({ provider: new WsProvider(CHOPSTICKS), noInitWarn: true });
await cryptoWaitReady();
const keyring = new Keyring({ type: "sr25519", ss58Format: api.registry.chainSS58 ?? 56 });
const alice = keyring.addFromUri("//Alice");
const MIN = BigInt(api.consts.tokenMigration.minimumMigrationAmount.toString());

const newBlock = () => api.rpc("dev_newBlock");
async function fund(who, amount) {
	await api.rpc("dev_setStorage", { System: { Account: [[[who], { providers: 1, data: { free: amount.toString() } }]] } });
	await newBlock();
}
await api.rpc("dev_setStorage", [[api.query.tokenMigration.paused.key(), "0x00"]]);
await newBlock();

async function migrate(amount, baseAddress) {
	await fund(alice.address, MIN * 200n);
	await api.tx.tokenMigration.migrate(amount, baseAddress).signAndSend(alice);
	await newBlock();
	const events = await api.query.system.events();
	const ev = events.map((r) => r.event).find((e) => e.section === "tokenMigration" && e.method === "MigrationInitiated");
	assert(ev, "no MigrationInitiated event");
	return { nonce: BigInt(ev.data[0].toString()) };
}

killMatching(["dist/main.js"]);
clearState(["attestor/cp1.json", "attestor/cp2.json", "attestor/cp3.json", "attestor/cp4.json"]);
const baseEnv = { BASE_RPC_URL: "http://127.0.0.1:8545", VAULT_ADDRESS: vault, BASE_CHAIN_ID: "31337", POLL_INTERVAL_MS: "2000" };
const head = (await api.query.system.number()).toString();
for (let i = 0; i < 4; i++) {
	start(`attestor${i + 1}`, "attestor", {
		...baseEnv, PENDULUM_WS: CHOPSTICKS,
		ATTESTOR_PRIVATE_KEY: keys[i + 1], CHECKPOINT_FILE: `./cp${i + 1}.json`, START_BLOCK: head,
	});
}

const recipient = "0x000000000000000000000000000000000000beef";
const versionBefore = api.runtimeVersion.specVersion.toNumber();

section(`Upgrade under fire (starting at spec ${versionBefore})`);

await check("the fleet releases normally before the upgrade", async () => {
	const { nonce } = await migrate(MIN * 2n, recipient);
	await waitFor(async () => (await read("nonceConsumed", [nonce])) === true,
		{ timeoutMs: 90_000, label: "pre-upgrade release" });
});

await check("the runtime upgrade applies (spec_version increases)", async () => {
	const code = `0x${readFileSync(upgradeWasmPath).toString("hex")}`;
	// Writing :code is what an enacted upgrade does; the next block runs it.
	await api.rpc("dev_setStorage", [["0x3a636f6465", code]]);
	await newBlock();
	await newBlock();
	const version = await api.rpc.state.getRuntimeVersion();
	const after = version.specVersion.toNumber();
	assert(after > versionBefore, `spec_version did not increase: ${versionBefore} -> ${after}`);
	return;
});

await check("attestors keep decoding and releasing across the upgrade", async () => {
	const { nonce } = await migrate(MIN * 3n, recipient);
	try {
		await waitFor(async () => (await read("nonceConsumed", [nonce])) === true,
			{ timeoutMs: 90_000, label: "post-upgrade release" });
	} catch (e) {
		throw new Error(`${e.message}\n--- attestor1 ---\n${logs("attestor1")}`);
	}
});

await check("no attestor died across the upgrade", () => {
	for (let i = 1; i <= 4; i++) {
		assert(alive(`attestor${i}`), `attestor${i} exited:\n${logs(`attestor${i}`)}`);
	}
});

stopAll();
await api.disconnect();
process.exit(summarise() ? 0 : 1);
