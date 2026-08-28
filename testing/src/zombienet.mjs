/**
 * Spawning, probing and tearing down the local Zombienet network.
 *
 * Note `killMatching`: it deliberately does not shell out to `pkill -f`.
 * `pkill -f <pattern>` run through a shell matches the shell's own command
 * line, because the pattern is part of it — which is how three waiter shells
 * in an earlier session ended up killing themselves or spinning forever on a
 * condition that could never become false. Reading the process table and
 * filtering in-process avoids the whole class of problem.
 */

import { execFileSync, execSync, spawn } from "node:child_process";
import path from "node:path";
import { ROOT, TESTING } from "./rehearsal-env.mjs";

export const CHAIN_BINARIES = [
	"zombienet-macos",
	"target/testnet/polkadot",
	"target/release/pendulum-node",
];

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Kill processes whose command line contains any of `patterns`, skipping this
 *  process and its ancestors so a teardown can never kill its own caller. */
export function killMatching(patterns, { signal = "SIGKILL" } = {}) {
	let table;
	try {
		table = execSync("ps -eo pid,ppid,command", { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
	} catch {
		return 0;
	}
	const own = new Set();
	for (let pid = process.pid; pid && pid > 1; ) {
		own.add(pid);
		const row = table.split("\n").find((l) => Number(l.trim().split(/\s+/)[0]) === pid);
		pid = row ? Number(row.trim().split(/\s+/)[1]) : 0;
	}
	let killed = 0;
	for (const line of table.split("\n").slice(1)) {
		const trimmed = line.trim();
		if (!trimmed) continue;
		const [pidStr, , ...rest] = trimmed.split(/\s+/);
		const pid = Number(pidStr);
		const command = rest.join(" ");
		if (!pid || own.has(pid)) continue;
		if (!patterns.some((p) => command.includes(p))) continue;
		try {
			process.kill(pid, signal);
			killed++;
		} catch { /* already gone */ }
	}
	return killed;
}

/** Regenerate the raw chain spec Zombienet consumes. */
export function generateSpec(log) {
	log("generating chain spec (repairing variant casings, swapping in the shipped runtime) ...");
	execFileSync("node", [path.join(TESTING, "src/make-zombienet-spec.mjs")], {
		cwd: ROOT,
		stdio: ["ignore", "pipe", "pipe"],
		maxBuffer: 64 * 1024 * 1024,
	});
}

export function spawnNetwork(log) {
	const binary = path.join(ROOT, "zombienet-macos-arm64");
	log("spawning Zombienet (relay + Pendulum collator) ...");
	const proc = spawn(binary, ["spawn", "testing/zombienet.toml", "--provider", "native"], {
		cwd: ROOT,
		stdio: ["ignore", "pipe", "pipe"],
	});
	const out = [];
	const capture = (c) => {
		for (const l of String(c).split("\n")) if (l.trim()) out.push(l.replace(/\x1b\[[0-9;]*m/g, ""));
		if (out.length > 2000) out.splice(0, out.length - 2000);
	};
	proc.stdout.on("data", capture);
	proc.stderr.on("data", capture);
	return { proc, out };
}

export function teardown(log) {
	const killed = killMatching(CHAIN_BINARIES);
	if (log) log(`torn down ${killed} chain process(es)`);
	return killed;
}

/**
 * Find the collator's RPC. Zombienet reassigns ports on every spawn and the
 * collator also exposes an embedded relay client, so a fixed port is as likely
 * to answer as Rococo as it is as Pendulum.
 */
export async function discoverCollator(ApiPromise, WsProvider, { timeoutMs = 240_000, log } = {}) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		let ports = [];
		try {
			ports = execSync(
				"lsof -nP -iTCP -sTCP:LISTEN 2>/dev/null | grep pendulum | awk '{print $9}' | sed 's/.*://' | sort -un",
				{ encoding: "utf8" },
			).trim().split("\n").filter(Boolean);
		} catch { /* nothing listening yet */ }

		for (const port of ports) {
			const url = `ws://127.0.0.1:${port}`;
			// Hold the provider so a failed probe can be disconnected: several of
			// these ports are not RPC at all, and an abandoned provider retries
			// for the rest of the run.
			const provider = new WsProvider(url, 1000);
			let api;
			try {
				api = await Promise.race([
					ApiPromise.create({ provider, noInitWarn: true, throwOnConnect: true }),
					new Promise((_, rej) => setTimeout(() => rej(new Error("timeout")), 8000)),
				]);
				const chain = (await api.rpc.system.chain()).toString();
				if (/pendulum/i.test(chain)) {
					if (log) log(`collator RPC on ${url} (${chain})`);
					return api;
				}
				await api.disconnect();
			} catch {
				if (api) await api.disconnect().catch(() => {});
				else await provider.disconnect().catch(() => {});
			}
		}
		await sleep(5000);
	}
	throw new Error("no Pendulum collator RPC appeared — check the Zombienet log");
}

/** Wait until the parachain is finalising, which is the real readiness signal:
 *  authoring alone does not mean the relay is backing and including candidates. */
export async function waitForFinality(api, { timeoutMs = 300_000, log } = {}) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		const best = (await api.rpc.chain.getHeader()).number.toNumber();
		const finalized = (await api.rpc.chain.getHeader(await api.rpc.chain.getFinalizedHead())).number.toNumber();
		if (finalized >= 1) {
			if (log) log(`parachain finalising (best #${best}, finalized #${finalized})`);
			return { best, finalized };
		}
		await sleep(5000);
	}
	throw new Error("parachain never finalised a block — the relay is not including candidates");
}
