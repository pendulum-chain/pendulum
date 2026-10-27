import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
	ContractFunctionRevertedError,
	encodeErrorResult,
	HttpRequestError,
	InvalidInputRpcError,
	InvalidParamsRpcError,
	LimitExceededRpcError,
	RpcRequestError,
	TimeoutError,
} from "viem";
import { chunks, classifyReleaseFailure, contractErrorName, isRangeTooLarge, readInRanges, toPalletAmount } from "./checks.js";
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

/** An endpoint's refusal of a log range, as viem raises a JSON-RPC error. */
const rpcError = (code: number, message: string) => new RpcRequestError({ body: {}, error: { code, message }, url: "http://rpc" });

test("an eth_getLogs refusal for the size of its range is recognised, other failures are not", () => {
	// Base's public endpoints answer it with HTTP 400.
	const base = { code: -32602, message: "block range greater than 200 max" };
	assert.equal(isRangeTooLarge(new HttpRequestError({ url: "http://rpc", status: 400, details: JSON.stringify(base) })), true);
	assert.equal(isRangeTooLarge(new InvalidParamsRpcError(rpcError(base.code, base.message))), true);
	for (const message of [
		"exceed maximum block range: 50000",
		"query exceeds max block range 100000",
		"eth_getLogs is limited to a 10,000 range",
		"query returned more than 10000 results",
		"Log response size exceeded.",
	]) {
		assert.equal(isRangeTooLarge(new InvalidInputRpcError(rpcError(-32000, message))), true, message);
	}
	assert.equal(isRangeTooLarge(new HttpRequestError({ url: "http://rpc", status: 429, details: "Too Many Requests" })), false);
	assert.equal(isRangeTooLarge(new LimitExceededRpcError(rpcError(-32005, "request limit reached"))), false);
	assert.equal(isRangeTooLarge(new TimeoutError({ body: {}, url: "http://rpc" })), false);
});

test("log ranges are read in order and gapless, shrinking while the endpoint refuses their size and growing back to the maximum", async () => {
	let cap = 200n;
	const reads: [bigint, bigint][] = [];
	const read = async (start: bigint, end: bigint) => {
		if (end - start + 1n > cap) throw new InvalidParamsRpcError(rpcError(-32602, `block range greater than ${cap} max`));
		reads.push([start, end]);
	};
	const span = { size: 10_000n, max: 10_000n, streak: 0 };
	await readInRanges(1n, 500n, span, read);
	// 10,000 halved until accepted.
	assert.deepEqual(reads, [
		[1n, 156n],
		[157n, 312n],
		[313n, 468n],
		[469n, 500n],
	]);
	// Ten full-size reads in a row try twice the size again, refused here.
	reads.length = 0;
	await readInRanges(501n, 2060n, span, read);
	assert.deepEqual(
		reads.map(([start, end]) => end - start + 1n),
		Array(10).fill(156n),
	);
	assert.equal(reads.at(-1)?.[1], 2060n);
	assert.equal(span.size, 156n);
	// On an endpoint without the cap it grows back, never above the maximum.
	cap = 1_000_000n;
	reads.length = 0;
	await readInRanges(2061n, 1_000_000n, span, read);
	const sizes = reads.map(([start, end]) => end - start + 1n);
	assert.ok(sizes.every((size, i) => size <= 10_000n && (i === 0 || reads[i][0] === reads[i - 1][1] + 1n)));
	assert.equal(span.size, 10_000n);
	assert.equal(sizes.at(-2), 10_000n);
	// Nothing new to scan.
	reads.length = 0;
	await readInRanges(11n, 10n, span, read);
	assert.deepEqual(reads, []);
});

test("a refused single block and any other failure propagate", async () => {
	const refused = async () => {
		throw new InvalidParamsRpcError(rpcError(-32602, "block range greater than 0 max"));
	};
	const tried: bigint[] = [];
	const span = { size: 4n, max: 4n, streak: 0 };
	await assert.rejects(
		readInRanges(1n, 10n, span, async (start, end) => {
			tried.push(end - start + 1n);
			await refused();
		}),
		InvalidParamsRpcError,
	);
	assert.deepEqual(tried, [4n, 2n, 1n]);
	const unchanged = { size: 100n, max: 100n, streak: 0 };
	await assert.rejects(
		readInRanges(1n, 10n, unchanged, async () => {
			throw new HttpRequestError({ url: "http://rpc", status: 503 });
		}),
		HttpRequestError,
	);
	assert.equal(unchanged.size, 100n, "not a reason to shrink");
});

test("an amount larger than the daily cap itself is blocked, not a refill wait", () => {
	// The releaser synthesizes this name when ExceedsDailyCap is decoded for an
	// amount above dailyCap: the allowance can never reach it, so it needs a
	// governance setCaps rather than quiet retries.
	assert.equal(classifyReleaseFailure("ExceedsDailyCapPermanently"), "blocked");
	assert.equal(classifyReleaseFailure("ExceedsDailyCap"), "retry");
});
