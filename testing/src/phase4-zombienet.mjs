/**
 * Phase 4 — real relay-chain finality (Zombienet).
 *
 * The one property Chopsticks cannot reproduce: Chopsticks finalises every
 * block it authors, so an attestor reading finalized heads there is
 * indistinguishable from one reading best heads. Against a genuine relay,
 * finalized lags best, and this is what proves the attestors are gated on
 * finality rather than merely appearing to be.
 *
 * Assumes a network spawned from testing/zombienet.toml with the collator RPC
 * on 9944. Run: node testing/src/phase4-zombienet.mjs
 */
import { execSync } from "node:child_process";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { check, summarise } from "./harness.mjs";

const SAMPLE_SECONDS = Number(process.env.SAMPLE_SECONDS ?? 90);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Find the collator's RPC endpoint. Zombienet reassigns ports on every spawn,
 * and the collator also runs an embedded relay client, so a fixed port is as
 * likely to land on Rococo as on Pendulum. Probe the listening ports and keep
 * the one that reports the Pendulum runtime.
 */
async function connectToCollator() {
	if (process.env.PENDULUM_WS) {
		return ApiPromise.create({ provider: new WsProvider(process.env.PENDULUM_WS), noInitWarn: true });
	}
	const listening = execSync(
		"lsof -nP -iTCP -sTCP:LISTEN 2>/dev/null | grep pendulum | awk '{print $9}' | sed 's/.*://' | sort -un",
	)
		.toString()
		.trim()
		.split("\n")
		.filter(Boolean);
	for (const port of listening) {
		const url = `ws://127.0.0.1:${port}`;
		// Hold the provider separately: several of these ports are not RPC at
		// all (prometheus, p2p), and a provider left behind by a failed probe
		// keeps retrying for the rest of the run.
		const provider = new WsProvider(url, 1000);
		let api;
		try {
			api = await Promise.race([
				ApiPromise.create({ provider, noInitWarn: true, throwOnConnect: true }),
				new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), 8000)),
			]);
			const chain = (await api.rpc.system.chain()).toString();
			if (/pendulum/i.test(chain)) {
				console.log(`collator RPC discovered on ${url} (${chain})`);
				return api;
			}
			await api.disconnect();
		} catch {
			if (api) await api.disconnect().catch(() => {});
			else await provider.disconnect().catch(() => {});
		}
	}
	throw new Error(`no Pendulum collator RPC among listening ports: ${listening.join(", ")}`);
}

const api = await connectToCollator();

await check("collator is a parachain running the Pendulum runtime", async () => {
	const chain = (await api.rpc.system.chain()).toString();
	const name = (await api.rpc.system.name()).toString();
	if (!/pendulum/i.test(chain)) throw new Error(`unexpected chain: ${chain}`);
	return `${chain} / ${name}`;
});

await check("token-migration pallet is present in the runtime metadata", async () => {
	if (!api.query.tokenMigration) throw new Error("tokenMigration missing from metadata");
	const calls = Object.keys(api.tx.tokenMigration);
	for (const required of ["migrate", "setPaused", "migrateTreasury", "setTreasuryDestination"]) {
		if (!calls.includes(required)) throw new Error(`missing extrinsic ${required}`);
	}
	return `4 extrinsics: ${calls.join(", ")}`;
});

await check("pallet ships PAUSED on a chain that never wrote its storage", async () => {
	const paused = await api.query.tokenMigration.paused();
	if (!paused.isTrue) throw new Error("pallet is unpaused on a fresh chain — fail-safe default broken");
	return "paused = true";
});

await check("parachain blocks are being authored", async () => {
	const start = (await api.rpc.chain.getHeader()).number.toNumber();
	await sleep(30_000);
	const end = (await api.rpc.chain.getHeader()).number.toNumber();
	if (end <= start) throw new Error(`best head stuck at ${start}`);
	return `best head ${start} -> ${end}`;
});

// The core of the phase: finality must ADVANCE (proving relay/collator
// interop) and must LAG best (proving it is genuine relay finality and not
// the instant self-finalisation Chopsticks does).
let sawLag = false;
let finalityAdvanced = false;
await check(`finalized head advances and lags best over ${SAMPLE_SECONDS}s`, async () => {
	const firstFinal = (await api.rpc.chain.getHeader(await api.rpc.chain.getFinalizedHead())).number.toNumber();
	const samples = [];
	const deadline = Date.now() + SAMPLE_SECONDS * 1000;
	while (Date.now() < deadline) {
		const best = (await api.rpc.chain.getHeader()).number.toNumber();
		const fin = (await api.rpc.chain.getHeader(await api.rpc.chain.getFinalizedHead())).number.toNumber();
		samples.push(best - fin);
		if (best - fin > 0) sawLag = true;
		await sleep(6000);
	}
	const lastFinal = (await api.rpc.chain.getHeader(await api.rpc.chain.getFinalizedHead())).number.toNumber();
	finalityAdvanced = lastFinal > firstFinal;
	if (!finalityAdvanced) {
		throw new Error(`finalized head stuck at ${firstFinal} — relay is not finalising parachain blocks`);
	}
	const mean = samples.reduce((a, b) => a + b, 0) / samples.length;
	return `finalized ${firstFinal} -> ${lastFinal}; lag samples [${samples.join(", ")}], mean ${mean.toFixed(1)} blocks`;
});

await check("finality lag is strictly positive at least once (not instant finalisation)", async () => {
	if (!sawLag) throw new Error("finalized never lagged best — this is not genuine relay finality");
	return "observed best > finalized, as expected against a real relay";
});

await check("subscribeFinalizedHeads delivers monotonically increasing heads", async () => {
	const seen = [];
	await new Promise((resolve, reject) => {
		let unsub;
		const timer = setTimeout(() => {
			if (unsub) unsub();
			seen.length >= 2 ? resolve() : reject(new Error(`only ${seen.length} finalized head(s) in 60s`));
		}, 60_000);
		api.rpc.chain
			.subscribeFinalizedHeads((head) => {
				seen.push(head.number.toNumber());
				if (seen.length >= 3) {
					clearTimeout(timer);
					if (unsub) unsub();
					resolve();
				}
			})
			.then((u) => {
				unsub = u;
			})
			.catch(reject);
	});
	for (let i = 1; i < seen.length; i++) {
		if (seen[i] <= seen[i - 1]) throw new Error(`non-monotonic finalized heads: ${seen.join(", ")}`);
	}
	return `heads ${seen.join(" -> ")}`;
});

await api.disconnect();
summarise();
