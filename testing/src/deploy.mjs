/**
 * Deploys the migration stack to a local Anvil using the REAL Deploy.s.sol,
 * not a hand-rolled deployment. That is deliberate: the deploy script and its
 * two-step admin handover are themselves part of what phase 1 validates.
 */

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { accounts, keys, RPC } from "./anvil.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const CONTRACTS = path.join(ROOT, "contracts");

export const PARAMS = {
	MAX_ISSUANCE: 150_000_000n * 10n ** 18n,
	// Small caps so the deferral paths are reachable in a test run.
	PER_RELEASE_CAP: 1_000_000n * 10n ** 18n,
	DAILY_CAP: 1_000_000n * 10n ** 18n,
	CONVERSION_FACTOR: 1_000_000n,
};

export function deployStack({ earliestSweepOffsetSeconds = 365 * 24 * 3600 } = {}) {
	const [deployer, attA, attB, attC, attD, guardian, admin] = accounts;
	const env = {
		...process.env,
		PRIVATE_KEY: keys[0],
		ADMIN_SAFE: admin.address,
		GUARDIAN_SAFE: guardian.address,
		ATTESTOR_1: attA.address,
		ATTESTOR_2: attB.address,
		ATTESTOR_3: attC.address,
		ATTESTOR_4: attD.address,
		MAX_ISSUANCE: PARAMS.MAX_ISSUANCE.toString(),
		PER_RELEASE_CAP: PARAMS.PER_RELEASE_CAP.toString(),
		DAILY_CAP: PARAMS.DAILY_CAP.toString(),
		EARLIEST_SWEEP_TS: String(Math.floor(Date.now() / 1000) + earliestSweepOffsetSeconds),
	};

	execFileSync(
		"forge",
		["script", "script/Deploy.s.sol", "--rpc-url", RPC, "--broadcast", "--private-key", keys[0], "-vvv"],
		{ cwd: CONTRACTS, env, stdio: ["ignore", "pipe", "pipe"] },
	);

	// Read the addresses back out of the broadcast record rather than parsing
	// log output, so this stays stable if the script's console output changes.
	const chainId = 31337;
	const file = path.join(CONTRACTS, "broadcast", "Deploy.s.sol", String(chainId), "run-latest.json");
	const run = JSON.parse(readFileSync(file, "utf8"));
	const creations = run.transactions.filter((t) => t.transactionType === "CREATE");
	const vault = creations.find((t) => t.contractName === "MigrationVault")?.contractAddress;
	const pen = creations.find((t) => t.contractName === "PEN")?.contractAddress;
	if (!vault || !pen) throw new Error("could not locate deployed addresses in the broadcast record");
	return { vault, pen, env };
}
