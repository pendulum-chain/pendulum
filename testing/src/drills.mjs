/**
 * Phase 6 — failure drills (runbooks RB-1, RB-3, RB-4, RB-6, RB-7).
 *
 * Rehearses each runbook once against the full Sepolia stack, so the first
 * time anyone runs them is not during an incident. RB-2 (attestor outage) is
 * already exercised by the phase 5 rehearsal; RB-5 (runtime upgrade under a
 * live fleet) runs separately on Chopsticks, where a runtime can actually be
 * swapped (see drill-rb5-upgrade.mjs).
 *
 * Two Sepolia constraints shape the drills:
 *  - A conservation DEFICIT cannot be created here: nothing can move tokens
 *    out of the vault except the vault itself, which is the security property.
 *    The deficit alarm + auto-pause is covered by phase 3 on Anvil, where the
 *    vault can be impersonated. This phase covers the SURPLUS side, which
 *    needs no impersonation: migrate to an address we control and send PEN in.
 *  - Time cannot be warped, so the contracts are deployed with a sweep floor
 *    only ~90s out; by the time the RB-7 drill runs at the end it has passed.
 *    The drills never REDUCE the threshold, so `thresholdReducedAt` stays 0
 *    and the 7-day settling gate does not bite.
 *
 * Usage: node src/drills.mjs [--attach] [--keep]
 */

import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { erc20Abi, vaultAbi } from "./abi.mjs";
import { alive, clearState, logs, start, stopAll, stopAndWait, waitFor } from "./daemons.mjs";
import { assert, assertEq, check, section, summarise } from "./harness.mjs";
import {
	assertTestnet, buildContext, CONVERSION_FACTOR, loadEnv, PEN_12, ROOT, send, TESTING,
} from "./rehearsal-env.mjs";
import { deployToSepolia } from "./rehearsal-deploy.mjs";
import { discoverCollator, generateSpec, killMatching, sleep, spawnNetwork, teardown, waitForFinality } from "./zombienet.mjs";

const flags = new Set(process.argv.slice(2));
const ATTACH = flags.has("--attach");
const KEEP = flags.has("--keep");

const started = new Date();
const stamp = `${started.toISOString().replace(/[:.]/g, "-")}-drills`;
const runDir = path.join(TESTING, ".rehearsal", stamp);
const log = (m) => console.log(`  ${m}`);

let network = null;
let api = null;

async function main() {
	console.log(`Phase 6 — failure drills (Zombienet + Base Sepolia)\n  run ${stamp}`);
	const env = loadEnv();
	// Short sweep floor so RB-7 is reachable in wall clock; see header.
	env.REHEARSAL_SWEEP_OFFSET_SECONDS = process.env.DRILL_SWEEP_OFFSET ?? "90";
	const ctx = buildContext(env);
	await assertTestnet(ctx, null);

	// --- Substrate side -----------------------------------------------------
	section("Local Pendulum");
	if (!ATTACH) {
		teardown(log);
		generateSpec(log);
		network = spawnNetwork(log);
	}
	api = await discoverCollator(ApiPromise, WsProvider, { log });
	await waitForFinality(api, { log });
	await assertTestnet(ctx, api);
	await cryptoWaitReady();
	const keyring = new Keyring({ type: "sr25519", ss58Format: api.registry.chainSS58 ?? 42 });
	const alice = keyring.addFromUri("//Alice");
	const MIN = BigInt(api.consts.tokenMigration.minimumMigrationAmount.toString());

	/** Drive the pallet's pause origin the way mainnet governance will: through
	 *  the technical committee (this chain has no sudo; the generated spec seats
	 *  a single member so threshold 1 executes immediately). */
	async function committeeSetPaused(paused) {
		const call = api.tx.tokenMigration.setPaused(paused);
		const collective = api.tx.technicalCommittee ?? api.tx.council;
		await new Promise((resolve, reject) => {
			collective.propose(1, call, call.method.encodedLength)
				.signAndSend(alice, ({ status, dispatchError }) => {
					if (dispatchError) return reject(new Error(dispatchError.toString()));
					if (status.isInBlock) resolve();
				})
				.catch(reject);
		});
		await waitFor(async () => (await api.query.tokenMigration.paused()).isTrue === paused,
			{ timeoutMs: 60_000, label: `pallet paused=${paused} to be visible` });
	}
	await committeeSetPaused(false);
	log(`pallet unpaused; minimum migration ${MIN / PEN_12} PEN`);

	// --- Base side ----------------------------------------------------------
	section("Base Sepolia");
	const { vault, pen, sweepTs } = deployToSepolia({ env, roles: ctx.roles, log });
	log(`vault ${vault} (sweep floor ${new Date(sweepTs * 1000).toISOString()})`);
	const V = { address: vault, abi: vaultAbi };
	const P = { address: pen, abi: erc20Abi };

	/** Read that rides out endpoint throttling (same rationale as phase 5). */
	async function read(c, fn, args) {
		let last;
		for (let attempt = 0; attempt < 5; attempt++) {
			try {
				return await ctx.pub.readContract({ ...c, functionName: fn, args });
			} catch (error) {
				last = error;
				const text = `${error?.details ?? ""} ${error?.shortMessage ?? ""} ${error?.message ?? ""}`;
				if (!/rate limit|too many requests|timeout|fetch failed|50[234]/i.test(text)) throw error;
				await sleep(3000 * (attempt + 1));
			}
		}
		throw last;
	}
	const eventually = (fn, label, timeoutMs = 120_000) =>
		waitFor(async () => { try { await fn(); return true; } catch { return false; } },
			{ timeoutMs, intervalMs: 3000, label });

	await waitFor(
		async () => (await read(V, "pendingAdmin", [])).toLowerCase() === ctx.roles.admin.address.toLowerCase(),
		{ timeoutMs: 120_000, intervalMs: 3000, label: "pendingAdmin to be visible" });
	await send(ctx, ctx.roles.admin, { ...V, functionName: "acceptAdmin", args: [] });
	await eventually(
		async () => assertEq((await read(V, "admin", [])).toLowerCase(), ctx.roles.admin.address.toLowerCase(), "admin"),
		"admin handover to settle");
	log("admin handover complete");

	// --- the fleet ----------------------------------------------------------
	section("Attestor fleet");
	killMatching(["dist/main.js"]);
	clearState(["attestor/cp1.json", "attestor/cp2.json", "attestor/cp3.json", "attestor/cp4.json",
		"releaser/releaser-state.json"]);
	const pendulumWs = api._options?.provider?.endpoint ?? process.env.PENDULUM_WS;
	const baseEnv = {
		BASE_RPC_URL: env.BASE_SEPOLIA_RPC_URL, VAULT_ADDRESS: vault,
		BASE_CHAIN_ID: "84532", POLL_INTERVAL_MS: "12000",
	};
	const pendulumHead = (await api.query.system.number()).toString();
	const baseHead = String(await ctx.pub.getBlockNumber());
	const attestorEnv = (i) => ({
		...baseEnv, PENDULUM_WS: pendulumWs,
		ATTESTOR_PRIVATE_KEY: env[`ATTESTOR_${i}_PRIVATE_KEY`],
		CHECKPOINT_FILE: `./cp${i}.json`, START_BLOCK: pendulumHead,
	});
	for (let i = 1; i <= 4; i++) {
		start(`attestor${i}`, "attestor", attestorEnv(i));
		await sleep(3000);
	}
	start("monitor", "monitor", { ...baseEnv, PENDULUM_WS: pendulumWs, GRACE_SECONDS: "300" });
	start("releaser", "releaser", { ...baseEnv, RELEASER_PRIVATE_KEY: env.RELEASER_PRIVATE_KEY, START_BLOCK: baseHead });
	log(`fleet up (Pendulum from #${pendulumHead}, Base from #${baseHead})`);

	// PEN lands at an address we control, so RB-3 can push a surplus back in.
	const recipient = ctx.roles.admin.address;

	/** Burn on the local chain; returns {nonce, amount, atBlock}. */
	async function migrate(amountPen) {
		const amount = amountPen * PEN_12;
		const atBlock = Number((await api.query.system.number()).toString());
		return new Promise((resolve, reject) => {
			api.tx.tokenMigration.migrate(amount, recipient)
				.signAndSend(alice, ({ status, events, dispatchError }) => {
					if (dispatchError) {
						const decoded = dispatchError.isModule
							? api.registry.findMetaError(dispatchError.asModule).name
							: dispatchError.toString();
						return reject(new Error(`migrate failed: ${decoded}`));
					}
					if (!status.isInBlock) return;
					const ev = events.map((r) => r.event)
						.find((e) => e.section === "tokenMigration" && e.method === "MigrationInitiated");
					if (!ev) return reject(new Error("no MigrationInitiated event"));
					resolve({
						nonce: BigInt(ev.data[0].toString()),
						amount: BigInt(ev.data[3].toString().replaceAll(",", "")),
						atBlock,
					});
				})
				.catch(reject);
		});
	}
	const payloadOf = (m) => read(V, "payloadHash", [m.nonce, recipient, m.amount]);
	const released = (m) => read(V, "nonceConsumed", [m.nonce]);
	const waitReleased = (m, label) =>
		waitFor(async () => (await released(m)) === true, { timeoutMs: 240_000, intervalMs: 5000, label });

	// --- drills -------------------------------------------------------------
	section("Baseline");
	await check("the pipeline works before we start breaking it", async () => {
		const m = await migrate(200n);
		await waitReleased(m, "baseline release");
		assert((await read(P, "balanceOf", [recipient])) >= m.amount * CONVERSION_FACTOR, "recipient balance");
	});

	section("RB-1 — attestor removal retroactively invalidates its approvals");
	let held; // the migration parked below quorum, completed in the RB-6 drill
	let heldPayload;
	await check("a removed attestor's recorded approval stops counting", async () => {
		// Premise: attestor1 is offline, so exactly att2..att4 (threshold) vote.
		await stopAndWait("attestor1");
		// Pause the vault: approvals record, releases wait. This parks the
		// migration at exactly-threshold so removing one voter drops it below.
		await send(ctx, ctx.roles.guardian, { ...V, functionName: "pause", args: [] });
		held = await migrate(150n);
		heldPayload = await payloadOf(held);
		await waitFor(async () => (await read(V, "activeApprovals", [heldPayload])) >= 3n,
			{ timeoutMs: 240_000, intervalMs: 5000, label: "three approvals to be recorded while paused" });
		await eventually(async () => assertEq(await read(V, "pendingRelease", [heldPayload]), true, "pendingRelease"),
			"the pending mark to be visible");

		// The drill: remove attestor4 (RB-1's compromised key).
		await send(ctx, ctx.roles.admin, { ...V, functionName: "removeAttestor", args: [ctx.roles.attestors[3].address] });
		await eventually(async () => assertEq(await read(V, "activeApprovals", [heldPayload]), 2n, "activeApprovals"),
			"the removal to invalidate the recorded approval");
		assertEq(await read(V, "hasApproved", [heldPayload, ctx.roles.attestors[3].address]), false,
			"hasApproved for the removed attestor");
	});

	await check("below quorum, the release cannot be completed by anyone", async () => {
		await send(ctx, ctx.roles.admin, { ...V, functionName: "unpause", args: [] });
		// The releaser daemon is live and retrying; give it real time to try.
		await sleep(30_000);
		assertEq(await released(held), false, "nonceConsumed with only 2 active approvals");
	});

	section("RB-6 — a re-added attestor must approve again");
	await check("re-adding bumps the generation: the old approval stays dead", async () => {
		await send(ctx, ctx.roles.admin, { ...V, functionName: "addAttestor", args: [ctx.roles.attestors[3].address] });
		await eventually(async () => assertEq(await read(V, "isAttestor", [ctx.roles.attestors[3].address]), true, "isAttestor"),
			"the re-add to settle");
		// Same address, same recorded vote — but a new generation, so it counts
		// for nothing until the attestor signs again.
		assertEq(await read(V, "activeApprovals", [heldPayload]), 2n, "activeApprovals after re-add");
	});

	await check("the documented recovery completes the quorum: rewind and re-approve", async () => {
		// RB-6's operator step: restart the re-added attestor with its checkpoint
		// rewound to before the affected migrations, so it re-scans and re-signs.
		await stopAndWait("attestor4");
		writeFileSync(path.join(ROOT, "attestor/cp4.json"),
			JSON.stringify({ lastProcessedBlock: held.atBlock - 1 }));
		start("attestor4", "attestor", attestorEnv(4));
		await waitReleased(held, "the held migration to release after re-approval");
		assertEq(await read(V, "activeApprovals", [heldPayload]), 3n, "final active approvals");
	});

	await check("a fifth attestor can be added and removed cleanly", async () => {
		const fifth = privateKeyToAccount(generatePrivateKey()).address;
		await send(ctx, ctx.roles.admin, { ...V, functionName: "addAttestor", args: [fifth] });
		await eventually(async () => assertEq(await read(V, "attestorCount", []), 5n, "attestorCount"), "count to reach 5");
		await send(ctx, ctx.roles.admin, { ...V, functionName: "removeAttestor", args: [fifth] });
		await eventually(async () => assertEq(await read(V, "attestorCount", []), 4n, "attestorCount"), "count back to 4");
		// Bring the standby back for the remaining drills.
		start("attestor1", "attestor", attestorEnv(1));
		await sleep(5000);
		assert(alive("attestor1"), `attestor1 did not come back:\n${logs("attestor1")}`);
	});

	section("RB-3 — the monitor tolerates a surplus (deficit is covered on Anvil)");
	await check("PEN sent into the vault raises no alarm and kills nothing", async () => {
		// A deficit cannot be created on Sepolia — nothing but the vault itself
		// can move its tokens, which is the security property. Phase 3 covers
		// the deficit alarm via Anvil impersonation. Here: the benign inverse.
		const monitorLogBefore = logs("monitor").length;
		await send(ctx, ctx.roles.admin, { ...P, functionName: "transfer", args: [vault, 10n * 10n ** 18n] });
		await sleep(40_000); // > two monitor poll cycles
		assert(alive("monitor"), `monitor died on a surplus:\n${logs("monitor")}`);
		const fresh = logs("monitor").slice(monitorLogBefore);
		assert(!/DEFICIT/i.test(fresh), `monitor raised a deficit alert on a surplus:\n${fresh}`);
	});

	section("RB-4 — coordinated stop, resumed in reverse order");
	await check("pause pallet then vault; new burns fail at the source", async () => {
		await committeeSetPaused(true);
		let failed = false;
		try { await migrate(100n); } catch (e) { failed = /MigrationsPaused/.test(e.message); }
		assert(failed, "migrate should fail MigrationsPaused while the pallet is paused");
		await send(ctx, ctx.roles.guardian, { ...V, functionName: "pause", args: [] });
		await eventually(async () => assertEq(await read(V, "paused", []), true, "vault paused"), "vault pause to settle");
	});

	await check("resume in reverse: vault first, then pallet; the pipeline recovers", async () => {
		await send(ctx, ctx.roles.admin, { ...V, functionName: "unpause", args: [] });
		await committeeSetPaused(false);
		const m = await migrate(120n);
		await waitReleased(m, "post-resume release");
	});

	section("RB-7 — window close, reconcile, sweep");
	await check("close the window: pause the pallet, confirm nothing is in flight", async () => {
		await committeeSetPaused(true);
		await eventually(async () => assertEq(await read(V, "pendingApprovedAmount", []), 0n, "pendingApprovedAmount"),
			"all quorum-approved releases to have settled");
	});

	await check("reconcile: conservation holds exactly at the close", async () => {
		const [balance, rel, swept, supply] = await Promise.all([
			read(P, "balanceOf", [vault]), read(V, "totalReleased", []),
			read(V, "totalSwept", []), read(P, "totalSupply", []),
		]);
		// The RB-3 surplus sits in the vault's balance, ON TOP of conservation.
		const surplus = 10n * 10n ** 18n;
		assertEq(balance + rel + swept, supply + surplus, "balance + released + swept vs supply (+known surplus)");
	});

	await check("the sweep executes and the monitor does not false-alarm", async () => {
		assert(Math.floor(Date.now() / 1000) >= sweepTs, "sweep floor not yet passed — raise DRILL_SWEEP_OFFSET");
		assertEq(await read(V, "thresholdReducedAt", []), 0n, "thresholdReducedAt (settling gate must not be armed)");
		const monitorLogBefore = logs("monitor").length;
		const sweepAmount = 1000n * 10n ** 18n;
		const sweptBefore = await read(V, "totalSwept", []);
		await send(ctx, ctx.roles.admin, { ...V, functionName: "sweepRemainder", args: [recipient, sweepAmount] });
		await eventually(async () => assertEq(await read(V, "totalSwept", []), sweptBefore + sweepAmount, "totalSwept"),
			"the sweep to settle");
		await sleep(40_000);
		assert(alive("monitor"), `monitor died after the sweep:\n${logs("monitor")}`);
		const fresh = logs("monitor").slice(monitorLogBefore);
		assert(!/DEFICIT/i.test(fresh), `monitor false-alarmed on a swept balance:\n${fresh}`);
	});

	await check("every daemon survived every drill", () => {
		for (const name of ["attestor1", "attestor2", "attestor3", "attestor4", "monitor", "releaser"]) {
			assert(alive(name), `${name} is dead:\n${logs(name)}`);
		}
	});

	// --- record + teardown ----------------------------------------------------
	mkdirSync(runDir, { recursive: true });
	writeFileSync(path.join(runDir, "manifest.json"), JSON.stringify({
		kind: "phase6-drills",
		startedAt: started.toISOString(),
		commit: execSync("git rev-parse HEAD", { cwd: ROOT, encoding: "utf8" }).trim(),
		vault, pen, sweepTimestamp: sweepTs,
		covered: ["RB-1", "RB-3 (surplus half)", "RB-4", "RB-6", "RB-7"],
		coveredElsewhere: { "RB-2": "phase 5 rehearsal", "RB-3 deficit": "phase 3 (Anvil impersonation)", "RB-5": "drill-rb5-upgrade.mjs on Chopsticks" },
	}, null, 2));
	for (const name of ["attestor1", "attestor2", "attestor3", "attestor4", "monitor", "releaser"]) {
		writeFileSync(path.join(runDir, `${name}.log`), logs(name));
	}
	if (network) writeFileSync(path.join(runDir, "zombienet.log"), network.out.join("\n"));
	console.log(`\n  run artifacts: ${path.relative(ROOT, runDir)}`);

	const ok = summarise();
	if (KEEP) { console.log("\n  --keep: leaving everything running."); process.exit(ok ? 0 : 1); }
	stopAll();
	await sleep(2000);
	if (!ATTACH) teardown(log);
	if (api) await api.disconnect().catch(() => {});
	process.exit(ok ? 0 : 1);
}

process.on("SIGINT", () => { stopAll(); if (!KEEP) teardown(console.log); process.exit(130); });

main().catch(async (error) => {
	console.error(`\ndrills aborted: ${error.message}`);
	stopAll();
	if (!KEEP && !ATTACH) teardown(console.log);
	process.exit(1);
});
