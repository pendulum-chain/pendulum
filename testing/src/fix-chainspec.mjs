/**
 * Repair a Pendulum chain spec so the node can read back its own output.
 *
 * `build-spec` serialises `CurrencyId`/`OracleKey` variants first-letter
 * lowercased (`native`, `xCM`, `exchangeRate`) but deserialises expecting the
 * original casing, so `build-spec --chain <plain.json> --raw` fails on a file
 * the very same binary just wrote. This blocks anything that needs a raw spec:
 * `benchmark pallet --chain pendulum`, and Zombienet, which performs the
 * plain -> raw conversion internally.
 *
 * Rather than hardcode a variant list that will drift, this drives itself from
 * the node's own error: it converts, reads `unknown variant `x`, expected one
 * of `A`, `B`` off stderr, rewrites only the keys and string values that equal
 * `x` case-insensitively, and repeats until the conversion succeeds. Field
 * names that merely look similar (`chainType`, `bootNodes`, `tokenSymbol`) are
 * never touched, because the node never complains about them.
 *
 * Usage: node testing/src/fix-chainspec.mjs <node-bin> <plain.json> <out-raw.json>
 */
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const [nodeBin, plainPath, rawOut] = process.argv.slice(2);
if (!nodeBin || !plainPath || !rawOut) {
	console.error("usage: fix-chainspec.mjs <node-bin> <plain.json> <out-raw.json>");
	process.exit(2);
}

const patchedPath = `${rawOut}.plain-patched.json`;
let spec = JSON.parse(readFileSync(plainPath, "utf8"));
const applied = [];

/** Rename every object key and string value equal to `from` (case-insensitively). */
function rename(node, from, to) {
	let count = 0;
	const walk = (value) => {
		if (Array.isArray(value)) {
			for (let i = 0; i < value.length; i++) {
				if (typeof value[i] === "string" && value[i].toLowerCase() === from.toLowerCase()) {
					value[i] = to;
					count++;
				} else walk(value[i]);
			}
			return;
		}
		if (value === null || typeof value !== "object") return;
		for (const key of Object.keys(value)) {
			const child = value[key];
			if (typeof child === "string" && child.toLowerCase() === from.toLowerCase()) {
				value[key] = to;
				count++;
			} else walk(child);
			if (key.toLowerCase() === from.toLowerCase() && key !== to) {
				value[to] = value[key];
				delete value[key];
				count++;
			}
		}
	};
	walk(node);
	return count;
}

for (let attempt = 1; attempt <= 40; attempt++) {
	writeFileSync(patchedPath, JSON.stringify(spec));
	try {
		const raw = execFileSync(
			nodeBin,
			["build-spec", "--chain", patchedPath, "--raw", "--disable-default-bootnode"],
			{ maxBuffer: 512 * 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] },
		);
		writeFileSync(rawOut, raw);
		console.log(`raw spec written to ${rawOut} after ${applied.length} rename(s)`);
		for (const entry of applied) console.log(`  ${entry}`);
		process.exit(0);
	} catch (error) {
		const stderr = `${error.stderr ?? ""}`;
		// Two wordings, depending on how many variants the enum has:
		//   unknown variant `x`, expected one of `A`, `B`
		//   unknown variant `x`, expected `A`
		const match = stderr.match(/unknown variant `([^`]+)`, expected (?:one of )?((?:`[^`]+`(?:, )?)+)/);
		if (!match) {
			console.error("build-spec failed for a reason this cannot repair:\n", stderr.slice(-2000));
			process.exit(1);
		}
		const bad = match[1];
		const expected = [...match[2].matchAll(/`([^`]+)`/g)].map((m) => m[1]);
		const correct = expected.find((name) => name.toLowerCase() === bad.toLowerCase());
		if (!correct) {
			console.error(`no case-insensitive match for \`${bad}\` among ${expected.join(", ")}`);
			process.exit(1);
		}
		const renamed = rename(spec, bad, correct);
		if (renamed === 0) {
			console.error(`could not locate \`${bad}\` in the spec; aborting to avoid a loop`);
			process.exit(1);
		}
		applied.push(`${bad} -> ${correct} (${renamed} site${renamed === 1 ? "" : "s"})`);
	}
}
console.error("gave up after 40 attempts");
process.exit(1);
