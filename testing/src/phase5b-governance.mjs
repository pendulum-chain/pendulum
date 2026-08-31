/**
 * Phase 5b — governance rehearsal: Governor + Timelock on Base Sepolia.
 *
 * The Foundry suite covers the governance LOGIC (a full proposal lifecycle,
 * threshold, quorum arithmetic, the timestamp clock). What had never run
 * before this script: DeployGovernance.s.sol itself, the vault-admin ->
 * timelock handover — whose acceptance is itself a governance proposal — and
 * any proposal against deployed contracts on a public chain.
 *
 * Purely Base-side: no Zombienet and no daemon fleet. Voter PEN is released
 * from the vault through the real 3-of-4 approve() path, driven directly by
 * the attestor keys with synthetic (nonce, recipient, amount) tuples.
 *
 * Drill parameters are wall-clock-sized (see governanceDrillParams), and the
 * quorum fraction is 0 so mechanics are testable with drill-scale voting
 * power. Quorum SIZING is deliberately not rehearsed — production quorum is
 * ~3M delegated PEN, unreachable here — and stays covered by the unit tests.
 *
 * Usage:
 *   node src/phase5b-governance.mjs             full automated run
 *   node src/phase5b-governance.mjs --manual    deploy + fund + verify wiring,
 *                                               then stop and print a handoff
 *                                               card for a by-hand walkthrough
 *                                               (Tally / Blockscout / MetaMask)
 *   SKIP_VERIFY=1 ...                           skip Blockscout verification
 */

import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { encodeFunctionData, keccak256, parseEventLogs, toBytes } from "viem";
import { erc20Abi, governorAbi, timelockAbi, vaultAbi } from "./abi.mjs";
import { waitFor } from "./daemons.mjs";
import { assert, assertEq, check, section, summarise } from "./harness.mjs";
import {
	assertTestnet, buildContext, loadEnv, PEN_12, PEN_18, ROOT, send, TESTING,
} from "./rehearsal-env.mjs";
import { deployGovernanceToSepolia, deployToSepolia } from "./rehearsal-deploy.mjs";

const flags = new Set(process.argv.slice(2));
const MANUAL = flags.has("--manual");

const started = new Date();
const stamp = `${started.toISOString().replace(/[:.]/g, "-")}-governance`;
const runDir = path.join(TESTING, ".rehearsal", stamp);
const log = (m) => console.log(`  ${m}`);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** OZ Governor proposal states, for readable assertions. */
const STATE = { Pending: 0, Active: 1, Canceled: 2, Defeated: 3, Succeeded: 4, Queued: 5, Expired: 6, Executed: 7 };
const stateName = (n) => Object.keys(STATE).find((k) => STATE[k] === Number(n)) ?? String(n);

async function main() {
	console.log(`Phase 5b — governance rehearsal (Base Sepolia)\n  run ${stamp}`);
	const env = loadEnv();
	const ctx = buildContext(env);
	await assertTestnet(ctx, null);

	/** Throttle-tolerant read (same rationale as phase 5). */
	async function read(c, fn, args) {
		let last;
		for (let attempt = 0; attempt < 5; attempt++) {
			try {
				return await ctx.pub.readContract({ ...c, functionName: fn, args });
			} catch (error) {
				last = error;
				const text = `${error?.details ?? ""} ${error?.shortMessage ?? ""} ${error?.message ?? ""}`;
				if (!/rate limit|too many requests|timeout|fetch failed|\b50[234]\b/i.test(text)) throw error;
				await sleep(3000 * (attempt + 1));
			}
		}
		throw last;
	}
	const eventually = (fn, label, timeoutMs = 120_000) =>
		waitFor(async () => { try { await fn(); return true; } catch { return false; } },
			{ timeoutMs, intervalMs: 3000, label });

	// --- contracts ----------------------------------------------------------
	section("Deploy (token + vault)");
	const { vault, pen } = deployToSepolia({ env, roles: ctx.roles, log });
	log(`vault ${vault}\n  PEN   ${pen}`);
	const V = { address: vault, abi: vaultAbi };
	const P = { address: pen, abi: erc20Abi };

	await waitFor(
		async () => (await read(V, "pendingAdmin", [])).toLowerCase() === ctx.roles.admin.address.toLowerCase(),
		{ timeoutMs: 120_000, intervalMs: 3000, label: "pendingAdmin to be visible" });
	await send(ctx, ctx.roles.admin, { ...V, functionName: "acceptAdmin", args: [] });
	await eventually(
		async () => assertEq((await read(V, "admin", [])).toLowerCase(), ctx.roles.admin.address.toLowerCase(), "admin"),
		"admin handover to settle");

	await check("fresh vault has consumed none of the drill nonces", async () => {
		assertEq(await read(V, "nonceConsumed", [0n]), false, "nonceConsumed(0)");
	});

	// --- voter funding through the real release path ------------------------
	section("Voter funding via 3-of-4 release");
	// The releaser key doubles as the VOTER: it is gas-funded, unprivileged
	// over the vault, and its production role is irrelevant here.
	const voter = ctx.roles.releaser;
	const releases = [
		{ nonce: 0n, palletAmount: 10_000n * PEN_12 },
		{ nonce: 1n, palletAmount: 8_000n * PEN_12 },
	];
	await check("attestor keys release PEN to the voter (18k) through approve()", async () => {
		for (const r of releases) {
			for (const attestor of ctx.roles.attestors.slice(0, 3)) {
				// Fixed gas: approve() is bimodal — the threshold-crossing call
				// executes the release inline, and gas estimated against a node
				// that has not yet seen the earlier approvals covers only the
				// cheap record path. This is the same OutOfGas the attestor
				// daemon pads for (GAS_LIMIT_MULTIPLIER); a direct caller must
				// pad too.
				await send(ctx, attestor, { ...V, functionName: "approve", args: [r.nonce, voter.address, r.palletAmount], gas: 500_000n });
			}
			await eventually(async () => assertEq(await read(V, "nonceConsumed", [r.nonce]), true, `nonce ${r.nonce}`),
				`release ${r.nonce} to settle`);
		}
		const balance = await read(P, "balanceOf", [voter.address]);
		assertEq(balance, 18_000n * PEN_18, "voter PEN balance");
	});

	// --- governance deployment ----------------------------------------------
	section("Governance deployment (first execution of DeployGovernance.s.sol)");
	const { timelock, governor, params } = deployGovernanceToSepolia({ env, pen, log });
	log(`timelock ${timelock}\n  governor ${governor}`);
	const G = { address: governor, abi: governorAbi };
	const T = { address: timelock, abi: timelockAbi };

	await check("role wiring: governor proposes/cancels, execution is open, deployer is out", async () => {
		const [proposerRole, cancellerRole, executorRole, adminRole] = await Promise.all([
			read(T, "PROPOSER_ROLE", []), read(T, "CANCELLER_ROLE", []),
			read(T, "EXECUTOR_ROLE", []), read(T, "DEFAULT_ADMIN_ROLE", []),
		]);
		// The renounce is the deploy's LAST transaction; retry until the reads
		// see it rather than trusting the first node the RPC hands us.
		await eventually(async () => {
			assertEq(await read(T, "hasRole", [proposerRole, governor]), true, "governor is proposer");
			assertEq(await read(T, "hasRole", [cancellerRole, governor]), true, "governor is canceller");
			assertEq(await read(T, "hasRole", [executorRole, "0x0000000000000000000000000000000000000000"]), true, "execution open to anyone");
			assertEq(await read(T, "hasRole", [adminRole, ctx.roles.deployer.address]), false, "deployer admin renounced");
			assertEq(await read(T, "hasRole", [adminRole, timelock]), true, "timelock self-administered");
		}, "the role wiring (incl. the final renounce) to be visible");
	});

	await check("governor references and clock are correct", async () => {
		assertEq((await read(G, "token", [])).toLowerCase(), pen.toLowerCase(), "governor token");
		assertEq((await read(G, "timelock", [])).toLowerCase(), timelock.toLowerCase(), "governor timelock");
		assertEq(await read(G, "CLOCK_MODE", []), "mode=timestamp", "clock mode");
		assertEq(await read(G, "proposalThreshold", []), params.proposalThreshold, "proposal threshold");
	});

	await check("voting power exists only after delegation", async () => {
		const block = await ctx.pub.getBlock();
		const before = await read(G, "getVotes", [voter.address, block.timestamp - 1n]);
		assertEq(before, 0n, "votes before delegation");
		await send(ctx, voter, { ...P, functionName: "delegate", args: [voter.address] });
		await eventually(async () => {
			const now = (await ctx.pub.getBlock()).timestamp;
			const votes = await read(G, "getVotes", [voter.address, now - 1n]);
			assertEq(votes, 18_000n * PEN_18, "votes after delegation");
		}, "delegated power to appear");
	});

	// --- proposal machinery --------------------------------------------------
	/** Run one proposal through its whole life: propose -> vote -> queue ->
	 *  (assert the ETA gates) -> execute. Returns the proposal id. */
	/** Run one proposal through its whole life: propose -> vote -> queue ->
	 *  (assert the ETA gates) -> execute. Returns the proposal id.
	 *
	 *  Every step tolerates a lagging node behind the load-balanced RPC: a
	 *  state() read for a proposal a node has not seen yet REVERTS
	 *  (GovernorNonexistentProposal) rather than returning Pending, and a
	 *  simulate against such a node rejects a perfectly valid vote — so state
	 *  polls treat reverts as "not yet" and the writes retry.
	 */
	async function governanceExecute(description, target, calldata) {
		const targets = [target];
		const values = [0n];
		const calldatas = [calldata];
		const descriptionHash = keccak256(toBytes(description));

		const receipt = await send(ctx, voter, { ...G, functionName: "propose", args: [targets, values, calldatas, description] });
		const created = parseEventLogs({ abi: governorAbi, logs: receipt.logs })
			.find((e) => e.eventName === "ProposalCreated");
		assert(created, "propose succeeded but emitted no ProposalCreated event");
		const proposalId = created.args.proposalId;
		log(`proposed "${description}" -> id ${proposalId} (tx ${receipt.transactionHash})`);

		const stateIs = async (want) => {
			try { return Number(await read(G, "state", [proposalId])) === want; } catch { return false; }
		};
		const sendRetrying = (acct, params, label, timeoutMs = 120_000) =>
			waitFor(async () => {
				try { await send(ctx, acct, params); return true; } catch { return false; }
			}, { timeoutMs, intervalMs: 5000, label });

		await waitFor(() => stateIs(STATE.Active),
			{ timeoutMs: (params.votingDelay + 180) * 1000, intervalMs: 5000, label: `voting to open (${params.votingDelay}s delay)` });
		await sendRetrying(voter, { ...G, functionName: "castVote", args: [proposalId, 1] }, "the FOR vote to land");
		log("voted FOR");

		await waitFor(() => stateIs(STATE.Succeeded),
			{ timeoutMs: (params.votingPeriod + 240) * 1000, intervalMs: 10_000, label: `voting to close (${params.votingPeriod}s period)` });
		await sendRetrying(voter, { ...G, functionName: "queue", args: [targets, values, calldatas, descriptionHash] }, "the queue to land");
		await waitFor(() => stateIs(STATE.Queued),
			{ timeoutMs: 120_000, intervalMs: 5000, label: "the queue to be visible" });
		log("queued into the timelock");

		// The delay must actually gate: executing straight after queueing has to
		// revert until the ETA passes. (Confirmed from the other side below by
		// the same call SUCCEEDING once the delay has run out.)
		let gated = false;
		try {
			await send(ctx, voter, { ...G, functionName: "execute", args: [targets, values, calldatas, descriptionHash] });
		} catch { gated = true; }
		assert(gated, "execute succeeded BEFORE the timelock delay — the delay does not gate");
		log(`early execute reverted as required; waiting out the ${params.timelockDelay}s delay`);

		await sleep((params.timelockDelay + 15) * 1000);
		await sendRetrying(voter, { ...G, functionName: "execute", args: [targets, values, calldatas, descriptionHash] }, "the execute to land");
		await waitFor(() => stateIs(STATE.Executed),
			{ timeoutMs: 120_000, intervalMs: 5000, label: "execution to be visible" });
		return proposalId;
	}

	if (MANUAL) {
		printHandoff({ vault, pen, timelock, governor, voter, params, releases });
		process.exit(summarise() ? 0 : 1);
	}

	// --- the handover --------------------------------------------------------
	section("Vault-admin handover to the timelock, by proposal");
	let handoverProposal;
	await check("transferAdmin(timelock), then a proposal executes acceptAdmin()", async () => {
		await send(ctx, ctx.roles.admin, { ...V, functionName: "transferAdmin", args: [timelock] });
		await eventually(async () => assertEq((await read(V, "pendingAdmin", [])).toLowerCase(), timelock.toLowerCase(), "pendingAdmin"),
			"transferAdmin to settle");
		handoverProposal = await governanceExecute(
			"Accept MigrationVault admin (handover to governance)",
			vault,
			encodeFunctionData({ abi: vaultAbi, functionName: "acceptAdmin", args: [] }),
		);
		assertEq((await read(V, "admin", [])).toLowerCase(), timelock.toLowerCase(), "vault admin is the timelock");
	});

	// --- a real action through governance ------------------------------------
	section("A real admin action, end to end through governance");
	let capsProposal;
	await check("a proposal raises the vault caps through the timelock", async () => {
		const perRelease = await read(V, "perReleaseCap", []);
		const daily = await read(V, "dailyCap", []);
		capsProposal = await governanceExecute(
			"Raise MigrationVault caps (drill)",
			vault,
			encodeFunctionData({ abi: vaultAbi, functionName: "setCaps", args: [perRelease * 2n, daily * 2n] }),
		);
		assertEq(await read(V, "perReleaseCap", []), perRelease * 2n, "perReleaseCap");
		assertEq(await read(V, "dailyCap", []), daily * 2n, "dailyCap");
	});

	// --- negatives ------------------------------------------------------------
	section("What must NOT work");
	await check("an account below the proposal threshold cannot propose", async () => {
		let rejected = false;
		try {
			await send(ctx, ctx.roles.guardian, {
				...G, functionName: "propose",
				args: [[vault], [0n], [encodeFunctionData({ abi: vaultAbi, functionName: "unpause", args: [] })], "no votes"],
			});
		} catch { rejected = true; }
		assert(rejected, "a zero-power account was able to propose");
	});

	await check("the old admin EOA has lost its power over the vault", async () => {
		let rejected = false;
		try {
			await send(ctx, ctx.roles.admin, { ...V, functionName: "setCaps", args: [1n * PEN_18, 1n * PEN_18] });
		} catch { rejected = true; }
		assert(rejected, "the pre-handover admin can still change caps");
	});

	await check("guardian can still pause; nobody can unpause without a proposal", async () => {
		await send(ctx, ctx.roles.guardian, { ...V, functionName: "pause", args: [] });
		await eventually(async () => assertEq(await read(V, "paused", []), true, "paused"), "pause to settle");
		for (const [name, acct] of [["guardian", ctx.roles.guardian], ["old admin", ctx.roles.admin]]) {
			let rejected = false;
			try { await send(ctx, acct, { ...V, functionName: "unpause", args: [] }); } catch { rejected = true; }
			assert(rejected, `${name} was able to unpause without governance`);
		}
		// NOTE deliberately left paused: unpausing now takes a full proposal
		// (>= 48h in production) — that latency is the design's cost, and this
		// is where it becomes tangible.
	});

	// --- record ---------------------------------------------------------------
	mkdirSync(runDir, { recursive: true });
	writeFileSync(path.join(runDir, "manifest.json"), JSON.stringify({
		kind: "phase5b-governance",
		startedAt: started.toISOString(),
		commit: execSync("git rev-parse HEAD", { cwd: ROOT, encoding: "utf8" }).trim(),
		contracts: { vault, pen, timelock, governor },
		voter: voter.address,
		params: { ...params, proposalThreshold: params.proposalThreshold.toString() },
		proposals: {
			handover: handoverProposal?.toString(),
			capsRaise: capsProposal?.toString(),
		},
		endState: "vault admin = timelock; vault left PAUSED (unpause requires a proposal)",
	}, null, 2));
	console.log(`\n  run artifacts: ${path.relative(ROOT, runDir)}`);
	printHandoff({ vault, pen, timelock, governor, voter, params, releases });
	process.exit(summarise() ? 0 : 1);
}

function printHandoff({ vault, pen, timelock, governor, voter, params }) {
	console.log(`
  ================= governance handoff card =================
  network         Base Sepolia (chain 84532)
  PEN token       ${pen}
  MigrationVault  ${vault}
  Timelock        ${timelock}
  Governor        ${governor}
  voter EOA       ${voter.address}  (the releaser key; holds 18k PEN)

  Explore (Blockscout, verified read/write tabs):
    https://base-sepolia.blockscout.com/address/${governor}
    https://base-sepolia.blockscout.com/address/${vault}

  Tally (one-time, needs your wallet): tally.xyz -> Add a DAO ->
    network Base Sepolia, governor ${governor}
    (token is auto-detected from the governor)

  By hand, remember the ORDER: delegate FIRST (power snapshots at
  proposal creation), then propose -> wait ${params.votingDelay}s -> vote ->
  wait out the ${params.votingPeriod}s period -> queue -> wait ${params.timelockDelay}s -> execute.
  ===========================================================
`);
}

main().catch((error) => {
	console.error(`\nphase 5b aborted: ${error.message}`);
	process.exit(1);
});
