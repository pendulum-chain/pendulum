import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { assertCheckpointIdentity, loadCheckpoint, saveCheckpoint, type Checkpoint } from "./state.js";

const checkpoint: Checkpoint = {
	version: 1,
	baseChainId: 8453,
	vaultAddress: "0x1111111111111111111111111111111111111111",
	pendulumGenesisHash: `0x${"22".repeat(32)}`,
	lastProcessedBlock: 42,
};

test("checkpoint replacement is readable and identity-bound", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-attestor-state-"));
	const path = join(directory, "checkpoint.json");
	try {
		assert.equal(loadCheckpoint(path), undefined);
		saveCheckpoint(path, checkpoint);
		const loaded = loadCheckpoint(path);
		assert.deepEqual(loaded, checkpoint);
		assert.doesNotThrow(() => assertCheckpointIdentity(loaded!, checkpoint));
		assert.throws(
			() => assertCheckpointIdentity(loaded!, { ...checkpoint, baseChainId: 84532 }),
			/does not match/,
		);
		assert.throws(
			() => assertCheckpointIdentity(loaded!, { ...checkpoint, pendulumGenesisHash: `0x${"33".repeat(32)}` }),
			/Pendulum genesis/,
		);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});

test("a malformed checkpoint fails loudly instead of resetting progress", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-attestor-state-"));
	const path = join(directory, "checkpoint.json");
	try {
		writeFileSync(path, "{truncated");
		assert.throws(() => loadCheckpoint(path), /refusing to reset progress/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});
