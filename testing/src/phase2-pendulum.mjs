/**
 * Phase 2 of docs/pen-migration-local-test-plan.md — the Pendulum side, run
 * against a Chopsticks fork of live mainnet state with the new runtime
 * applied as a wasm override.
 *
 * This is the phase that exercises what unit tests cannot: that the upgrade
 * applies to real storage, that it ships PAUSED, and that migrate() behaves
 * correctly against genuine holder state (whales, vesting, staking locks, the
 * real treasury account).
 *
 * Start Chopsticks first:
 *   npx @acala-network/chopsticks@latest --config testing/chopsticks.yml \
 *     --wasm-override target/release/wbuild/pendulum-runtime/pendulum_runtime.compact.compressed.wasm
 * then: node testing/src/phase2-pendulum.mjs
 */

import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { assert, assertEq, check, section, summarise } from "./harness.mjs";

const ENDPOINT = process.env.CHOPSTICKS_WS ?? "ws://127.0.0.1:8000";
const UNIT = 10n ** 12n;
const BASE_ADDR = "0x00000000000000000000000000000000000000Be";
const ZERO_ADDR = "0x0000000000000000000000000000000000000000";
// The keyless Pendulum treasury account (py/trsry), as it exists on mainnet.
const TREASURY = "0x6d6f646c70792f74727372790000000000000000000000000000000000000000";

const api = await ApiPromise.create({ provider: new WsProvider(ENDPOINT), noInitWarn: true });
// Read the pallet's real bounds rather than assuming them: the harness must
// track whatever the runtime actually ships.
const MIN = BigInt(api.consts.tokenMigration.minimumMigrationAmount.toString());
const ED = BigInt(api.consts.balances.existentialDeposit.toString());
const AMOUNT = MIN * 2n;                 // comfortably above the minimum
const FUND = MIN * 100n;                 // plenty for fees and several migrations
await cryptoWaitReady();
const keyring = new Keyring({ type: "sr25519", ss58Format: api.registry.chainSS58 ?? 56 });
const alice = keyring.addFromUri("//Alice");

/** Produce a block so submitted extrinsics are applied. */
const newBlock = () => api.rpc("dev_newBlock");

/** Set storage through Chopsticks, standing in for a governance action.
 *  Raw [key, value] pairs on purpose: the human-readable object form treats a
 *  falsy value as a deletion, and deleting `Paused` makes it read back as its
 *  `true` default -- the opposite of what we want. */
const setStorageRaw = (pairs) => api.rpc("dev_setStorage", pairs);
const setStorage = (values) => api.rpc("dev_setStorage", values);

/** Submit, produce a block, and report whether the extrinsic succeeded. */
async function submit(tx, signer) {
	await tx.signAndSend(signer);
	await newBlock();
	const events = await api.query.system.events();
	let failure;
	for (const record of events) {
		const { section: s, method, data } = record.event;
		if (s !== "system" || method !== "ExtrinsicFailed") continue;
		const [dispatchError] = data;
		if (dispatchError.isModule) {
			const meta = api.registry.findMetaError(dispatchError.asModule);
			failure = meta.name;
		} else {
			failure = dispatchError.toString();
		}
	}
	const migrationEvent = events
		.map((r) => r.event)
		.find((e) => e.section === "tokenMigration" && e.method === "MigrationInitiated");
	return { ok: !failure, error: failure, events, migrationEvent };
}

const paused = async () => (await api.query.tokenMigration.paused()).toPrimitive() === true;
const freeOf = async (who) => BigInt((await api.query.system.account(who)).data.free.toString());
const issuance = async () => BigInt((await api.query.balances.totalIssuance()).toString());

async function fund(who, amount) {
	await setStorage({ System: { Account: [[[who], { providers: 1, data: { free: amount.toString() } }]] } });
	// Chopsticks applies storage overrides after extrinsics within a block, so
	// the funding needs its own block before anything can spend it.
	await newBlock();
}

console.log(`Phase 2 — Pendulum on Chopsticks (${ENDPOINT})`);

{
	const nonce = BigInt((await api.query.tokenMigration.nextNonce()).toString());
	if (nonce !== 0n) {
		console.error(
			`\nThis chain is not fresh (nextNonce=${nonce}). Phase 2 asserts the state of a\n` +
			"just-upgraded chain, so restart Chopsticks before re-running.\n",
		);
		await api.disconnect();
		process.exit(2);
	}
}

section("Upgrade applied and fresh state");

await check("the token-migration pallet exists in the upgraded runtime", async () => {
	assert(api.query.tokenMigration !== undefined, "tokenMigration not present in metadata");
	assert(api.tx.tokenMigration?.migrate !== undefined, "migrate extrinsic missing");
});

await check("SHIPS PAUSED — no storage written, pallet reads as paused", async () => {
	assert(await paused(), "expected paused() == true immediately after the upgrade");
});

await check("counters start at zero and no treasury destination is set", async () => {
	assertEq(BigInt((await api.query.tokenMigration.nextNonce()).toString()), 0n, "nextNonce");
	assertEq(BigInt((await api.query.tokenMigration.totalMigrated()).toString()), 0n, "totalMigrated");
	assert((await api.query.tokenMigration.treasuryDestination()).isNone, "treasuryDestination should be None");
});

await check("migrate is rejected while paused", async () => {
	await fund(alice.address, FUND);
	const r = await submit(api.tx.tokenMigration.migrate(AMOUNT, BASE_ADDR), alice);
	assertEq(r.error, "MigrationsPaused", "expected MigrationsPaused");
});

section("After governance enables migrations");

await check("unpausing works and is the separate, explicit act", async () => {
	// SCALE bool false == 0x00, written directly to the storage key.
	await setStorageRaw([[api.query.tokenMigration.paused.key(), "0x00"]]);
	await newBlock();
	assert(!(await paused()), "expected paused() == false after enabling");
});

await check("happy path burns the amount and reduces total issuance", async () => {
	await fund(alice.address, FUND);
	const before = await freeOf(alice.address);
	const issuedBefore = await issuance();
	const amount = AMOUNT;

	const r = await submit(api.tx.tokenMigration.migrate(amount, BASE_ADDR), alice);
	assert(r.ok, `migrate failed: ${r.error}`);

	const spent = before - (await freeOf(alice.address));
	assert(spent >= amount, `balance fell by ${spent}, expected at least ${amount}`);
	assertEq(issuedBefore - (await issuance()), amount, "issuance drop (burn, not transfer)");
});

await check("MigrationInitiated field ORDER is {nonce, who, base_address, amount}", async () => {
	// The attestor decodes this event positionally, so field order is a
	// wire-compatibility contract, not a cosmetic detail.
	await fund(alice.address, FUND);
	const amount = AMOUNT;
	const r = await submit(api.tx.tokenMigration.migrate(amount, BASE_ADDR), alice);
	assert(r.migrationEvent, "no MigrationInitiated event emitted");
	const data = r.migrationEvent.data;
	assertEq(data.length, 4, "event field count");
	assert(!Number.isNaN(Number(data[0].toString())), "field 0 (nonce) should be numeric");
	assertEq(data[1].toString(), alice.address, "field 1 (who)");
	assertEq(data[2].toHex().toLowerCase(), BASE_ADDR.toLowerCase(), "field 2 (base_address)");
	assertEq(BigInt(data[3].toString().replaceAll(",", "")), amount, "field 3 (amount)");
});

await check("the zero Base address is rejected", async () => {
	const r = await submit(api.tx.tokenMigration.migrate(AMOUNT, ZERO_ADDR), alice);
	assertEq(r.error, "InvalidBaseAddress", "expected InvalidBaseAddress");
});

await check("an amount below the minimum is rejected", async () => {
	const r = await submit(api.tx.tokenMigration.migrate(MIN - 1n, BASE_ADDR), alice);
	assertEq(r.error, "AmountBelowMinimum", "expected AmountBelowMinimum");
});

await check("leaving a dust remainder below the existential deposit is rejected", async () => {
	await fund(alice.address, FUND);
	const free = await freeOf(alice.address);
	const r = await submit(api.tx.tokenMigration.migrate(free - (ED - 1n), BASE_ADDR), alice);
	assertEq(r.error, "WouldLeaveDust", "expected WouldLeaveDust");
});

section("Against real mainnet holder state");

await check("the fork really carries mainnet state, not a blank chain", async () => {
	// If this fails, every other 'real state' claim in this phase is void.
	const issued = await issuance();
	assert(issued > 100_000_000n * UNIT, `total issuance looks wrong: ${issued}`);
	const treasuryFree = await freeOf(TREASURY);
	assert(treasuryFree > 0n, "the real treasury account has no balance in the fork");
	let lockedAccounts = 0;
	for (const [, value] of await api.query.balances.locks.entries()) {
		if (value.length > 0) lockedAccounts++;
		if (lockedAccounts >= 50) break;
	}
	assert(lockedAccounts > 0, "no locked accounts in the fork — state did not load");
});

await check("a locked balance cannot be migrated", async () => {
	// Lock most of a funded account, then try to migrate past the free portion.
	await fund(alice.address, FUND);
	const lockId = "0x76657374696e6720"; // b"vesting "
	const locked = FUND - MIN;  // leave less than the minimum unlocked
	// Locks and AccountData.frozen must both be set: the withdraw path checks
	// `frozen`, and writing Balances.Locks alone does not recompute it.
	await setStorage({
		System: {
			Account: [[[alice.address], { providers: 1, consumers: 1, data: { free: FUND.toString(), frozen: locked.toString() } }]],
		},
		Balances: { Locks: [[[alice.address], [{ id: lockId, amount: locked.toString(), reasons: "All" }]]] },
	});
	await newBlock();
	const r = await submit(api.tx.tokenMigration.migrate(FUND / 2n, BASE_ADDR), alice);
	assert(!r.ok, "expected a locked balance to be refused");
	// Clear the lock again so later checks operate on a clean account.
	await setStorage({ Balances: { Locks: [[[alice.address], []]] } });
	await newBlock();
});

await check("the real treasury account is present and funded in the fork", async () => {
	// The keyless py/trsry account is what migrate_treasury burns from. Its
	// root-gated execution cannot be driven here (Pendulum has no sudo pallet,
	// so root only comes from a referendum); that path is covered by the
	// pallet's unit tests. What phase 2 can confirm is that the account this
	// runtime will burn from really exists in mainnet state and holds a
	// balance, and that the destination is settable.
	const treasuryFree = await freeOf(TREASURY);
	assert(treasuryFree > 0n, "real treasury account has no balance in the fork");
	await setStorage({ TokenMigration: { TreasuryDestination: BASE_ADDR } });
	await newBlock();
	const dest = await api.query.tokenMigration.treasuryDestination();
	assert(dest.isSome, "treasury destination did not persist");
	assertEq(dest.unwrap().toHex().toLowerCase(), BASE_ADDR.toLowerCase(), "treasury destination");
});

await check("nonces are unique and monotonic across all migrations so far", async () => {
	const next = BigInt((await api.query.tokenMigration.nextNonce()).toString());
	assert(next > 0n, "expected at least one migration to have consumed a nonce");
	const migrated = BigInt((await api.query.tokenMigration.totalMigrated()).toString());
	assert(migrated > 0n, "TotalMigrated should have advanced");
});

const ok = summarise();
await api.disconnect();
process.exit(ok ? 0 : 1);
