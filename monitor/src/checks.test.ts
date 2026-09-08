/**
 * Unit tests for the monitor's conservation/liveness predicates.
 *
 * Run with `npm test` (compiles, then `node --test`). These lock in the
 * round-5 fixes: the M2b check must ignore a token surplus (donation / vault
 * recipient) and only fire on a real deficit.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
	approvalMismatch,
	blockRanges,
	chunks,
	isStale,
	provablyUnsourced,
	releaseMismatch,
	releasedExceedsMigrated,
	shouldAwaitSource,
	vaultConservationDeficit,
} from "./checks.js";

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

test("an unseen future source nonce gets bounded RPC-lag grace, never an old nonce", () => {
	const now = 10_000_000;
	assert.equal(shouldAwaitSource(8n, 8n, now, now, 600), true);
	assert.equal(shouldAwaitSource(9n, 8n, now - 599_000, now, 600), true);
	assert.equal(shouldAwaitSource(9n, 8n, now - 601_000, now, 600), false);
	assert.equal(shouldAwaitSource(7n, 8n, now, now, 600), false);
});

test("a fabricated event is proven unsourced once the source view passes its first observation", () => {
	const firstSeen = 10_000_000;
	const margin = 120_000;
	const now = firstSeen + 600_000;
	// Source finalized view has moved past the moment the event was first
	// observed (plus skew margin): the burn would already have been ingested —
	// fabricated, pause without waiting out the grace.
	assert.equal(provablyUnsourced(firstSeen, firstSeen + margin, now, margin), true);
	assert.equal(provablyUnsourced(firstSeen, firstSeen + margin + 1, now, margin), true);
	// A lagging source view trails the observation: NOT proof, so the RPC-lag
	// grace applies and node lag can never fast-path a false pause.
	assert.equal(provablyUnsourced(firstSeen, firstSeen + margin - 1, now, margin), false);
	assert.equal(provablyUnsourced(firstSeen, firstSeen - 300_000, now, margin), false);
});

test("a future-dated source view proves nothing", () => {
	// A collator clock ahead of real time must not let the view "pass" an
	// observation it has not genuinely caught up with.
	const firstSeen = 10_000_000;
	const margin = 120_000;
	const now = firstSeen + 10_000;
	assert.equal(provablyUnsourced(firstSeen, now + margin + 1, now, margin), false);
	assert.equal(provablyUnsourced(firstSeen, now + margin, now, margin), true);
});

test("the proof anchor is the observation time, not a Base block timestamp that may trail real time", () => {
	// An event observed now whose Base block timestamp trails real time by a
	// sequencer catch-up: the view being past that OLD timestamp is not proof
	// (the predicate never sees it), only being past the observation is.
	const firstSeen = 10_000_000;
	const margin = 120_000;
	assert.equal(provablyUnsourced(firstSeen, firstSeen + 60_000, firstSeen + 60_000, margin), false);
});

test("M2 tuple matching rejects missing, wrong-recipient and wrong-amount approvals", () => {
	const expected = { nonce: 7n, recipient: "0x1111111111111111111111111111111111111111", palletAmount: 100n };
	assert.match(approvalMismatch(undefined, expected)!, /no finalized/);
	assert.match(
		approvalMismatch(expected, { ...expected, recipient: "0x2222222222222222222222222222222222222222" })!,
		/recipient/,
	);
	assert.match(approvalMismatch(expected, { ...expected, palletAmount: 99n })!, /pallet amount/);
	assert.equal(approvalMismatch(expected, { ...expected, recipient: expected.recipient.toUpperCase() }), undefined);
});

test("M2 release matching includes the one-and-only decimal conversion", () => {
	const expected = { nonce: 7n, recipient: "0x1111111111111111111111111111111111111111", palletAmount: 100n };
	assert.equal(releaseMismatch(expected, { ...expected, tokenAmount: 100n * CF }, CF), undefined);
	assert.match(releaseMismatch(expected, { ...expected, tokenAmount: 100n * CF + 1n }, CF)!, /token amount/);
});

test("RPC work is split into inclusive block ranges and bounded batches", () => {
	assert.deepEqual(blockRanges(1n, 10n, 4n), [[1n, 4n], [5n, 8n], [9n, 10n]]);
	assert.deepEqual(blockRanges(11n, 10n, 4n), []);
	assert.deepEqual(chunks([1, 2, 3, 4, 5], 2), [[1, 2], [3, 4], [5]]);
});
