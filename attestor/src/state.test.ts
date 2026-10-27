import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
	assertBaseChain,
	assertCheckpointIdentity,
	assertPendulumGenesis,
	loadCheckpoint,
	saveCheckpoint,
	type Checkpoint,
} from "./state.js";

const checkpoint = {
	version: 1,
	baseChainId: 8453,
	vaultAddress: "0x1111111111111111111111111111111111111111",
	pendulumGenesisHash: `0x${"22".repeat(32)}`,
	attestorAddress: "0x4444444444444444444444444444444444444444",
	attestorGeneration: 1,
	lastProcessedBlock: 42,
} satisfies Checkpoint;

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

test("a checkpoint made by another key or an earlier generation of this key is refused", () => {
	// A replacement key resuming its predecessor's progress would never approve
	// the unreleased migrations before it.
	assert.throws(
		() =>
			assertCheckpointIdentity(checkpoint, {
				...checkpoint,
				attestorAddress: "0x5555555555555555555555555555555555555555",
			}),
		/belongs to attestor .* start with no checkpoint/,
	);
	// The address is matched case-insensitively.
	assert.doesNotThrow(() =>
		assertCheckpointIdentity(checkpoint, { ...checkpoint, attestorAddress: checkpoint.attestorAddress.toUpperCase() }),
	);
	// A removed and re-added key: its approvals from generation 1 stopped counting.
	assert.throws(
		() => assertCheckpointIdentity(checkpoint, { ...checkpoint, attestorGeneration: 2 }),
		/generation 1, .* lower lastProcessedBlock .* set attestorGeneration to 2/,
	);
});

test("a checkpoint from before the signer was recorded still loads", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-attestor-state-"));
	const path = join(directory, "checkpoint.json");
	try {
		const { attestorAddress, attestorGeneration, ...legacy } = checkpoint;
		writeFileSync(path, JSON.stringify(legacy));
		const loaded = loadCheckpoint(path);
		assert.deepEqual(loaded, legacy);
		assert.doesNotThrow(() => assertCheckpointIdentity(loaded!, { ...legacy, attestorAddress, attestorGeneration }));
		writeFileSync(path, JSON.stringify({ ...checkpoint, attestorGeneration: 0 }));
		assert.throws(() => loadCheckpoint(path), /invalid or legacy schema/);
		writeFileSync(path, JSON.stringify({ ...checkpoint, attestorAddress: [checkpoint.attestorAddress] }));
		assert.throws(() => loadCheckpoint(path), /invalid or legacy schema/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});

test("a checkpoint with only one of the signer fields is refused, not taken for one from before they existed", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-attestor-state-"));
	const path = join(directory, "checkpoint.json");
	try {
		const { attestorAddress, attestorGeneration, ...legacy } = checkpoint;
		writeFileSync(path, JSON.stringify({ ...legacy, attestorAddress }));
		assert.throws(
			() => assertCheckpointIdentity(loadCheckpoint(path)!, { ...checkpoint, attestorGeneration: 2 }),
			/has attestorAddress but no attestorGeneration; .*\(runbook RB-6\)/,
		);
		writeFileSync(path, JSON.stringify({ ...legacy, attestorGeneration }));
		assert.throws(
			() =>
				assertCheckpointIdentity(loadCheckpoint(path)!, {
					...checkpoint,
					attestorAddress: "0x5555555555555555555555555555555555555555",
				}),
			/has attestorGeneration but no attestorAddress/,
		);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});

test("the Base endpoint must serve the configured chain and the Pendulum node the pinned genesis", () => {
	assert.doesNotThrow(() => assertBaseChain(8453, 8453));
	assert.throws(() => assertBaseChain(84532, 8453), /BASE_RPC_URL serves chain 84532, but BASE_CHAIN_ID is 8453/);
	const pendulum = `0x${"5d".repeat(32)}`;
	assert.doesNotThrow(() => assertPendulumGenesis(pendulum.toUpperCase().replace("0X", "0x"), pendulum));
	assert.throws(
		() => assertPendulumGenesis(`0x${"aa".repeat(32)}`, pendulum),
		/PENDULUM_WS serves the chain with genesis 0xaaaa.*, but PENDULUM_GENESIS_HASH is 0x5d5d/,
	);
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
