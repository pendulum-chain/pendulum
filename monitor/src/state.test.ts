import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
	assertMonitorStateIdentity,
	loadMonitorState,
	saveMonitorState,
	type PersistedMonitorState,
} from "./state.js";

const state: PersistedMonitorState = {
	version: 1,
	baseChainId: 8453,
	vaultAddress: "0x1111111111111111111111111111111111111111",
	pendulumGenesisHash: `0x${"33".repeat(32)}`,
	lastPendulumBlock: 100,
	nextExpectedNonce: "3",
	baseFromBlock: "200",
	migrations: [{
		nonce: "2",
		recipient: "0x2222222222222222222222222222222222222222",
		palletAmount: "100",
		firstSeenMs: 1_700_000_000_000,
		sourceBlock: 100,
	}],
	livenessAlertedAt: [["2", 1_700_000_001_000]],
	unmatchedBaseFirstSeenAt: [["Approved:3", 1_700_000_002_000]],
};

test("monitor state survives a durable replacement with its security history", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-monitor-state-"));
	const path = join(directory, "state.json");
	try {
		assert.equal(loadMonitorState(path), undefined);
		saveMonitorState(path, state);
		const loaded = loadMonitorState(path);
		assert.deepEqual(loaded, state);
		assert.doesNotThrow(() => assertMonitorStateIdentity(loaded!, state));
		assert.throws(
			() => assertMonitorStateIdentity(loaded!, { ...state, pendulumGenesisHash: `0x${"44".repeat(32)}` }),
			/Pendulum genesis/,
		);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});

test("corrupt monitor state fails rather than restarting grace timers", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-monitor-state-"));
	const path = join(directory, "state.json");
	try {
		writeFileSync(path, "{");
		assert.throws(() => loadMonitorState(path), /refusing to reset security history/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});

test("missing unmatched-event history is rejected rather than resetting its grace period", () => {
	const directory = mkdtempSync(join(tmpdir(), "pen-monitor-state-"));
	const path = join(directory, "state.json");
	try {
		const { unmatchedBaseFirstSeenAt: _, ...incomplete } = state;
		writeFileSync(path, JSON.stringify(incomplete));
		assert.throws(() => loadMonitorState(path), /invalid or legacy schema/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});
