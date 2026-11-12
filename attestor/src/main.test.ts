import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("a failed Base read at startup is retried, paging once, instead of exiting", { timeout: 30_000 }, async () => {
	// A Base endpoint that refuses connections, as during an outage.
	const closed = createServer().listen(0, "127.0.0.1");
	await once(closed, "listening");
	const port = (closed.address() as AddressInfo).port;
	closed.close();
	const child = spawn(process.execPath, [fileURLToPath(new URL("./main.js", import.meta.url))], {
		env: {
			PATH: process.env.PATH,
			PENDULUM_WS: "ws://127.0.0.1:9944",
			BASE_RPC_URL: `http://127.0.0.1:${port}/v2/secret-key`,
			VAULT_ADDRESS: "0x1111111111111111111111111111111111111111",
			ATTESTOR_PRIVATE_KEY: `0x${"11".repeat(32)}`,
		},
	});
	let stdout = "";
	let stderr = "";
	// A regression must fail the test, not leave the attestor running.
	const kill = setTimeout(() => child.kill(), 20_000);
	try {
		// Until the second failed read, paged or logged, or an early exit.
		await new Promise((resolve) => {
			const seen = () => {
				if (`${stdout}${stderr}`.split("Base read failed at startup").length > 2) resolve(undefined);
			};
			child.stdout.on("data", (chunk) => {
				stdout += chunk;
				seen();
			});
			child.stderr.on("data", (chunk) => {
				stderr += chunk;
				seen();
			});
			child.on("exit", resolve);
		});
		const all = `${stdout}${stderr}`;
		assert.equal(child.exitCode, null, all);
		assert.equal(stderr.split("ALERT: Base read failed at startup").length, 2, all);
		assert.equal(stdout.split("Base read failed at startup").length, 2, all);
		assert.ok(!all.includes("secret-key"), all);
	} finally {
		clearTimeout(kill);
		child.kill();
	}
});
