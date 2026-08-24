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
import { isUnreleasable, ZERO_ADDRESS } from "./checks.js";

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
