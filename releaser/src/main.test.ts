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

/** What the startup reads ask for, answered so that startup completes. */
const STARTUP_RESULTS = {
	eth_chainId: "0x2105",
	eth_call: `0x${"0".repeat(63)}1`, // conversionFactor
	eth_getCode: "0x60", // Multicall3
	eth_getBalance: "0xde0b6b3a7640000", // 1 ETH
};

const count = (text: string, marker: string) => text.split(marker).length - 1;

/** The releaser, started against `server`, with its output so far. `until`
 *  resolves once `seen` holds for that output or the releaser has exited. The
 *  releaser is stopped after 20 s, so a regression fails a test, not hangs it. */
function startReleaser(server: Server, stateFile: string) {
	const child = spawn(process.execPath, [fileURLToPath(new URL("./main.js", import.meta.url))], {
		env: {
			PATH: process.env.PATH,
			BASE_RPC_URL: `http://127.0.0.1:${(server.address() as AddressInfo).port}/v2/secret-key`,
			VAULT_ADDRESS: "0x1111111111111111111111111111111111111111",
			RELEASER_PRIVATE_KEY: `0x${"11".repeat(32)}`,
			STATE_FILE: stateFile,
			POLL_INTERVAL_MS: "1000",
		},
	});
	const output = { stdout: "", stderr: "" };
	child.stdout.on("data", (chunk) => (output.stdout += chunk));
	child.stderr.on("data", (chunk) => (output.stderr += chunk));
	const kill = setTimeout(() => child.kill(), 20_000);
	child.on("exit", () => clearTimeout(kill));
	const until = (seen: (all: string) => boolean) =>
		new Promise<void>((resolve) => {
			const check = () => {
				if (seen(`${output.stdout}${output.stderr}`) || child.exitCode !== null || child.signalCode !== null) resolve();
			};
			check();
			child.stdout.on("data", check);
			child.stderr.on("data", check);
			child.on("exit", check);
		});
	return { child, output, until };
}

test("startup refuses a Base endpoint serving another chain before it reads the state", { timeout: 30_000 }, async () => {
	// Answers eth_chainId with Base Sepolia, and nothing else.
	const server = await rpcServer({ eth_chainId: "0x14a34" });
	const directory = mkdtempSync(join(tmpdir(), "pen-releaser-main-"));
	const stateFile = join(directory, "state.json");
	// Read first, this would fail on the state instead.
	writeFileSync(stateFile, "{");
	const releaser = startReleaser(server, stateFile);
	try {
		await releaser.until(() => false);
		assert.equal(releaser.child.exitCode, 1);
		assert.match(releaser.output.stderr, /releaser startup failed — Error: BASE_RPC_URL serves chain 84532, but BASE_CHAIN_ID is 8453/);
	} finally {
		releaser.child.kill();
		server.close();
		rmSync(directory, { recursive: true, force: true });
	}
});

test("a failed startup read is retried, paging once, until the endpoint answers", { timeout: 30_000 }, async () => {
	// Nothing answers at first, as during an outage.
	const results: Record<string, string> = {};
	const server = await rpcServer(results);
	const directory = mkdtempSync(join(tmpdir(), "pen-releaser-main-"));
	const releaser = startReleaser(server, join(directory, "state.json"));
	try {
		await releaser.until((all) => count(all, "releaser startup read failed") >= 2);
		Object.assign(results, STARTUP_RESULTS);
		await releaser.until((all) => all.includes(" started; "));
		const { stdout, stderr } = releaser.output;
		const all = `${stdout}${stderr}`;
		assert.equal(releaser.child.exitCode, null, all);
		assert.ok(all.includes(" started; "), all);
		assert.equal(count(stderr, "ALERT: releaser startup read failed"), 1, all);
		assert.ok(count(stdout, "releaser startup read failed, retrying:") >= 1, all);
		assert.ok(!all.includes("secret-key"), all);
	} finally {
		releaser.child.kill();
		server.closeAllConnections();
		server.close();
		rmSync(directory, { recursive: true, force: true });
	}
});

test("a failing cycle pages once, then is only logged, and no output shows the endpoint", { timeout: 30_000 }, async () => {
	// The startup reads succeed; then every cycle fails on its first read.
	const server = await rpcServer(STARTUP_RESULTS);
	const directory = mkdtempSync(join(tmpdir(), "pen-releaser-main-"));
	const releaser = startReleaser(server, join(directory, "state.json"));
	try {
		// Until the third failed cycle, paged or logged, or an early exit.
		await releaser.until((all) => count(all, "releaser cycle failed") >= 3);
		const { stdout, stderr } = releaser.output;
		const all = `${stdout}${stderr}`;
		assert.equal(count(stderr, "ALERT: releaser cycle failed"), 1, all);
		assert.equal(count(stdout, "releaser cycle failed:"), 2, all);
		assert.match(stderr, /URL: <url>/);
		assert.ok(!all.includes("secret-key"), all);
	} finally {
		releaser.child.kill();
		server.closeAllConnections();
		server.close();
		rmSync(directory, { recursive: true, force: true });
	}
});
