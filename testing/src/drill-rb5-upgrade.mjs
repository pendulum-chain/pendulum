/**
 * RB-5 drill — THE runtime upgrade lands while the attestor fleet is running.
 *
 * This drills the actual production upgrade, not a stand-in: a Chopsticks fork
 * of live mainnet state (spec 25, no token-migration pallet) has the spec-26
 * wasm written to `:code` — which is exactly what the enacted referendum does —
 * while the attestor fleet is already running. Before the upgrade there is
 * nothing to decode; after it, the pallet exists, migrations flow, and the
 * fleet must decode and release without a restart. The shape-change half of
 * RB-5 — fail loudly rather than skip — is the attestor's unit-tested 4-field
 * assertion.
 *
 * This cannot run on Zombienet or Sepolia: swapping a runtime needs root (no
 * sudo on Pendulum) or storage access. Two Chopsticks quirks shape the script:
 *
 *   - It must fork WITHOUT --wasm-override: an override pins the executing
 *     runtime, so a `:code` write beneath it splits execution from reporting.
 *   - Even without an override, Chopsticks serves metadata and runtime-version
 *     RPCs from a runtime cached at fork time, and keeps doing so across
 *     `--resume` restarts. After the `:code` write the EXECUTING runtime
 *     really is the new one — frame-system records lastRuntimeUpgrade = the
 *     new spec — but the RPC layer keeps reporting the old, and an extrinsic
 *     built against that stale metadata is rejected by the real runtime as
 *     `badProof`. Consequence: a post-upgrade `migrate` cannot be SUBMITTED
 *     through Chopsticks at all, so this drill asserts the operational RB-5
 *     property instead — every attestor keeps processing post-upgrade blocks
 *     through its normal decode path (checkpoints advance past the upgrade
 *     block; a decode failure would exit the daemon by design) and rides a
 *     full node restart. Decoding actual MigrationInitiated events under the
 *     post-upgrade runtime is exercised wholesale by phases 2 and 3, which run
 *     that runtime via wasm-override; the changed-shape half is the attestor's
 *     unit-tested 4-field assertion.
 *
 * Prerequisites: Anvil on :8545, services built, and UPGRADE_WASM pointing at
 * the spec-bumped runtime (bump spec_version, rebuild). Chopsticks is spawned
 * and restarted by the drill itself.
 *
 * Usage: UPGRADE_WASM=/path/to/spec26.wasm node src/drill-rb5-upgrade.mjs
 */

import { spawn } from "node:child_process";
import { readFileSync, rmSync } from "node:fs";
import path from "node:path";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { alive, clearState, logs, start, stopAll, waitFor } from "./daemons.mjs";
import { killMatching } from "./zombienet.mjs";
import { assert, assertEq, check, section, summarise } from "./harness.mjs";
import { keys } from "./anvil.mjs";
import { deployStack } from "./deploy.mjs";

const CHOPSTICKS = process.env.CHOPSTICKS_WS ?? "ws://127.0.0.1:8000";
const upgradeWasmPath = process.env.UPGRADE_WASM;
if (!upgradeWasmPath) {
	console.error("UPGRADE_WASM is required — a runtime wasm with a higher spec_version");
	process.exit(2);
}

const ROOT = path.resolve(new URL(".", import.meta.url).pathname, "../..");
let chopsticks = null;

/** Spawn Chopsticks on the e2e config (db-backed, NO wasm-override) and wait
 *  until it answers. `fresh` wipes the db for a clean fork. */
async function startChopsticks({ fresh, resumeAt, override }) {
	if (fresh) {
		for (const f of [".chopsticks-e2e.sqlite", ".chopsticks-e2e.sqlite-shm", ".chopsticks-e2e.sqlite-wal"]) {
			try { rmSync(path.join(ROOT, "testing", f)); } catch {}
		}
	}
	// Without --resume, a restart re-forks at the remote chain's CURRENT head
	// and the locally-built blocks — including the enacted upgrade — are
	// silently discarded. --resume continues from a block in the db. Note the
	// CLI form must be a block number or hash: a literal `--resume true`
	// arrives as the string "true" and fails config validation.
	const args = ["@acala-network/chopsticks@latest", "--config", "testing/chopsticks-e2e.yml"];
	if (!fresh) args.push("--resume", String(resumeAt));
	if (override) args.push("--wasm-override", override);
	chopsticks = spawn("npx", args, {
		cwd: ROOT, stdio: ["ignore", "pipe", "pipe"],
	});
	const out = [];
	chopsticks.stdout.on("data", (c) => out.push(String(c)));
	chopsticks.stderr.on("data", (c) => out.push(String(c)));
	try {
		await waitFor(() => out.join("").includes("listening on"),
			{ timeoutMs: 240_000, intervalMs: 2000, label: "chopsticks to come up" });
	} catch (error) {
		// Surface what the node actually said — a silent timeout is undebuggable.
		throw new Error(`${error.message}\n--- chopsticks output (tail) ---\n${out.join("").slice(-3000)}`);
	}
}

async function stopChopsticks() {
	if (!chopsticks) return;
	chopsticks.kill("SIGTERM");
	await new Promise((resolve) => {
		chopsticks.on("exit", resolve);
		setTimeout(() => { chopsticks.kill("SIGKILL"); resolve(); }, 10_000);
	});
	chopsticks = null;
}

console.log("starting Chopsticks (fresh mainnet fork, no wasm-override) ...");
await startChopsticks({ fresh: true });

console.log("RB-5 drill — runtime upgrade under a live fleet");
const { vault, pen } = deployStack();
console.log(`  vault ${vault}`);

let api = await ApiPromise.create({ provider: new WsProvider(CHOPSTICKS), noInitWarn: true });

const newBlock = () => api.rpc("dev_newBlock");

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

const versionBefore = api.runtimeVersion.specVersion.toNumber();
let upgradedHead;
let upgradedHeadNumber;

section(`The real upgrade, under a live fleet (mainnet fork at spec ${versionBefore})`);

await check("pre-upgrade: the pallet does not exist and the fleet idles happily", async () => {
	assert(!api.query.tokenMigration, "tokenMigration already present — this fork is not pre-upgrade mainnet");
	await newBlock();
	await newBlock();
	for (let i = 1; i <= 4; i++) assert(alive(`attestor${i}`), `attestor${i} died on pallet-less blocks:\n${logs(`attestor${i}`)}`);
});

await check("the referendum's upgrade applies: the executing runtime records it", async () => {
	const code = `0x${readFileSync(upgradeWasmPath).toString("hex")}`;
	// Writing :code is what the enacted referendum does; the next block runs it.
	await api.rpc("dev_setStorage", [["0x3a636f6465", code]]);
	await newBlock();
	await newBlock();
	// The strongest possible evidence the upgrade executed: frame-system itself
	// wrote its new version into storage. (Chopsticks' RPC layer still reports
	// the cached old runtime at this point — see the header.)
	const head = await api.rpc.chain.getFinalizedHead();
	// Keep the HASH: chopsticks' --resume validates its string form as a
	// 66-char block hash — a bare block number is rejected by the schema.
	upgradedHead = head.toHex();
	upgradedHeadNumber = (await api.rpc.chain.getHeader(head)).number.toNumber();
	const apiAt = await api.at(head);
	const lru = (await apiAt.query.system.lastRuntimeUpgrade()).toJSON();
	assert(lru && lru.specVersion > versionBefore,
		`lastRuntimeUpgrade did not advance: ${JSON.stringify(lru)}`);
});

await check("every attestor processes post-upgrade blocks through its decode path", async () => {
	// Produce post-upgrade blocks and require every checkpoint to move past
	// the upgrade block. Advancing means migrationEventsInBlock ran to
	// completion on blocks built by the NEW runtime — a decode failure is
	// fatal by design (PRD A5), so mere survival plus progress is the proof.
	for (let i = 0; i < 4; i++) await newBlock();
	const target = upgradedHeadNumber + 2;
	await waitFor(() => [1, 2, 3, 4].every((i) => {
		try {
			return JSON.parse(readFileSync(path.join(ROOT, `attestor/cp${i}.json`), "utf8")).lastProcessedBlock >= target;
		} catch { return false; }
	}), { timeoutMs: 120_000, intervalMs: 3000, label: `all checkpoints to pass block ${target}` });
});

await check("the fleet rides a full node restart on the upgraded chain", async () => {
	await api.disconnect();
	await stopChopsticks();
	await startChopsticks({ fresh: false, resumeAt: upgradedHead });
	api = await ApiPromise.create({ provider: new WsProvider(CHOPSTICKS), noInitWarn: true });
	// The daemons were never restarted; their WsProviders must reconnect and
	// resume the finalized-heads subscription on their own.
	const before = (await api.rpc.chain.getHeader()).number.toNumber();
	for (let i = 0; i < 3; i++) await newBlock();
	const target = before + 2;
	await waitFor(() => [1, 2, 3, 4].every((i) => {
		try {
			return JSON.parse(readFileSync(path.join(ROOT, `attestor/cp${i}.json`), "utf8")).lastProcessedBlock >= target;
		} catch { return false; }
	}), { timeoutMs: 180_000, intervalMs: 3000, label: `all checkpoints to pass block ${target} after the restart` });
});

await check("no attestor died across the upgrade", () => {
	for (let i = 1; i <= 4; i++) {
		assert(alive(`attestor${i}`), `attestor${i} exited:\n${logs(`attestor${i}`)}`);
	}
});

stopAll();
await api.disconnect();
await stopChopsticks();
process.exit(summarise() ? 0 : 1);
