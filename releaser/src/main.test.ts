import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

/** A local JSON-RPC endpoint giving these results by method, and an error for
 *  any other method. */
async function rpcServer(results: Record<string, string>): Promise<Server> {
	const server = createServer((request, response) => {
		let body = "";
		request.on("data", (chunk) => (body += chunk));
		request.on("end", () => {
			const { id, method } = JSON.parse(body);
			const result = results[method];
			response.setHeader("content-type", "application/json");
			response.end(
				JSON.stringify(
					result === undefined
						? { jsonrpc: "2.0", id, error: { code: -32601, message: `unexpected ${method}` } }
						: { jsonrpc: "2.0", id, result },
				),
			);
		});
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	return server;
}

test("startup refuses a Base endpoint serving another chain before it reads the state", { timeout: 30_000 }, async () => {
	// Answers eth_chainId with Base Sepolia, and nothing else.
	const server = await rpcServer({ eth_chainId: "0x14a34" });
	const directory = mkdtempSync(join(tmpdir(), "pen-releaser-main-"));
	const stateFile = join(directory, "state.json");
	// Read first, this would fail on the state instead.
	writeFileSync(stateFile, "{");
	try {
		const child = spawn(process.execPath, [fileURLToPath(new URL("./main.js", import.meta.url))], {
			env: {
				PATH: process.env.PATH,
				BASE_RPC_URL: `http://127.0.0.1:${(server.address() as AddressInfo).port}`,
				VAULT_ADDRESS: "0x1111111111111111111111111111111111111111",
				RELEASER_PRIVATE_KEY: `0x${"11".repeat(32)}`,
				STATE_FILE: stateFile,
			},
		});
		let stderr = "";
		child.stderr.on("data", (chunk) => (stderr += chunk));
		const kill = setTimeout(() => child.kill(), 20_000);
		const status = await new Promise<number | null>((resolve) => child.on("exit", resolve));
		clearTimeout(kill);
		assert.equal(status, 1);
		assert.match(stderr, /releaser startup failed — Error: BASE_RPC_URL serves chain 84532, but BASE_CHAIN_ID is 8453/);
	} finally {
		server.close();
		rmSync(directory, { recursive: true, force: true });
	}
});

test("a failing cycle pages once, then is only logged, and no output shows the endpoint", { timeout: 30_000 }, async () => {
	// The startup reads succeed; then every cycle fails on its first read.
	const server = await rpcServer({
		eth_chainId: "0x2105",
		eth_call: `0x${"0".repeat(63)}1`, // conversionFactor
		eth_getCode: "0x60", // Multicall3
		eth_getBalance: "0xde0b6b3a7640000", // 1 ETH
	});
	const directory = mkdtempSync(join(tmpdir(), "pen-releaser-main-"));
	const child = spawn(process.execPath, [fileURLToPath(new URL("./main.js", import.meta.url))], {
		env: {
			PATH: process.env.PATH,
			BASE_RPC_URL: `http://127.0.0.1:${(server.address() as AddressInfo).port}/v2/secret-key`,
			VAULT_ADDRESS: "0x1111111111111111111111111111111111111111",
			RELEASER_PRIVATE_KEY: `0x${"11".repeat(32)}`,
			STATE_FILE: join(directory, "state.json"),
			POLL_INTERVAL_MS: "1000",
		},
	});
	// A regression must fail the test, not leave the releaser running.
	const kill = setTimeout(() => child.kill(), 20_000);
	try {
		let stdout = "";
		let stderr = "";
		// Until the third failed cycle, paged or logged, or an early exit.
		await new Promise((resolve) => {
			const seen = () => {
				if (`${stdout}${stderr}`.split("releaser cycle failed").length > 3) resolve(undefined);
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
		const output = `${stdout}${stderr}`;
		assert.equal(stderr.split("ALERT: releaser cycle failed").length, 2, output);
		assert.equal(stdout.split("releaser cycle failed:").length, 3, output);
		assert.match(stderr, /URL: <url>/);
		assert.ok(!output.includes("secret-key"), output);
	} finally {
		clearTimeout(kill);
		child.kill();
		server.closeAllConnections();
		server.close();
		rmSync(directory, { recursive: true, force: true });
	}
});
