import { strict as assert } from "node:assert";
import { test } from "node:test";
import { blockRanges, classifyReleaseFailure, toPalletAmount } from "./checks.js";

test("a cap-deferred release retries quietly — the case this service exists for", () => {
	assert.equal(classifyReleaseFailure("ExceedsDailyCap(1000, 0)"), "retry");
	assert.equal(classifyReleaseFailure("EnforcedPause()"), "retry");
	assert.equal(classifyReleaseFailure("InsufficientVaultBalance()"), "retry");
	assert.equal(classifyReleaseFailure("NotEnoughApprovals(2, 3)"), "retry");
});

test("a consumed nonce is done, not an error", () => {
	assert.equal(classifyReleaseFailure("NonceAlreadyConsumed(42)"), "done");
});

test("the per-release ceiling cannot self-heal and is surfaced, not swallowed", () => {
	assert.equal(classifyReleaseFailure("ExceedsPerReleaseCap(5000, 3000)"), "blocked");
});

test("anything unmodelled alerts a human", () => {
	assert.equal(classifyReleaseFailure("TokenNotSet()"), "unexpected");
	assert.equal(classifyReleaseFailure("connection reset"), "unexpected");
});

test("tokenAmount converts back to the exact palletAmount", () => {
	assert.equal(toPalletAmount(5_000_000_000_000_000_000n, 1_000_000n), 5_000_000_000_000n);
});

test("a non-exact conversion throws rather than truncating into a wrong payload hash", () => {
	assert.throws(() => toPalletAmount(1_000_001n, 1_000_000n), /not a multiple/);
	assert.throws(() => toPalletAmount(10n, 0n), /invalid conversionFactor/);
});

test("block ranges are chunked for RPC log limits, inclusive and gapless", () => {
	assert.deepEqual(blockRanges(1n, 10n, 4n), [
		[1n, 4n],
		[5n, 8n],
		[9n, 10n],
	]);
	// A single block still produces one range.
	assert.deepEqual(blockRanges(7n, 7n, 100n), [[7n, 7n]]);
	// Nothing new to scan.
	assert.deepEqual(blockRanges(11n, 10n, 100n), []);
});
