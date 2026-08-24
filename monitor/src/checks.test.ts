/**
 * Unit tests for the monitor's conservation/liveness predicates.
 *
 * Run with `npm test` (compiles, then `node --test`). These lock in the
 * round-5 fixes: the M2b check must ignore a token surplus (donation / vault
 * recipient) and only fire on a real deficit.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { isStale, newNonces, releasedExceedsMigrated, vaultConservationDeficit } from "./checks.js";

const CF = 1_000_000n; // 12 -> 18 decimals
const SUPPLY = 150_000_000n * 10n ** 18n;

test("M2a: released within migrated does not fire", () => {
	assert.equal(releasedExceedsMigrated(5n * CF, 5n, CF), false);
	assert.equal(releasedExceedsMigrated(5n * CF, 10n, CF), false); // migrated lags releases via finality: fine
});

test("M2a: released exceeding migrated fires", () => {
	assert.equal(releasedExceedsMigrated(6n * CF, 5n, CF), true);
});

test("M2b: exact conservation is not a deficit", () => {
	// balance + released + swept == totalSupply
	assert.equal(vaultConservationDeficit(SUPPLY - 30n, 20n, 10n, SUPPLY), false);
});

test("M2b: a surplus (donation / vault-recipient self-transfer) must NOT fire", () => {
	// Someone transferred dust into the vault: balance is higher than the
	// identity predicts, so the sum EXCEEDS totalSupply. This is harmless and
	// must never trip the check (previously a strict `!=` paused the vault here).
	const donation = 5n * 10n ** 18n;
	assert.equal(vaultConservationDeficit(SUPPLY - 30n + donation, 20n, 10n, SUPPLY), false);
	// Even a 1-wei donation must not fire.
	assert.equal(vaultConservationDeficit(SUPPLY + 1n, 0n, 0n, SUPPLY), false);
});

test("M2b: a real deficit (tokens vanished without a counter update) fires", () => {
	// balance too low for the recorded released+swept: genuine loss.
	assert.equal(vaultConservationDeficit(SUPPLY - 100n, 20n, 10n, SUPPLY), true);
});

test("M4: staleness respects the grace period", () => {
	const grace = 1800; // seconds
	const now = 10_000_000;
	assert.equal(isStale(now - 1000, now, grace), false); // 1s old
	assert.equal(isStale(now - 1800 * 1000, now, grace), false); // exactly at grace: not yet stale
	assert.equal(isStale(now - 1801 * 1000, now, grace), true); // just past grace
});

test("M4: newNonces incorporates each nonce exactly once (no re-add of consumed)", () => {
	// Round-7 regression: the old scan re-added every nonce 0..nextNonce each
	// poll, so a consumed-and-pruned nonce was re-read via Multicall forever,
	// making the scan O(all migrations) instead of O(pending backlog).
	const firstSeen = new Map<bigint, number>();
	let incorporated = 0n;

	// Poll 1: five migrations exist — all freshly stamped.
	for (const nonce of newNonces(incorporated, 5n)) firstSeen.set(nonce, 1_000);
	incorporated = 5n;
	assert.deepEqual([...firstSeen.keys()], [0n, 1n, 2n, 3n, 4n]);

	// Nonces 0,1,2 get consumed on Base and are pruned from the pending set.
	firstSeen.delete(0n);
	firstSeen.delete(1n);
	firstSeen.delete(2n);

	// Poll 2: nextNonce unchanged — nothing to incorporate, and the consumed
	// nonces must NOT reappear (the bug that this fix closes).
	assert.deepEqual(newNonces(incorporated, 5n), []);
	assert.deepEqual([...firstSeen.keys()], [3n, 4n]);

	// Poll 3: two new migrations arrive — only those are freshly incorporated.
	for (const nonce of newNonces(incorporated, 7n)) firstSeen.set(nonce, 2_000);
	incorporated = 7n;
	assert.deepEqual([...firstSeen.keys()], [3n, 4n, 5n, 6n]);
});
