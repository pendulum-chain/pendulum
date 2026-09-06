/**
 * Deploys the migration stack to Base Sepolia using the real Deploy.s.sol.
 *
 * Fresh contracts on every run, deliberately. The Zombienet chain is ephemeral
 * and restarts its nonce sequence at zero on each spawn, while the vault's
 * `nonceConsumed` mapping is permanent — so reusing a vault across runs means
 * the second run re-emits nonce 0, every attestor's `alreadyHandled` pre-check
 * returns true, and the pipeline logs "skip (already released)" while testing
 * nothing at all. Redeploying also means the deploy script itself is exercised
 * on every cycle rather than once.
 */

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { BASE_SEPOLIA_CHAIN_ID, PEN_18, ROOT } from "./rehearsal-env.mjs";

const CONTRACTS = path.join(ROOT, "contracts");

export function rehearsalParams(env) {
	const dailyCapPen = BigInt(env.REHEARSAL_DAILY_CAP_PEN ?? "28800");
	// Equal to the daily cap: the vault rejects perReleaseCap > dailyCap since
	// round 9 (that band could never release).
	const perReleaseCapPen = BigInt(env.REHEARSAL_PER_RELEASE_CAP_PEN ?? "28800");
	return {
		maxIssuance: 150_000_000n * PEN_18,
		dailyCap: dailyCapPen * PEN_18,
		perReleaseCap: perReleaseCapPen * PEN_18,
		sweepOffsetSeconds: Number(env.REHEARSAL_SWEEP_OFFSET_SECONDS ?? "3600"),
		// How long 100 PEN takes to refill, for the deferred-drain scenario.
		refillSecondsPer100Pen: Number((100n * 86400n) / dailyCapPen),
	};
}

export function deployToSepolia({ env, roles, log }) {
	const params = rehearsalParams(env);
	const sweepTs = Math.floor(Date.now() / 1000) + params.sweepOffsetSeconds;
	const scriptEnv = {
		...process.env,
		PRIVATE_KEY: env.DEPLOYER_PRIVATE_KEY,
		ADMIN_SAFE: roles.admin.address,
		GUARDIAN_SAFE: roles.guardian.address,
		ATTESTOR_1: roles.attestors[0].address,
		ATTESTOR_2: roles.attestors[1].address,
		ATTESTOR_3: roles.attestors[2].address,
		ATTESTOR_4: roles.attestors[3].address,
		MAX_ISSUANCE: params.maxIssuance.toString(),
		PER_RELEASE_CAP: params.perReleaseCap.toString(),
		DAILY_CAP: params.dailyCap.toString(),
		EARLIEST_SWEEP_TS: String(sweepTs),
	};

	log(`deploying to Base Sepolia (sweep floor in ${params.sweepOffsetSeconds}s) ...`);
	execFileSync(
		"forge",
		[
			"script", "script/Deploy.s.sol",
			"--rpc-url", env.BASE_SEPOLIA_RPC_URL,
			"--broadcast",
			"--private-key", env.DEPLOYER_PRIVATE_KEY,
			"--slow",
		],
		{ cwd: CONTRACTS, env: scriptEnv, stdio: ["ignore", "pipe", "pipe"], maxBuffer: 64 * 1024 * 1024 },
	);

	const file = path.join(
		CONTRACTS, "broadcast", "Deploy.s.sol", String(BASE_SEPOLIA_CHAIN_ID), "run-latest.json",
	);
	const run = JSON.parse(readFileSync(file, "utf8"));
	const creations = run.transactions.filter((t) => t.transactionType === "CREATE");
	const vault = creations.find((t) => t.contractName === "MigrationVault")?.contractAddress;
	const pen = creations.find((t) => t.contractName === "PEN")?.contractAddress;
	if (!vault || !pen) throw new Error("could not locate deployed addresses in the broadcast record");
	return { vault, pen, params, sweepTs };
}

/** Drill-scale governance timings: long enough that each stage is observable
 *  and the ETA gate can be asserted, short enough that two full proposal
 *  lifecycles fit in one sitting. Production values are validated by the
 *  Foundry suite; QUORUM_FRACTION=0 makes mechanics testable with drill-scale
 *  voting power — quorum SIZING is deliberately not rehearsed here. */
export function governanceDrillParams(env) {
	return {
		timelockDelay: Number(env.GOV_TIMELOCK_DELAY ?? "180"),
		votingDelay: Number(env.GOV_VOTING_DELAY ?? "60"),
		votingPeriod: Number(env.GOV_VOTING_PERIOD ?? "240"),
		proposalThreshold: BigInt(env.GOV_PROPOSAL_THRESHOLD_PEN ?? "1000") * PEN_18,
		quorumFraction: Number(env.GOV_QUORUM_FRACTION ?? "0"),
		quorumFloor: BigInt(env.GOV_QUORUM_FLOOR_PEN ?? "0") * PEN_18,
	};
}

/** Runs the real DeployGovernance.s.sol — its first execution anywhere. */
export function deployGovernanceToSepolia({ env, pen, log }) {
	const params = governanceDrillParams(env);
	const scriptEnv = {
		...process.env,
		PEN_TOKEN: pen,
		TIMELOCK_DELAY: String(params.timelockDelay),
		VOTING_DELAY: String(params.votingDelay),
		VOTING_PERIOD: String(params.votingPeriod),
		PROPOSAL_THRESHOLD: params.proposalThreshold.toString(),
		QUORUM_FRACTION: String(params.quorumFraction),
		QUORUM_FLOOR: params.quorumFloor.toString(),
	};
	log(`deploying governance (timelock ${params.timelockDelay}s, voting ${params.votingDelay}s+${params.votingPeriod}s, quorum ${params.quorumFraction}% of circulating, floor ${params.quorumFloor / PEN_18} PEN) ...`);
	execFileSync(
		"forge",
		[
			"script", "script/DeployGovernance.s.sol",
			"--rpc-url", env.BASE_SEPOLIA_RPC_URL,
			"--broadcast",
			"--private-key", env.DEPLOYER_PRIVATE_KEY,
			"--slow",
		],
		{ cwd: CONTRACTS, env: scriptEnv, stdio: ["ignore", "pipe", "pipe"], maxBuffer: 64 * 1024 * 1024 },
	);
	const file = path.join(
		CONTRACTS, "broadcast", "DeployGovernance.s.sol", String(BASE_SEPOLIA_CHAIN_ID), "run-latest.json",
	);
	const run = JSON.parse(readFileSync(file, "utf8"));
	const creations = run.transactions.filter((t) => t.transactionType === "CREATE");
	const timelock = creations.find((t) => t.contractName === "TimelockController")?.contractAddress;
	const governor = creations.find((t) => t.contractName === "PENGovernor")?.contractAddress;
	if (!timelock || !governor) throw new Error("could not locate governance addresses in the broadcast record");
	return { timelock, governor, params };
}
