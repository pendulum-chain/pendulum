/**
 * Unit tests for the attestor's tuple-classification predicate.
 *
 * Run with `npm test` (compiles, then `node --test`). These lock in the round-6
 * fix: `isUnreleasable` must flag BOTH the zero address and the vault's own
 * address (case-insensitively), so a `migrate(_, <vault address>)` event is
 * skipped with a critical alert instead of crash-looping the whole attestor
 * fleet on the vault's `RecipientIsVault` revert.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { HttpRequestError, TimeoutError } from "viem";
import { isTransientRpcError, isUnreleasable, ZERO_ADDRESS } from "./checks.js";

const VAULT = "0x1111111111111111111111111111111111111111";
const NORMAL = "0x00000000000000000000000000000000deadbeef";
const AMOUNT = 5_000_000_000_000n; // 5 PEN in 12-decimal pallet units

test("a normal recipient with a non-zero amount is releasable", () => {
	assert.equal(isUnreleasable(NORMAL, AMOUNT, VAULT), false);
});

test("the zero address is unreleasable (vault reverts ZeroAddress)", () => {
	assert.equal(isUnreleasable(ZERO_ADDRESS, AMOUNT, VAULT), true);
});

test("the vault's own address is unreleasable (vault reverts RecipientIsVault)", () => {
	// The round-6 finding: without this, a 1-PEN migration to the vault address
	// crash-loops the entire fleet, since the pallet cannot reject it.
	assert.equal(isUnreleasable(VAULT, AMOUNT, VAULT), true);
});

test("the vault address is matched case-insensitively", () => {
	// The event decodes the H160 lower-cased; the configured vault address may
	// be EIP-55 checksummed. The comparison must not depend on casing either way.
	const vaultChecksummed = "0xAbCdEf0000000000000000000000000000000001";
	assert.equal(isUnreleasable(vaultChecksummed.toLowerCase(), AMOUNT, vaultChecksummed), true);
	assert.equal(isUnreleasable(vaultChecksummed, AMOUNT, vaultChecksummed.toLowerCase()), true);
});

test("a zero amount is unreleasable (vault reverts ZeroAmount)", () => {
	assert.equal(isUnreleasable(NORMAL, 0n, VAULT), true);
});

test("an unreleasable condition still wins when combined with a normal one", () => {
	assert.equal(isUnreleasable(ZERO_ADDRESS, 0n, VAULT), true);
	assert.equal(isUnreleasable(VAULT, 0n, VAULT), true);
});

test("a fatal error whose label embeds nonce 429/502 stays fatal", () => {
	// Nonces are sequential, so 429, 502, 503 and 504 all occur. An error text
	// embedding them (every submission error carries the tuple label) must not
	// be reclassified as an endpoint failure — the post-drills L1 class.
	assert.equal(
		isTransientRpcError(new Error("approve transaction reverted: 0xabc (nonce=429 recipient=0x11 amount=1000)")),
		false,
	);
	assert.equal(isTransientRpcError(new Error("unexpected state for nonce=502")), false);
});

test("HTTP-status endpoint failures are transient, matched structurally", () => {
	assert.equal(isTransientRpcError(new HttpRequestError({ url: "https://rpc", status: 429 })), true);
	assert.equal(isTransientRpcError(new HttpRequestError({ url: "https://rpc", status: 503 })), true);
	assert.equal(isTransientRpcError(new HttpRequestError({ url: "https://rpc", status: 403 })), false);
});

test("transient causes are found anywhere in the error chain", () => {
	const wrapped = new Error("request failed", { cause: new HttpRequestError({ url: "https://rpc", status: 502 }) });
	assert.equal(isTransientRpcError(wrapped), true);
	assert.equal(isTransientRpcError(new TimeoutError({ body: {}, url: "https://rpc" })), true);
});

test("socket-level failures are transient via their structured code", () => {
	const refused = new Error("connect failed") as Error & { code: string };
	refused.code = "ECONNREFUSED";
	assert.equal(isTransientRpcError(refused), true);
});

test("text-only transport failures (polkadot-js) are still recognized", () => {
	assert.equal(isTransientRpcError(new Error("WebSocket is not connected")), true);
	assert.equal(isTransientRpcError(new Error("disconnected from wss://node:443: 1006")), true);
	assert.equal(isTransientRpcError(new Error("fetch failed")), true);
	assert.equal(isTransientRpcError(new Error("socket hang up")), true);
	// A safe-block state read outside the node's retained window heals as the
	// safe head catches up; it must wait, not exit.
	assert.equal(isTransientRpcError(new Error("missing trie node 0xabc (path ) <nil>")), true);
});

test("a decode failure is never transient", () => {
	assert.equal(isTransientRpcError(new Error("MigrationInitiated in block 7 has 5 fields, expected 4")), false);
});
