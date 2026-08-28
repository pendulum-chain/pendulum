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
	const perReleaseCapPen = BigInt(env.REHEARSAL_PER_RELEASE_CAP_PEN ?? "50000");
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
