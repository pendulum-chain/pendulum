/**
 * Contract ABIs, loaded from the Foundry build artifacts.
 *
 * Read from `contracts/out/` rather than hand-maintained here so they cannot
 * drift from the compiled contracts -- and, importantly, so viem can decode
 * the vault's custom errors by name. Without the error entries a revert shows
 * up only as a bare selector, which makes every negative assertion in this
 * harness unreadable.
 */

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../contracts/out");

function abiOf(file, name) {
	const p = path.join(OUT, file, `${name}.json`);
	try {
		return JSON.parse(readFileSync(p, "utf8")).abi;
	} catch {
		throw new Error(`missing artifact ${p} — run \`forge build\` in contracts/ first`);
	}
}

export const vaultAbi = abiOf("MigrationVault.sol", "MigrationVault");
export const erc20Abi = abiOf("PEN.sol", "PEN");

export const governorAbi = abiOf("PENGovernor.sol", "PENGovernor");
export const timelockAbi = abiOf("TimelockController.sol", "TimelockController");
