/**
 * Build a Zombienet-ready raw chain spec for the Pendulum runtime.
 *
 * Zombienet normally injects the collator's authoring key into genesis, but it
 * can only do that for a spec it builds itself — and it cannot build this one,
 * because `build-spec --chain pendulum` emits variant casings the node refuses
 * to read back (see fix-chainspec.mjs). So we take over the whole pipeline:
 * generate the plain spec, repoint authority/governance genesis at well-known
 * dev keys, then convert to raw through the repair loop.
 *
 * Only the parts that must differ on a local network are touched — authorities,
 * governance membership, funding and the relay id. The migration-relevant
 * genesis (and therefore what the pallet sees) stays as production emits it.
 *
 * Usage: node testing/src/make-zombienet-spec.mjs <out-raw.json>
 */
import { execFileSync } from "node:child_process";
import { cryptoWaitReady, encodeAddress } from "@polkadot/util-crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../..");
const NODE_BIN = process.env.PENDULUM_NODE ?? resolve(repoRoot, "target/release/pendulum-node");

// Well-known dev public keys. This spec serialises AccountId as SS58, so they
// are encoded with the chain's own prefix once it is read off the spec.
// //Alice — zombienet derives the same key for a node named `alice`.
const ALICE_PUB = "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
// //Bob, //Charlie, //Dave — funded so the harness has accounts to migrate from.
const FUNDED_PUB = [
	ALICE_PUB,
	"0x8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48",
	"0x90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22",
	"0x306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20",
];

await cryptoWaitReady();

const rawOut = resolve(process.argv[2] ?? resolve(repoRoot, "testing/.zombienet-pendulum-raw.json"));
const plainPath = `${rawOut}.plain.json`;

console.log("building plain spec from --chain pendulum ...");
const plain = execFileSync(NODE_BIN, ["build-spec", "--chain", "pendulum", "--disable-default-bootnode"], {
	maxBuffer: 512 * 1024 * 1024,
});
const spec = JSON.parse(plain.toString());

// Network identity: a local rococo relay, not Polkadot.
spec.relay_chain = "rococo-local";
spec.bootNodes = [];
spec.name = "Pendulum Local";
spec.id = "pendulum-local";

const ss58Prefix = spec.properties?.ss58Format ?? 42;
const ALICE = encodeAddress(ALICE_PUB, ss58Prefix);
const FUNDED = FUNDED_PUB.map((pub) => encodeAddress(pub, ss58Prefix));
console.log(`encoding accounts with ss58 prefix ${ss58Prefix}; alice = ${ALICE}`);

const genesis = spec.genesis.runtimeGenesis.patch;

// One collator (alice) authors every block.
const stake = 5_000_000_000_000_000n;
genesis.session = { keys: [[ALICE, ALICE, { aura: ALICE }]] };
if (genesis.parachainStaking) {
	genesis.parachainStaking.stakers = [[ALICE, null, Number(stake)]];
}

// Governance origins we must be able to drive locally: the pallet's pause
// origin is root / half-council / 2-3 technical committee, and this chain has
// no sudo pallet.
if (genesis.council) genesis.council.members = [ALICE];
if (genesis.technicalCommittee) genesis.technicalCommittee.members = [ALICE];

// Fund the dev accounts on top of whatever production genesis already holds.
const endowment = 1_000_000_000_000_000_000n;
genesis.balances = genesis.balances ?? { balances: [] };
const existing = new Map(genesis.balances.balances.map(([who, amount]) => [who, amount]));
for (const who of FUNDED) existing.set(who, Number(endowment));
genesis.balances.balances = [...existing.entries()];

writeFileSync(plainPath, JSON.stringify(spec));
console.log(`patched plain spec -> ${plainPath}`);

console.log("converting to raw (repairing variant casings as needed) ...");
execFileSync("node", [resolve(here, "fix-chainspec.mjs"), NODE_BIN, plainPath, rawOut], { stdio: "inherit" });

// Replace the runtime the node binary happens to embed with the artifact we
// actually ship. `cargo build --features runtime-benchmarks` silently rewrites
// the embedded wasm, and a benchmarking runtime decompresses past the relay's
// VALIDATION_CODE_BOMB_LIMIT (MAX_CODE_SIZE * 4 = 12 MiB) — the relay then
// rejects every candidate with `PossibleBomb` and the parachain never gets
// past its own block #1. Using the shipped artifact is both smaller and the
// thing under test.
const runtimeWasm = process.env.PENDULUM_RUNTIME_WASM ??
	resolve(repoRoot, "target/release/wbuild/pendulum-runtime/pendulum_runtime.compact.compressed.wasm");
const rawSpec = JSON.parse(readFileSync(rawOut, "utf8"));
const CODE_KEY = "0x3a636f6465"; // :code
const embedded = rawSpec.genesis.raw.top[CODE_KEY];
const shipped = `0x${readFileSync(runtimeWasm).toString("hex")}`;
if (embedded !== shipped) {
	const mib = (hex) => ((hex.length - 2) / 2 / 1048576).toFixed(2);
	console.log(`replacing embedded :code (${mib(embedded)} MiB) with ${runtimeWasm} (${mib(shipped)} MiB)`);
	rawSpec.genesis.raw.top[CODE_KEY] = shipped;
	writeFileSync(rawOut, JSON.stringify(rawSpec));
}

const written = JSON.parse(readFileSync(rawOut, "utf8"));
console.log(`\nraw spec: ${rawOut}`);
console.log(`  name=${written.name} id=${written.id} para_id=${written.para_id} relay=${written.relay_chain}`);
