import { strict as assert } from "node:assert";
import { test } from "node:test";
import { ContractFunctionRevertedError, encodeErrorResult } from "viem";
import { blockRanges, chunks, classifyReleaseFailure, contractErrorName, toPalletAmount } from "./checks.js";
import { vaultAbi } from "./vaultAbi.js";

test("a cap-deferred release retries quietly — the case this service exists for", () => {
	assert.equal(classifyReleaseFailure("ExceedsDailyCap"), "retry");
	assert.equal(classifyReleaseFailure("EnforcedPause"), "retry");
	assert.equal(classifyReleaseFailure("NotEnoughApprovals"), "retry");
	assert.equal(classifyReleaseFailure("PendingFinality"), "retry");
});

test("a consumed nonce is done, not an error", () => {
	assert.equal(classifyReleaseFailure("NonceAlreadyConsumed"), "done");
});

test("the per-release ceiling cannot self-heal and is surfaced, not swallowed", () => {
	assert.equal(classifyReleaseFailure("ExceedsPerReleaseCap"), "blocked");
	assert.equal(classifyReleaseFailure("InsufficientVaultBalance"), "blocked");
});

test("anything unmodelled alerts a human", () => {
	assert.equal(classifyReleaseFailure("TokenNotSet"), "unexpected");
	assert.equal(classifyReleaseFailure(undefined), "unexpected");
});

test("viem decodes the real vault error ABI before classification", () => {
	const data = encodeErrorResult({ abi: vaultAbi, errorName: "ExceedsDailyCap", args: [1000n, 0n] });
	const error = new ContractFunctionRevertedError({ abi: vaultAbi, data, functionName: "release" });
	assert.equal(contractErrorName(error), "ExceedsDailyCap");
	assert.equal(classifyReleaseFailure(contractErrorName(error)), "retry");
});

test("individual fallbacks are split into bounded chunks", () => {
	assert.deepEqual(chunks([1, 2, 3, 4, 5], 2), [[1, 2], [3, 4], [5]]);
	assert.throws(() => chunks([1], 0), /invalid chunk size/);
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

test("an amount larger than the daily cap itself is blocked, not a refill wait", () => {
	// The releaser synthesizes this name when ExceedsDailyCap is decoded for an
	// amount above dailyCap: the allowance can never reach it, so it needs a
	// governance setCaps rather than quiet retries.
	assert.equal(classifyReleaseFailure("ExceedsDailyCapPermanently"), "blocked");
	assert.equal(classifyReleaseFailure("ExceedsDailyCap"), "retry");
});
