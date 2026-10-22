/**
 * Unit tests for the monitor's conservation/liveness predicates.
 *
 * Run with `npm test` (compiles, then `node --test`). Among others, these lock
 * in that the M2b check ignores a token surplus (donation / vault recipient)
 * and only fires on a real deficit.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
	approvalMismatch,
	blockRanges,
	chunks,
	describeCall,
	describeTimelockOperation,
	disagreeingAttestors,
	isStale,
	provablyUnsourced,
	releaseMismatch,
	releasedExceedsMigrated,
	riskyCalls,
	shouldAwaitSource,
	slackText,
	toCallNode,
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

test("M2a: an acknowledged loss is tolerated, and nothing beyond it", () => {
	assert.equal(releasedExceedsMigrated(6n * CF, 5n, CF, CF), false);
	assert.equal(releasedExceedsMigrated(6n * CF + 1n, 5n, CF, CF), true);
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

test("approvals against the burn count once per attestor and only while they disagree", () => {
	const burn = { nonce: 7n, recipient: "0x1111111111111111111111111111111111111111", palletAmount: 100n };
	const [a, b] = ["0xA1a1a1a1a1A1a1a1a1a1a1A1a1a1a1a1a1a1A1a1", "0xa2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2"];
	const approvals = [
		{ ...burn, palletAmount: 99n, attestor: a },
		{ ...burn, palletAmount: 98n, attestor: a.toLowerCase() },
		{ ...burn, attestor: b }, // agrees
		{ ...burn, nonce: 8n, palletAmount: 99n, attestor: b }, // another nonce
	];
	assert.deepEqual(disagreeingAttestors(burn, 7n, approvals), [a.toLowerCase()]);
	assert.deepEqual(disagreeingAttestors(undefined, 7n, approvals), [a.toLowerCase(), b]); // no burn known
	assert.deepEqual(disagreeingAttestors({ ...burn, palletAmount: 99n }, 7n, approvals), [a.toLowerCase(), b]);
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

test("a scheduled governance operation is described with names, targets and the veto deadline", () => {
	const vault = "0x2222222222222222222222222222222222222222";
	const labels = { [vault]: "vault" };
	const functions = { "0x0e18b681": "acceptAdmin()", "0x212bf316": "setCaps(uint256,uint256)" };
	const calls = [
		{ index: 1n, target: vault.toUpperCase().replace("0X", "0x"), value: 0n, data: "0x212bf316" + "00".repeat(64) },
		{ index: 0n, target: vault, value: 0n, data: "0x0e18b681" },
	];
	const now = Date.UTC(2026, 9, 21, 10, 0, 0);
	const text = describeTimelockOperation("0xabc", calls, BigInt((now + (48 * 60 + 5) * 60_000) / 1000), now, labels, functions);
	assert.match(text, /^operation 0xabc: #0 vault\.acceptAdmin\(\) data=0x0e18b681; #1 vault\.setCaps\(uint256,uint256\)/);
	assert.match(text, /executable by anyone from 2026-10-23T10:05:00\.000Z \(in 48h 5m\)/);
	assert.match(text, /canceller must cancel it before then$/);
});

test("a scheduled operation with unknown parts or a settled timestamp still pages readably", () => {
	const other = "0x3333333333333333333333333333333333333333";
	const call = { index: 0n, target: other, value: 5n, data: "0xdeadbeef" };
	assert.match(describeTimelockOperation("0x1", [call], 0n, 0, {}, {}), new RegExp(`#0 ${other}\\.unknown function 0xdeadbeef value=5 .*cancelled\\)$`));
	assert.match(describeTimelockOperation("0x1", [call], 1n, 0, {}, {}), /already executed$/);
	assert.match(describeTimelockOperation("0x1", [call], 100n, 200_000, {}, {}), /\(ready now\)/);
});

test("a scheduled operation pages whatever its delay or its event carries", () => {
	const call = { index: 0n, target: "0x3333333333333333333333333333333333333333", value: 0n, data: "0x8456cb59" };
	assert.match(describeTimelockOperation("0x1", [call], 10n ** 20n, 0, {}, {}), /from timestamp 100000000000000000000 \(in /);
	const undecoded = { index: undefined, target: undefined, value: undefined, data: undefined } as unknown as typeof call;
	assert.match(describeTimelockOperation("0x1", [undecoded, call], 5n, 0, {}, {}), /^operation 0x1: #undefined undefined\.unknown function undefined/);
});

test("a page is Slack text with its control characters escaped and a long detail cut short", () => {
	assert.equal(
		slackText("pen-monitor", "critical", "SUBJECT", "to <!channel> & <https://x.example|click>"),
		"[CRITICAL] pen-monitor: SUBJECT\nto &lt;!channel&gt; &amp; &lt;https://x.example|click&gt;",
	);
	const text = slackText("pen-monitor", "warning", "SUBJECT", "a".repeat(10_000));
	assert.ok(text.startsWith(`[WARNING] pen-monitor: SUBJECT\n${"a".repeat(3_000)}… (cut short`), text.slice(0, 80));
	assert.ok(text.length < 3_200, String(text.length));
});

test("a governance call tree is walked through batches and minting calls are flagged", () => {
	const setBalance = { section: "tokens", method: "setBalance", args: ["who", "Native", 4_000_000n] };
	const batch = toCallNode({ section: "utility", method: "batch", args: [[setBalance, setBalance]] });
	assert.equal(describeCall(batch), "utility.batch(tokens.setBalance, tokens.setBalance)");
	assert.deepEqual(riskyCalls(batch), ["tokens.setBalance"]);
	const nested = toCallNode({ section: "utility", method: "dispatchAs", args: ["Root", { section: "tokenMigration", method: "setPaused", args: [false] }] });
	assert.deepEqual(riskyCalls(nested), ["tokenMigration.setPaused"]);
	assert.deepEqual(riskyCalls(toCallNode({ section: "system", method: "remark", args: ["0x00"] })), []);
});

test("the call-tree walk is bounded in size and depth", () => {
	const remark = { section: "system", method: "remark", args: [] };
	const batch = (calls: unknown[]) => ({ section: "utility", method: "batch", args: [calls] });
	assert.equal(toCallNode(batch(Array(255).fill(remark))).calls.length, 255);
	assert.throws(() => toCallNode(batch(Array(256).fill(remark))), /more than 256 calls/);
	let nested: { section: string; method: string; args: unknown[] } = remark;
	for (let depth = 1; depth < 16; depth++) nested = batch([nested]);
	assert.equal(describeCall(toCallNode(nested)).split("(").length, 16);
	assert.throws(() => toCallNode(batch([nested])), /more than 16 levels/);
});
