/**
 * Phase 3 — the whole pipeline running together.
 *
 * A migration burned on the Substrate side must appear on Base without any
 * manual step, driven by four independent attestor processes, an invariant
 * monitor and the releaser. Every component is unit-tested in isolation; this
 * is the only place they run as a system, which is where the remaining risk
 * lives.
 *
 * Chopsticks stands in for Pendulum here rather than Zombienet: it reports
 * finalized heads, which is what the attestors subscribe to, and it carries
 * real mainnet state. What it does NOT reproduce is genuine relay-chain
 * finality timing -- lag, and the possibility of a fork before finality. That
 * remains a manual Zombienet exercise; see the README.
 *
 * Prerequisites: Chopsticks on :8000 (fresh), Anvil on :8545, and the services
 * built (`npm run build` in attestor/, monitor/ and releaser/).
 */

import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { assert, assertEq, check, section, summarise } from "./harness.mjs";
import { accounts, attestors, keys, pub, releaser as releaserAcct, RPC, send } from "./anvil.mjs";
import { erc20Abi, vaultAbi } from "./abi.mjs";
import { deployStack, PARAMS } from "./deploy.mjs";
import { alive, clearState, killStrays, logs, start, stopAll, stopAndWait, waitFor } from "./daemons.mjs";

const CHOPSTICKS = process.env.CHOPSTICKS_WS ?? "ws://127.0.0.1:8000";
const CF = PARAMS.CONVERSION_FACTOR;

console.log("Phase 3 — end-to-end pipeline");
const { vault, pen } = deployStack();
console.log(`  vault ${vault}\n  PEN   ${pen}`);
const V = { address: vault, abi: vaultAbi };
const P = { address: pen, abi: erc20Abi };
const read = (c, fn, args) => pub.readContract({ ...c, functionName: fn, args });

// Accept admin so caps can be tuned during the run.
const admin = accounts[6];
await send(admin, { ...V, functionName: "acceptAdmin", args: [] });

const api = await ApiPromise.create({ provider: new WsProvider(CHOPSTICKS), noInitWarn: true });
await cryptoWaitReady();
const keyring = new Keyring({ type: "sr25519", ss58Format: api.registry.chainSS58 ?? 56 });
const alice = keyring.addFromUri("//Alice");
const MIN = BigInt(api.consts.tokenMigration.minimumMigrationAmount.toString());

const newBlock = () => api.rpc("dev_newBlock");
async function setStorage(v) { await api.rpc("dev_setStorage", v); await newBlock(); }
async function fund(who, amount) {
	await setStorage({ System: { Account: [[[who], { providers: 1, data: { free: amount.toString() } }]] } });
}

// Enable migrations (stands in for the governance unpause).
await api.rpc("dev_setStorage", [[api.query.tokenMigration.paused.key(), "0x00"]]);
await newBlock();

/** Burn on Pendulum and return the emitted migration details. */
async function migrate(amount, baseAddress) {
	await fund(alice.address, MIN * 200n);
	await api.tx.tokenMigration.migrate(amount, baseAddress).signAndSend(alice);
	await newBlock();
	const events = await api.query.system.events();
	const ev = events.map((r) => r.event).find((e) => e.section === "tokenMigration" && e.method === "MigrationInitiated");
	assert(ev, "no MigrationInitiated event — the burn did not happen");
	return { nonce: BigInt(ev.data[0].toString()), amount: BigInt(ev.data[3].toString().replaceAll(",", "")) };
}

// --- start the fleet -------------------------------------------------------
killStrays();
clearState(["attestor/cp1.json", "attestor/cp2.json", "attestor/cp3.json", "attestor/cp4.json",
            "releaser/releaser-state.json"]);
const baseEnv = { BASE_RPC_URL: RPC, VAULT_ADDRESS: vault, BASE_CHAIN_ID: "31337", POLL_INTERVAL_MS: "2000" };
const startBlock = String(await pub.getBlockNumber());
// Where the attestors begin scanning Pendulum. Must be the current head: a
// fork sits at ~7.4M blocks, and starting from 0 would have each daemon walk
// every historical block before reaching anything under test.
const pendulumHead = (await api.query.system.number()).toString();
console.log(`  attestors scan Pendulum from block ${pendulumHead}`);

for (let i = 0; i < 4; i++) {
	start(`attestor${i + 1}`, "attestor", {
		...baseEnv, PENDULUM_WS: CHOPSTICKS,
		ATTESTOR_PRIVATE_KEY: keys[i + 1], CHECKPOINT_FILE: `./cp${i + 1}.json`, START_BLOCK: pendulumHead,
	});
}
start("monitor", "monitor", { ...baseEnv, PENDULUM_WS: CHOPSTICKS, GRACE_SECONDS: "30" });
start("releaser", "releaser", { ...baseEnv, RELEASER_PRIVATE_KEY: keys[7], START_BLOCK: startBlock });

const recipient = "0x000000000000000000000000000000000000beef";
const balanceOf = (who) => read(P, "balanceOf", [who]);

section("The pipeline end to end");

await check("a burn on Pendulum arrives on Base with no manual step", async () => {
	const before = await balanceOf(recipient);
	const { nonce, amount } = await migrate(MIN * 2n, recipient);
	try {
		await waitFor(async () => (await read(V, "nonceConsumed", [nonce])) === true,
			{ timeoutMs: 90_000, label: "the release to land on Base" });
	} catch (e) {
		// A silent pipeline is the hardest thing to debug from the outside, so
		// surface what each daemon actually saw.
		throw new Error(`${e.message}\n--- attestor1 ---\n${logs("attestor1")}\n--- monitor ---\n${logs("monitor")}`);
	}
	assertEq(await balanceOf(recipient), before + amount * CF, "released amount");
});

await check("losing the approval race is benign — no attestor exits", async () => {
	// Three approvals release; the fourth attestor's submission necessarily
	// reverts. That is the normal case, and it must not kill the process.
	for (let i = 1; i <= 4; i++) {
		assert(alive(`attestor${i}`), `attestor${i} exited:\n${logs(`attestor${i}`)}`);
	}
});

section("Resilience");

await check("one attestor down still releases (3 of 4)", async () => {
	await stopAndWait("attestor4");
	const { nonce } = await migrate(MIN * 2n, recipient);
	await waitFor(async () => (await read(V, "nonceConsumed", [nonce])) === true,
		{ timeoutMs: 90_000, label: "release with 3 attestors" });
});

await check("two attestors down stops releases cleanly, losing nothing", async () => {
	await stopAndWait("attestor3");
	const { nonce } = await migrate(MIN * 2n, recipient);
	await new Promise((r) => setTimeout(r, 20_000));
	assertEq(await read(V, "nonceConsumed", [nonce]), false, "should not release below quorum");

	// Restarting a third attestor drains the backlog without re-burning.
	start("attestor3b", "attestor", {
		...baseEnv, PENDULUM_WS: CHOPSTICKS,
		ATTESTOR_PRIVATE_KEY: keys[3], CHECKPOINT_FILE: "./cp3.json", START_BLOCK: pendulumHead,
	});
	await waitFor(async () => (await read(V, "nonceConsumed", [nonce])) === true,
		{ timeoutMs: 120_000, label: "the backlog to drain after recovery" });
});

section("Cap deferral and the releaser");

await check("a cap-deferred release is drained by the releaser, unattended", async () => {
	// Squeeze the daily budget so the next migration cannot release immediately.
	const tiny = MIN * CF; // one minimum-sized migration's worth
	await send(admin, { ...V, functionName: "setCaps", args: [tiny * 10n, tiny] });
	// Earlier checks in this run consumed the (much larger) original budget.
	// The bucket refills proportionally to the CURRENT dailyCap, so after
	// lowering it the old consumption decays slowly -- warp past it, or the
	// first release below has no budget at all.
	await pub.request({ method: "evm_increaseTime", params: [30 * 24 * 3600] });
	await pub.request({ method: "evm_mine", params: [] });

	const first = await migrate(MIN, recipient);
	await waitFor(async () => (await read(V, "nonceConsumed", [first.nonce])) === true,
		{ timeoutMs: 90_000, label: "the first release to consume the budget" });

	const second = await migrate(MIN, recipient);
	await waitFor(async () => (await read(V, "pendingApprovedAmount")) > 0n,
		{ timeoutMs: 90_000, label: "the second release to be deferred" });
	assertEq(await read(V, "nonceConsumed", [second.nonce]), false, "should be deferred, not released");

	// No manual release(): the releaser must pick it up as the bucket refills.
	await pub.request({ method: "evm_increaseTime", params: [25 * 3600] });
	await pub.request({ method: "evm_mine", params: [] });
	try {
		await waitFor(async () => (await read(V, "nonceConsumed", [second.nonce])) === true,
			{ timeoutMs: 120_000, label: "the releaser to drain the deferred release" });
	} catch (e) {
		throw new Error(`${e.message}\n--- releaser ---\n${logs("releaser")}`);
	}
	assertEq(await read(V, "pendingApprovedAmount"), 0n, "pending accounting cleared");
	assert(alive("releaser"), `releaser exited:\n${logs("releaser")}`);
});

section("Invariants");

await check("the monitor reports healthy and stays running", async () => {
	assert(alive("monitor"), `monitor exited:\n${logs("monitor")}`);
	await waitFor(() => logs("monitor").includes("ok:"), { timeoutMs: 60_000, label: "a monitor ok line" });
	const bad = logs("monitor").match(/ALERT: (CONSERVATION VIOLATION|VAULT BALANCE MISMATCH)/);
	assert(!bad, `monitor raised a conservation alert:\n${logs("monitor")}`);
});

await check("conservation holds across the whole run", async () => {
	const [bal, released, swept, supply] = await Promise.all([
		balanceOf(vault), read(V, "totalReleased"), read(V, "totalSwept"), read(P, "totalSupply"),
	]);
	assertEq(bal + released + swept, supply, "conservation identity");
});

const ok = summarise();
stopAll();
await api.disconnect();
process.exit(ok ? 0 : 1);
