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

test("MAX_BLOCK_RANGE must be a whole number of at least 1", () => {
	assert.equal(loadConfig({}, "String(config.maxBlockRange)").stdout, "10000");
	assert.equal(loadConfig({ MAX_BLOCK_RANGE: "200" }, "String(config.maxBlockRange)").stdout, "200");
	for (const value of ["0", "-1", "2.5", "lots"]) {
		const { status, stderr } = loadConfig({ MAX_BLOCK_RANGE: value });
		assert.notEqual(status, 0, value);
		assert.match(stderr, /invalid MAX_BLOCK_RANGE/, value);
	}
});
