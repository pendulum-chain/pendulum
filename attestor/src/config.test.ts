import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

/** Load config.js in a fresh process with the required variables plus `extra`,
 *  printing `print` (an expression over `config`) on success. */
function loadConfig(extra: Record<string, string>, print = "''"): { status: number | null; stdout: string; stderr: string } {
	const config = new URL("./config.js", import.meta.url).href;
	const script = `const { config } = await import(${JSON.stringify(config)}); console.log(${print})`;
	const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
		env: {
			PATH: process.env.PATH,
			PENDULUM_WS: "ws://127.0.0.1:9944",
			BASE_RPC_URL: "http://127.0.0.1:8545",
			VAULT_ADDRESS: "0x1111111111111111111111111111111111111111",
			ATTESTOR_PRIVATE_KEY: `0x${"11".repeat(32)}`,
			...extra,
		},
		encoding: "utf8",
	});
	return { status: result.status, stdout: result.stdout.trim(), stderr: result.stderr };
}

test("MISSED_APPROVAL_ALERT_COUNT must be a whole number of at least 1", () => {
	assert.equal(loadConfig({ MISSED_APPROVAL_ALERT_COUNT: "4" }).status, 0);
	for (const value of ["2.5", "0", "three"]) {
		const { status, stderr } = loadConfig({ MISSED_APPROVAL_ALERT_COUNT: value });
		assert.notEqual(status, 0, value);
		assert.match(stderr, /invalid MISSED_APPROVAL_ALERT_COUNT/, value);
	}
});

test("fee bumping defaults to 60 s, 20 %, 3 bumps and a 1 gwei cap, and refuses invalid settings", () => {
	const print = "[config.feeBump.afterMs, config.feeBump.percent, config.feeBump.max, config.feeBump.maxFeePerGas].join(' ')";
	assert.equal(loadConfig({}, print).stdout, "60000 20 3 1000000000");
	assert.equal(
		loadConfig({ FEE_BUMP_AFTER_SECONDS: "30", FEE_BUMP_PERCENT: "10", FEE_BUMP_MAX: "0", MAX_FEE_PER_GAS_GWEI: "0.25" }, print)
			.stdout,
		"30000 10 0 250000000",
	);
	const invalid: [string, string][] = [
		// Below the 10 % a replacement needs: the bump would be refused.
		["FEE_BUMP_PERCENT", "9"],
		["FEE_BUMP_PERCENT", "12.5"],
		["FEE_BUMP_MAX", "-1"],
		["FEE_BUMP_MAX", "1.5"],
		["FEE_BUMP_AFTER_SECONDS", "0"],
		["MAX_FEE_PER_GAS_GWEI", "0"],
		["MAX_FEE_PER_GAS_GWEI", "-1"],
		["MAX_FEE_PER_GAS_GWEI", "1e9"],
		["MAX_FEE_PER_GAS_GWEI", "0.0000000001"],
	];
	for (const [name, value] of invalid) {
		const { status, stderr } = loadConfig({ [name]: value });
		assert.notEqual(status, 0, `${name}=${value}`);
		assert.match(stderr, new RegExp(`invalid ${name}`), `${name}=${value}`);
	}
});
