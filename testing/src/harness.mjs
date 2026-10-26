/**
 * Minimal check runner shared by the phase scripts.
 *
 * Prints one line per check and exits non-zero if any fail, so the harness can
 * gate a deployment step rather than just producing output a human has to read.
 */

const results = [];

export async function check(name, fn) {
	try {
		await fn();
		results.push({ name, ok: true });
		console.log(`  PASS  ${name}`);
	} catch (error) {
		const reason = error instanceof Error ? error.message : String(error);
		results.push({ name, ok: false, reason });
		console.log(`  FAIL  ${name}`);
		console.log(`        ${reason}`);
	}
}

/** Record a check that this run cannot perform, so the summary shows it as
 *  skipped rather than passed. */
export function skip(name, reason) {
	results.push({ name, skipped: true, reason });
	console.log(`  SKIP  ${name}`);
	console.log(`        ${reason}`);
}

export function section(title) {
	console.log(`\n${title}`);
}

export function summarise() {
	const failed = results.filter((r) => !r.ok && !r.skipped);
	const skipped = results.filter((r) => r.skipped);
	const run = results.length - skipped.length;
	console.log(`\n${run - failed.length}/${run} checks passed${skipped.length ? `, ${skipped.length} skipped` : ""}`);
	if (skipped.length > 0) {
		console.log("\nSkipped:");
		for (const s of skipped) console.log(`  - ${s.name}: ${s.reason}`);
	}
	if (failed.length > 0) {
		console.log("\nFailed:");
		for (const f of failed) console.log(`  - ${f.name}: ${f.reason}`);
	}
	return failed.length === 0;
}

export function assert(condition, message) {
	if (!condition) throw new Error(message);
}

export function assertEq(actual, expected, label) {
	const a = typeof actual === "bigint" ? actual.toString() : String(actual);
	const e = typeof expected === "bigint" ? expected.toString() : String(expected);
	if (a !== e) throw new Error(`${label}: expected ${e}, got ${a}`);
}
