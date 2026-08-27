/**
 * Spawns and supervises the attestor / monitor / releaser processes for the
 * end-to-end phase, so a failed run cannot leave orphans behind.
 */

import { execSync, spawn } from "node:child_process";
import { rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const running = new Map();

/** Start a built service. Output is captured so a crash is diagnosable, and
 *  `exited` records whether the process died -- the attestor race regression
 *  is precisely "a daemon that should have kept running did not". */
export function start(name, dir, env) {
	const proc = spawn("node", ["dist/main.js"], {
		cwd: path.join(ROOT, dir),
		env: { ...process.env, ...env },
		stdio: ["ignore", "pipe", "pipe"],
	});
	const rec = { proc, name, out: [], exited: null };
	const capture = (chunk) => {
		for (const line of String(chunk).split("\n")) if (line.trim()) rec.out.push(line);
		if (rec.out.length > 500) rec.out.splice(0, rec.out.length - 500);
	};
	proc.stdout.on("data", capture);
	proc.stderr.on("data", capture);
	proc.on("exit", (code) => { rec.exited = code ?? -1; });
	running.set(name, rec);
	return rec;
}

export const get = (name) => running.get(name);
export const alive = (name) => running.get(name)?.exited === null;
export const logs = (name) => (running.get(name)?.out ?? []).join("\n");

export function stop(name) {
	const rec = running.get(name);
	if (rec && rec.exited === null) rec.proc.kill("SIGTERM");
}

/** Stop a daemon and WAIT until it has actually exited.
 *
 *  `stop` alone only signals. A daemon that is mid-cycle can still land a
 *  transaction after the signal, so an outage test that does not wait may
 *  observe a quorum it thought it had removed. Escalates to SIGKILL. */
export async function stopAndWait(name, { timeoutMs = 15_000 } = {}) {
	const rec = running.get(name);
	if (!rec || rec.exited !== null) return;
	rec.proc.kill("SIGTERM");
	const deadline = Date.now() + timeoutMs;
	while (rec.exited === null) {
		if (Date.now() > deadline) { rec.proc.kill("SIGKILL"); break; }
		await sleep(250);
	}
	while (rec.exited === null) await sleep(250);
}

export function stopAll() {
	for (const name of running.keys()) stop(name);
}

/** Remove a service's persisted state so a run starts clean. */
export function clearState(files) {
	for (const f of files) { try { rmSync(path.join(ROOT, f)); } catch {} }
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Poll until `fn()` is truthy or the timeout expires. */
export async function waitFor(fn, { timeoutMs = 60_000, intervalMs = 1000, label = "condition" } = {}) {
	const deadline = Date.now() + timeoutMs;
	for (;;) {
		const value = await fn();
		if (value) return value;
		if (Date.now() > deadline) throw new Error(`timed out after ${timeoutMs}ms waiting for ${label}`);
		await sleep(intervalMs);
	}
}

/** Kill any daemons left behind by an earlier aborted run.
 *
 *  Without this a previous run's processes keep writing the same checkpoint
 *  files, so a fresh attestor loads a stale checkpoint moments after the
 *  harness cleared it and silently scans the wrong block range. */
export function killStrays() {
	try {
		execSync("pkill -f 'dist/main.js' || true", { stdio: "ignore" });
	} catch { /* nothing to kill */ }
}

process.on("exit", stopAll);
process.on("SIGINT", () => { stopAll(); process.exit(130); });
process.on("SIGTERM", () => { stopAll(); process.exit(143); });
