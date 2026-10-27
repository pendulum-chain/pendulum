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
			BASE_RPC_URL: "http://127.0.0.1:8545",
			VAULT_ADDRESS: "0x1111111111111111111111111111111111111111",
			RELEASER_PRIVATE_KEY: `0x${"11".repeat(32)}`,
			...extra,
		},
		encoding: "utf8",
	});
	return { status: result.status, stdout: result.stdout.trim(), stderr: result.stderr };
}

test("HEARTBEAT_URL must be an http(s) URL, and a bad one is refused without echoing it", () => {
	assert.equal(loadConfig({ HEARTBEAT_URL: "https://hc-ping.com/secret-token" }, "config.heartbeatUrl").stdout, "https://hc-ping.com/secret-token");
	const { status, stderr } = loadConfig({ HEARTBEAT_URL: "hc-ping.com/secret-token" });
	assert.notEqual(status, 0);
	assert.match(stderr, /invalid HEARTBEAT_URL/);
	assert.doesNotMatch(stderr, /secret-token/);
});

test("fee bumping defaults to 60 s, 20 %, 3 bumps and a 1 gwei cap, and refuses invalid settings", () => {
	const print = "[config.feeBump.afterMs, config.feeBump.percent, config.feeBump.max, config.feeBump.maxFeePerGas].join(' ')";
	assert.equal(loadConfig({}, print).stdout, "60000 20 3 1000000000");
	assert.equal(
		loadConfig({ FEE_BUMP_AFTER_SECONDS: "30", FEE_BUMP_PERCENT: "10", FEE_BUMP_MAX: "0", MAX_FEE_PER_GAS_GWEI: "0.25" }, print)
			.stdout,
		"30000 10 0 250000000",
	);
	for (const [name, value] of [
		["FEE_BUMP_PERCENT", "9"],
		["FEE_BUMP_PERCENT", "12.5"],
		["FEE_BUMP_MAX", "-1"],
		["FEE_BUMP_AFTER_SECONDS", "0"],
		["MAX_FEE_PER_GAS_GWEI", "0"],
		["MAX_FEE_PER_GAS_GWEI", "1e9"],
	]) {
		const { status, stderr } = loadConfig({ [name]: value });
		assert.notEqual(status, 0, `${name}=${value}`);
		assert.match(stderr, new RegExp(`invalid ${name}`), `${name}=${value}`);
	}
});

test("a zero poll interval or read batch size is refused instead of looping or failing every cycle", () => {
	assert.equal(loadConfig({}, "[config.pollIntervalMs, config.readBatchSize].join(' ')").stdout, "60000 100");
	for (const [name, value] of [
		["POLL_INTERVAL_MS", "0"],
		["POLL_INTERVAL_MS", "999"],
		["READ_BATCH_SIZE", "0"],
		["READ_BATCH_SIZE", "2.5"],
	]) {
		const { status, stderr } = loadConfig({ [name]: value });
		assert.notEqual(status, 0, `${name}=${value}`);
		assert.match(stderr, new RegExp(`invalid ${name}`), `${name}=${value}`);
	}
});

test("MAX_BLOCK_RANGE must be a whole number of at least 1", () => {
	assert.equal(loadConfig({}, "String(config.maxBlockRange)").stdout, "10000");
	assert.equal(loadConfig({ MAX_BLOCK_RANGE: "200" }, "String(config.maxBlockRange)").stdout, "200");
	for (const value of ["0", "-1", "2.5", "lots"]) {
		const { status, stderr } = loadConfig({ MAX_BLOCK_RANGE: value });
		assert.notEqual(status, 0, value);
		assert.match(stderr, /invalid MAX_BLOCK_RANGE/, value);
	}
});
