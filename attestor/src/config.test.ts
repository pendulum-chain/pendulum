import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

/** Load config.js in a fresh process with the required variables plus `extra`. */
function loadConfig(extra: Record<string, string>): { status: number | null; stderr: string } {
	const config = new URL("./config.js", import.meta.url).href;
	const result = spawnSync(process.execPath, ["--input-type=module", "-e", `await import(${JSON.stringify(config)})`], {
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
	return { status: result.status, stderr: result.stderr };
}

test("MISSED_APPROVAL_ALERT_COUNT must be a whole number of at least 1", () => {
	assert.equal(loadConfig({ MISSED_APPROVAL_ALERT_COUNT: "4" }).status, 0);
	for (const value of ["2.5", "0", "three"]) {
		const { status, stderr } = loadConfig({ MISSED_APPROVAL_ALERT_COUNT: value });
		assert.notEqual(status, 0, value);
		assert.match(stderr, /invalid MISSED_APPROVAL_ALERT_COUNT/, value);
	}
});
