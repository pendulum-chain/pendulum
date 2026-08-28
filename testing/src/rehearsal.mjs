/**
 * Phase 5 — full-stack rehearsal: local Zombienet Pendulum + Base Sepolia.
 *
 * Phases 1-4 each hold one half of the system still. This runs the whole thing
 * against real infrastructure on both sides at once: genuine relay-chain
 * finality on the Substrate side, and a public EVM on the Base side with real
 * gas estimation, real block times and real RPC behaviour. Both production
 * bugs found during this work (attestor gas under-estimation, releaser
 * Multicall) lived exactly there, and neither was reachable from unit tests.
 *
 * It is built to be re-run casually — every failure investigation should start
 * by spinning this up again — so it redeploys contracts each time, clears
 * daemon state, and refuses to run anywhere that could cost real money.
 *
 * Usage:
 *   node src/rehearsal.mjs                 full run, then tear down
 *   node src/rehearsal.mjs --preflight     check prerequisites and funding only
 *   node src/rehearsal.mjs --fund          fan gas out from the deployer to the other roles
 *   node src/rehearsal.mjs --keep          leave everything running afterwards
 *   node src/rehearsal.mjs --attach        use an already-running Zombienet
 *   node src/rehearsal.mjs --skip-slow     skip the wall-clock cap-refill test
 */

import { execSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { encodeAbiParameters, keccak256 } from "viem";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { erc20Abi, vaultAbi } from "./abi.mjs";
import { alive, clearState, logs, start, stopAll, stopAndWait, waitFor } from "./daemons.mjs";
import { assert, assertEq, check, section, summarise } from "./harness.mjs";
import {
	assertTestnet, buildContext, CONVERSION_FACTOR, loadEnv, PEN_12, PEN_18, ROOT, send, TESTING,
} from "./rehearsal-env.mjs";
import { deployToSepolia, rehearsalParams } from "./rehearsal-deploy.mjs";
import { discoverCollator, killMatching, generateSpec, sleep, spawnNetwork, teardown, waitForFinality } from "./zombienet.mjs";

const flags = new Set(process.argv.slice(2));
const FUND = flags.has("--fund");
const KEEP = flags.has("--keep");
const ATTACH = flags.has("--attach");
const SKIP_SLOW = flags.has("--skip-slow");
const PREFLIGHT_ONLY = flags.has("--preflight");

const started = new Date();
const stamp = started.toISOString().replace(/[:.]/g, "-");
const runDir = path.join(TESTING, ".rehearsal", stamp);
const log = (m) => console.log(`  ${m}`);

// Gas each role needs. Sized against measured cost, not guesswork: a whole run
// — two contract deployments plus ~20 approvals — comes to roughly 0.00005 ETH
// on Base Sepolia, so these carry about two orders of magnitude of headroom for
// gas spikes and the L1 data fee. Small enough that one faucet claim into the
// deployer covers many runs.
const FUNDING = {
	deployer: { min: 3_000_000_000_000_000n, target: 3_000_000_000_000_000n },  // 0.003 ETH, the source
	attestor: { min: 1_000_000_000_000_000n, target: 2_000_000_000_000_000n },  // 0.001 / 0.002
	other: { min: 500_000_000_000_000n, target: 1_000_000_000_000_000n },       // 0.0005 / 0.001
};

const eth = (wei) => `${(Number(wei) / 1e18).toFixed(6)} ETH`;

/** Every funded role, with what it needs and what `--fund` tops it up to. */
function roleTable(ctx) {
	return [
		["deployer", ctx.roles.deployer, FUNDING.deployer],
		...ctx.roles.attestors.map((a, i) => [`attestor${i + 1}`, a, FUNDING.attestor]),
		["guardian", ctx.roles.guardian, FUNDING.other],
		["admin", ctx.roles.admin, FUNDING.other],
		["releaser", ctx.roles.releaser, FUNDING.other],
	];
}

/**
 * Distribute gas from the deployer to the other roles.
 *
 * Base Sepolia faucets are rate-limited per address, so claiming for eight
 * addresses is tedious and slow. Claim once into the deployer and fan out from
 * here. Idempotent: only roles below their minimum are topped up, and only to
 * their target, so re-running after a few rehearsals costs nothing.
 */
async function fundRoles(ctx) {
	section("Funding");
	await assertTestnet(ctx, null);

	const deployerBalance = await ctx.pub.getBalance({ address: ctx.roles.deployer.address });
	const needy = [];
	for (const [name, acct, limits] of roleTable(ctx).slice(1)) {
		const balance = await ctx.pub.getBalance({ address: acct.address });
		if (balance < limits.min) needy.push({ name, acct, top: limits.target - balance });
	}

	if (needy.length === 0) {
		log(`every role is already funded; deployer holds ${eth(deployerBalance)}`);
		return true;
	}

	const total = needy.reduce((sum, n) => sum + n.top, 0n);
	// Leave the deployer enough to actually deploy after funding everyone else.
	const reserve = FUNDING.deployer.min;
	log(`deployer holds ${eth(deployerBalance)}; distributing ${eth(total)} to ${needy.length} role(s)`);
	if (deployerBalance < total + reserve) {
		console.log(
			`
  deployer is short. It needs ${eth(total + reserve)} ` +
			`(${eth(total)} to distribute + ${eth(reserve)} to deploy with) but holds ${eth(deployerBalance)}.
` +
			`
  Claim Base Sepolia ETH into ${ctx.roles.deployer.address} from a faucet, then re-run --fund.`,
		);
		return false;
	}

	const wallet = ctx.wallet(ctx.roles.deployer);
	for (const { name, acct, top } of needy) {
		const hash = await wallet.sendTransaction({ to: acct.address, value: top });
		const receipt = await ctx.pub.waitForTransactionReceipt({ hash });
		if (receipt.status !== "success") throw new Error(`funding ${name} reverted: ${hash}`);
		log(`  ${name.padEnd(10)} +${eth(top)}  ${hash}`);
	}
	log("done; re-run --preflight to confirm");
	return true;
}

let network = null;
let api = null;

async function preflight(ctx) {
	section("Preflight");

	await check("services are built", () => {
		for (const svc of ["attestor", "monitor", "releaser"]) {
			assert(existsSync(path.join(ROOT, svc, "dist/main.js")), `${svc}/dist/main.js missing — run \`npm run build\` in ${svc}/`);
		}
	});

	await check("contract artifacts exist", () => {
		assert(existsSync(path.join(ROOT, "contracts/out/MigrationVault.sol/MigrationVault.json")),
			"contracts/out missing — run `forge build` in contracts/");
	});

	await check("Zombienet prerequisites present", () => {
		assert(existsSync(path.join(ROOT, "zombienet-macos-arm64")), "zombienet-macos-arm64 missing from the repo root");
		assert(existsSync(path.join(ROOT, "target/release/pendulum-node")), "target/release/pendulum-node missing");
		assert(existsSync(path.join(ROOT, "target/release/wbuild/pendulum-runtime/pendulum_runtime.compact.compressed.wasm")),
			"runtime wasm missing — run `cargo build --release -p pendulum-runtime`");
	});

	await check("Base Sepolia RPC reachable and is NOT mainnet", async () => {
		const id = await assertTestnet(ctx, null);
		return `chain ${id}`;
	});

	await check("every role is funded", async () => {
		const underfunded = [];
		for (const [name, acct, limits] of roleTable(ctx)) {
			const balance = await ctx.pub.getBalance({ address: acct.address });
			console.log(`        ${name.padEnd(10)} ${acct.address}  ${eth(balance)}`);
			if (balance < limits.min) underfunded.push(`${name} (${acct.address}) has ${eth(balance)}`);
		}
		assert(underfunded.length === 0,
			`underfunded:\n        ${underfunded.join("\n        ")}\n\n        ` +
			"Claim once into the deployer from a Base Sepolia faucet, then run --fund to fan out.");
	});
}

/** Enable migrations through the real governance path.
 *
 *  This chain has no sudo pallet, so unlike the Chopsticks phase there is no
 *  storage poke available — which is a feature: it means the rehearsal
 *  exercises the same technical-committee origin that will unpause mainnet.
 *  The generated spec seats a single member, so a threshold of 1 executes
 *  immediately rather than opening a vote. */
async function unpausePallet(alice) {
	const call = api.tx.tokenMigration.setPaused(false);
	const collective = api.tx.technicalCommittee ?? api.tx.council;
	assert(collective, "neither technicalCommittee nor council is available to drive the pause origin");
	await new Promise((resolve, reject) => {
		collective.propose(1, call, call.method.encodedLength)
			.signAndSend(alice, ({ status, dispatchError }) => {
				if (dispatchError) return reject(new Error(dispatchError.toString()));
				if (status.isInBlock) resolve();
			})
			.catch(reject);
	});
	await waitFor(async () => (await api.query.tokenMigration.paused()).isFalse,
		{ timeoutMs: 60_000, label: "the pallet to report unpaused" });
}

async function main() {
	console.log(`Phase 5 — full-stack rehearsal (Zombienet + Base Sepolia)\n  run ${stamp}`);
	const env = loadEnv();
	const ctx = buildContext(env);

	if (FUND) {
		const ok = await fundRoles(ctx);
		process.exit(ok ? 0 : 1);
	}

	await preflight(ctx);
	if (PREFLIGHT_ONLY) {
		process.exit(summarise() ? 0 : 1);
	}
	if (!summarise()) {
		console.log("\npreflight failed — not deploying anything");
		process.exit(1);
	}

	// --- bring up the Substrate side ---------------------------------------
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
	log(`minimum migration ${MIN / PEN_12} PEN`);

	await check("pallet ships paused, then unpauses through the committee origin", async () => {
		assert((await api.query.tokenMigration.paused()).isTrue, "pallet was not paused on a fresh chain");
		await unpausePallet(alice);
	});

	// --- bring up the Base side --------------------------------------------
	section("Base Sepolia");
	const { vault, pen, params, sweepTs } = deployToSepolia({ env, roles: ctx.roles, log });
	log(`vault ${vault}`);
	log(`PEN   ${pen}`);
	const V = { address: vault, abi: vaultAbi };
	const P = { address: pen, abi: erc20Abi };
	const read = (c, fn, args) => ctx.pub.readContract({ ...c, functionName: fn, args });

	await check("fresh vault has consumed no nonce the local chain will emit", async () => {
		const next = BigInt((await api.query.tokenMigration.nextNonce()).toString());
		const consumed = await read(V, "nonceConsumed", [next]);
		assert(!consumed,
			`vault has already consumed nonce ${next}. The local chain restarts its nonce sequence at zero on ` +
			"every spawn, so a reused vault silently skips every release. Deploy fresh contracts.");
	});

	await check("admin accepts the two-step handover", async () => {
		await send(ctx, ctx.roles.admin, { ...V, functionName: "acceptAdmin", args: [] });
		assertEq((await read(V, "admin", [])).toLowerCase(), ctx.roles.admin.address.toLowerCase(), "admin");
	});

	// --- start the fleet ----------------------------------------------------
	section("Attestor fleet");
	killMatching(["dist/main.js"]);
	clearState(["attestor/cp1.json", "attestor/cp2.json", "attestor/cp3.json", "attestor/cp4.json",
		"releaser/releaser-state.json"]);
	const pendulumWs = api._options?.provider?.endpoint ?? process.env.PENDULUM_WS;
	const baseEnv = {
		BASE_RPC_URL: env.BASE_SEPOLIA_RPC_URL,
		VAULT_ADDRESS: vault,
		BASE_CHAIN_ID: "84532",
		POLL_INTERVAL_MS: "5000",
	};
	const pendulumHead = (await api.query.system.number()).toString();
	const baseHead = String(await ctx.pub.getBlockNumber());
	for (let i = 0; i < 4; i++) {
		start(`attestor${i + 1}`, "attestor", {
			...baseEnv, PENDULUM_WS: pendulumWs,
			ATTESTOR_PRIVATE_KEY: env[`ATTESTOR_${i + 1}_PRIVATE_KEY`],
			CHECKPOINT_FILE: `./cp${i + 1}.json`, START_BLOCK: pendulumHead,
		});
	}
	start("monitor", "monitor", { ...baseEnv, PENDULUM_WS: pendulumWs, GRACE_SECONDS: "300" });
	start("releaser", "releaser", { ...baseEnv, RELEASER_PRIVATE_KEY: env.RELEASER_PRIVATE_KEY, START_BLOCK: baseHead });
	log(`fleet started (Pendulum from #${pendulumHead}, Base from #${baseHead})`);

	const recipient = "0x000000000000000000000000000000000000beef";
	const balanceOf = (who) => read(P, "balanceOf", [who]);

	// `pendingRelease` is keyed by the payload hash, not the nonce: the vault
	// commits to the exact (nonce, recipient, amount) tuple so a deferred
	// release cannot be completed with different arguments later.
	const payloadHash = (nonce, amount) =>
		keccak256(encodeAbiParameters(
			[{ type: "uint64" }, { type: "address" }, { type: "uint256" }],
			[nonce, recipient, amount],
		));

	/** Burn on the local chain and return the emitted migration. */
	async function migrate(amountPen) {
		const amount = amountPen * PEN_12;
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
					});
				})
				.catch(reject);
		});
	}

	const diagnose = (message) =>
		`${message}\n--- attestor1 ---\n${logs("attestor1")}\n--- releaser ---\n${logs("releaser")}`;

	// --- scenarios ----------------------------------------------------------
	section("End to end over real infrastructure");

	await check("a burn on Pendulum releases on Base with no manual step", async () => {
		const before = await balanceOf(recipient);
		const { nonce, amount } = await migrate(200n);
		try {
			await waitFor(async () => (await read(V, "nonceConsumed", [nonce])) === true,
				{ timeoutMs: 240_000, intervalMs: 5000, label: "the release to land on Base Sepolia" });
		} catch (e) {
			throw new Error(diagnose(e.message));
		}
		assertEq(await balanceOf(recipient), before + amount * CONVERSION_FACTOR, "released amount");
	});

	await check("losing the approval race is benign — every attestor survives", () => {
		for (let i = 1; i <= 4; i++) {
			assert(alive(`attestor${i}`), `attestor${i} exited:\n${logs(`attestor${i}`)}`);
		}
	});

	await check("three of four attestors still release", async () => {
		await stopAndWait("attestor4");
		const { nonce } = await migrate(150n);
		await waitFor(async () => (await read(V, "nonceConsumed", [nonce])) === true,
			{ timeoutMs: 240_000, intervalMs: 5000, label: "release with 3 attestors" });
	});

	await check("a restarted attestor resumes from its checkpoint without duplicating", async () => {
		const consumedBefore = await read(V, "totalReleased", []);
		start("attestor4", "attestor", {
			...baseEnv, PENDULUM_WS: pendulumWs,
			ATTESTOR_PRIVATE_KEY: env.ATTESTOR_4_PRIVATE_KEY,
			CHECKPOINT_FILE: "./cp4.json", START_BLOCK: pendulumHead,
		});
		await sleep(30_000);
		assert(alive("attestor4"), `attestor4 died on restart:\n${logs("attestor4")}`);
		assertEq(await read(V, "totalReleased", []), consumedBefore, "totalReleased after a restart");
	});

	await check("the monitor holds its conservation invariant", async () => {
		const [balance, released, swept, supply] = await Promise.all([
			read(P, "balanceOf", [vault]), read(V, "totalReleased", []),
			read(V, "totalSwept", []), read(P, "totalSupply", []),
		]);
		assertEq(balance + released + swept, supply, "balance + released + swept == totalSupply");
		assert(alive("monitor"), `monitor exited:\n${logs("monitor")}`);
	});

	await check("the guardian can pause but cannot unpause; the admin can", async () => {
		await send(ctx, ctx.roles.guardian, { ...V, functionName: "pause", args: [] });
		assertEq(await read(V, "paused", []), true, "paused by guardian");
		let rejected = false;
		try {
			await send(ctx, ctx.roles.guardian, { ...V, functionName: "unpause", args: [] });
		} catch { rejected = true; }
		assert(rejected, "the guardian was able to unpause — the asymmetry is broken");
		await send(ctx, ctx.roles.admin, { ...V, functionName: "unpause", args: [] });
		assertEq(await read(V, "paused", []), false, "unpaused by admin");
	});

	if (!SKIP_SLOW) {
		await check(`a cap-deferred release drains by itself (~${params.refillSecondsPer100Pen}s of refill)`, async () => {
			// Exhaust the rolling bucket, then migrate again: the excess must be
			// recorded pending rather than reverting, and the releaser must pick it
			// up unaided as the bucket refills. This is the one behaviour that
			// cannot be faked with a time warp here, which is why it is worth the
			// wall-clock wait.
			const available = await read(V, "availableDailyAllowance", []);
			const drainPen = available / PEN_18;
			assert(drainPen > 100n, `daily cap too small to exercise: ${drainPen} PEN available`);
			await migrate(drainPen);
			await waitFor(async () => (await read(V, "availableDailyAllowance", [])) < PEN_18 * 100n,
				{ timeoutMs: 300_000, intervalMs: 5000, label: "the daily bucket to be exhausted" });

			const { nonce, amount } = await migrate(100n);
			const payload = payloadHash(nonce, amount);
			await waitFor(
				async () => (await read(V, "pendingRelease", [payload])) === true
					|| (await read(V, "nonceConsumed", [nonce])) === true,
				{ timeoutMs: 180_000, intervalMs: 5000, label: "the release to be deferred" });
			await waitFor(async () => (await read(V, "nonceConsumed", [nonce])) === true,
				{ timeoutMs: (params.refillSecondsPer100Pen + 300) * 1000, intervalMs: 10_000,
				  label: "the releaser to drain the deferred release as the bucket refills" });
		});
	}

	// --- record -------------------------------------------------------------
	mkdirSync(runDir, { recursive: true });
	const manifest = {
		startedAt: started.toISOString(),
		commit: execSync("git rev-parse HEAD", { cwd: ROOT, encoding: "utf8" }).trim(),
		base: {
			chainId: 84532, rpc: env.BASE_SEPOLIA_RPC_URL, vault, pen,
			startBlock: baseHead, sweepTimestamp: sweepTs,
			caps: { dailyCap: params.dailyCap.toString(), perReleaseCap: params.perReleaseCap.toString() },
		},
		pendulum: { ws: pendulumWs, startBlock: pendulumHead, minimumMigration: MIN.toString() },
		// Addresses only — private keys stay in .env.rehearsal and out of run artifacts.
		roles: {
			deployer: ctx.roles.deployer.address,
			attestors: ctx.roles.attestors.map((a) => a.address),
			guardian: ctx.roles.guardian.address,
			admin: ctx.roles.admin.address,
			releaser: ctx.roles.releaser.address,
		},
	};
	writeFileSync(path.join(runDir, "manifest.json"), JSON.stringify(manifest, null, 2));
	for (const name of ["attestor1", "attestor2", "attestor3", "attestor4", "monitor", "releaser"]) {
		writeFileSync(path.join(runDir, `${name}.log`), logs(name));
	}
	if (network) writeFileSync(path.join(runDir, "zombienet.log"), network.out.join("\n"));
	console.log(`\n  run artifacts: ${path.relative(ROOT, runDir)}`);

	const ok = summarise();
	if (KEEP) {
		console.log("\n  --keep: leaving the network and fleet running.");
		console.log(`  vault ${vault} on Base Sepolia; Pendulum at ${pendulumWs}`);
		console.log("  tear down with: node -e \"import('./testing/src/zombienet.mjs').then(z=>z.teardown(console.log))\"");
		process.exit(ok ? 0 : 1);
	}
	stopAll();
	await sleep(2000);
	if (!ATTACH) teardown(log);
	if (api) await api.disconnect().catch(() => {});
	process.exit(ok ? 0 : 1);
}

process.on("SIGINT", () => {
	console.log("\ninterrupted — tearing down");
	stopAll();
	if (!KEEP) teardown(console.log);
	process.exit(130);
});

main().catch(async (error) => {
	console.error(`\nrehearsal aborted: ${error.message}`);
	stopAll();
	if (!KEEP && !ATTACH) teardown(console.log);
	process.exit(1);
});
