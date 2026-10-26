/**
 * Spawns and supervises the attestor / monitor / releaser processes for the
 * end-to-end phase, so a failed run cannot leave orphans behind.
 */

import { execSync, spawn } from "node:child_process";
import { createWriteStream, mkdirSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const running = new Map();

/** Start a built service. Output is captured so a crash is diagnosable, and
 *  `exited` records whether the process died -- the attestor race regression
 *  is precisely "a daemon that should have kept running did not". */
const LOG_DIR = path.join(ROOT, "testing", ".logs");

export function start(name, dir, env) {
	const proc = spawn("node", ["dist/main.js"], {
		cwd: path.resolve(ROOT, dir),
		env: { ...process.env, ...env },
		stdio: ["ignore", "pipe", "pipe"],
	});
	// Tee every daemon's output to testing/.logs/<name>.log as well: the
	// in-memory ring buffer dies with the harness, which left a silent attestor
	// exit during a phase-3 run with no stderr to read afterwards.
	mkdirSync(LOG_DIR, { recursive: true });
	const file = createWriteStream(path.join(LOG_DIR, `${name}.log`), { flags: "a" });
	const rec = { proc, name, out: [], exited: null };
	const capture = (chunk) => {
		file.write(chunk);
		for (const line of String(chunk).split("\n")) if (line.trim()) rec.out.push(line);
		if (rec.out.length > 500) rec.out.splice(0, rec.out.length - 500);
	};
	proc.stdout.on("data", capture);
	proc.stderr.on("data", capture);
	proc.on("exit", (code) => { rec.exited = code ?? -1; file.end(`\n[harness] ${name} exited with code ${code}\n`); });
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
	for (const f of files) { try { rmSync(path.resolve(ROOT, f)); } catch {} }
}

/** The monitor is maintained separately. Set MONITOR_DIR to a built monitor
 *  checkout (absolute, or relative to the current directory) to include it in
 *  a run; unset, runs go ahead without it and skip the monitor checks. */
export const MONITOR_DIR = process.env.MONITOR_DIR ? path.resolve(process.env.MONITOR_DIR) : undefined;

/** Start the monitor from MONITOR_DIR with a clean state file, or say once
 *  that this run is without it. */
export function startMonitor(env) {
	if (!MONITOR_DIR) {
		console.log("  MONITOR_DIR not set: running without the monitor, monitor checks are skipped");
		return;
	}
	clearState([path.join(MONITOR_DIR, "monitor-state.json")]);
	start("monitor", MONITOR_DIR, { ...env, STATE_FILE: "./monitor-state.json" });
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
