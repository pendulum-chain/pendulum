import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { assertStateIdentity, loadState, saveState, type PersistedState } from "./state.js";

const state: PersistedState = {
	version: 1,
	baseChainId: 8453,
	vaultAddress: "0x1111111111111111111111111111111111111111",
	fromBlock: "123",
	pending: [{ nonce: "7", recipient: "0x2222222222222222222222222222222222222222", palletAmount: "99" }],
};

test("releaser state is atomically persisted and identity-bound", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-releaser-state-"));
	const path = join(directory, "state.json");
	try {
		assert.equal(loadState(path), undefined);
		saveState(path, state);
		const loaded = loadState(path);
		assert.deepEqual(loaded, state);
		assert.doesNotThrow(() => assertStateIdentity(loaded!, state));
		assert.throws(() => assertStateIdentity(loaded!, { ...state, baseChainId: 1 }), /does not match/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});

test("corrupt releaser state is fatal", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-releaser-state-"));
	const path = join(directory, "state.json");
	try {
		writeFileSync(path, "{");
		assert.throws(() => loadState(path), /refusing to reset progress/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});
