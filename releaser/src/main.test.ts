import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("startup refuses a Base endpoint serving another chain before it reads the state", { timeout: 30_000 }, async () => {
	// Answers eth_chainId with Base Sepolia, and nothing else.
	const server = createServer((request, response) => {
		let body = "";
		request.on("data", (chunk) => (body += chunk));
		request.on("end", () => {
			const { id, method } = JSON.parse(body);
			response.setHeader("content-type", "application/json");
			response.end(
				JSON.stringify(
					method === "eth_chainId"
						? { jsonrpc: "2.0", id, result: "0x14a34" }
						: { jsonrpc: "2.0", id, error: { code: -32601, message: `unexpected ${method}` } },
				),
			);
		});
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
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
