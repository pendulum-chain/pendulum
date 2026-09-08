import {
	closeSync,
	fsyncSync,
	openSync,
	readFileSync,
	renameSync,
	unlinkSync,
	writeFileSync,
} from "node:fs";
import { dirname } from "node:path";

export interface PersistedMigration {
	nonce: string;
	recipient: `0x${string}`;
	palletAmount: string;
	firstSeenMs: number;
	sourceBlock: number;
}

export interface PersistedMonitorState {
	version: 1;
	baseChainId: number;
	vaultAddress: `0x${string}`;
	pendulumGenesisHash: `0x${string}`;
	lastPendulumBlock: number;
	nextExpectedNonce: string;
	baseFromBlock: string;
	migrations: PersistedMigration[];
	livenessAlertedAt: Array<[string, number]>;
	unmatchedBaseFirstSeenAt: Array<[string, number]>;
	/** Pendulum totalIssuance + TotalMigrated when first observed (round 9);
	 *  absent in state written before it existed and re-anchored on load. */
	issuanceAnchor?: string;
}

function isMissing(error: unknown): boolean {
	return (error as NodeJS.ErrnoException)?.code === "ENOENT";
}

function isUint(value: unknown): value is string {
	return typeof value === "string" && /^\d+$/.test(value);
}

export function loadMonitorState(path: string): PersistedMonitorState | undefined {
	let raw: string;
	try {
		raw = readFileSync(path, "utf8");
	} catch (error) {
		if (isMissing(error)) return undefined;
		throw new Error(`cannot read monitor state ${path}`, { cause: error });
	}
	let value: unknown;
	try {
		value = JSON.parse(raw);
	} catch (error) {
		throw new Error(`monitor state ${path} is not valid JSON; refusing to reset security history`, { cause: error });
	}
	const state = value as Partial<PersistedMonitorState>;
	const badMigration = !Array.isArray(state.migrations) || state.migrations.some((migration) =>
		!isUint(migration?.nonce) ||
		typeof migration?.recipient !== "string" ||
		!/^0x[0-9a-fA-F]{40}$/.test(migration.recipient) ||
		!isUint(migration?.palletAmount) ||
		!Number.isSafeInteger(migration?.firstSeenMs) ||
		!Number.isSafeInteger(migration?.sourceBlock),
	);
	const badAlerts = !Array.isArray(state.livenessAlertedAt) || state.livenessAlertedAt.some(
		(entry) => !Array.isArray(entry) || entry.length !== 2 || !isUint(entry[0]) || !Number.isSafeInteger(entry[1]),
	);
	const badUnmatchedEvents = !Array.isArray(state.unmatchedBaseFirstSeenAt) || state.unmatchedBaseFirstSeenAt.some(
		(entry) =>
			!Array.isArray(entry) ||
			entry.length !== 2 ||
			typeof entry[0] !== "string" ||
			!/^(Approved|Released):\d+$/.test(entry[0]) ||
			!Number.isSafeInteger(entry[1]),
	);
	if (
		state.version !== 1 ||
		!Number.isSafeInteger(state.baseChainId) ||
		typeof state.vaultAddress !== "string" ||
		!/^0x[0-9a-fA-F]{40}$/.test(state.vaultAddress) ||
		typeof state.pendulumGenesisHash !== "string" ||
		!/^0x[0-9a-fA-F]{64}$/.test(state.pendulumGenesisHash) ||
		!Number.isSafeInteger(state.lastPendulumBlock) ||
		!isUint(state.nextExpectedNonce) ||
		!isUint(state.baseFromBlock) ||
		(state.issuanceAnchor !== undefined && !isUint(state.issuanceAnchor)) ||
		badMigration ||
		badAlerts ||
		badUnmatchedEvents
	) {
		throw new Error(`monitor state ${path} has an invalid or legacy schema; refusing to reset security history`);
	}
	return state as PersistedMonitorState;
}

export function assertMonitorStateIdentity(
	state: { baseChainId: number; vaultAddress: string; pendulumGenesisHash: string },
	expected: { baseChainId: number; vaultAddress: string; pendulumGenesisHash: string },
): void {
	if (state.baseChainId !== expected.baseChainId) {
		throw new Error(`state Base chain ${state.baseChainId} does not match configured chain ${expected.baseChainId}`);
	}
	if (state.vaultAddress.toLowerCase() !== expected.vaultAddress.toLowerCase()) {
		throw new Error(`state vault ${state.vaultAddress} does not match configured vault ${expected.vaultAddress}`);
	}
	if (state.pendulumGenesisHash.toLowerCase() !== expected.pendulumGenesisHash.toLowerCase()) {
		throw new Error(
			`state Pendulum genesis ${state.pendulumGenesisHash} does not match connected chain ${expected.pendulumGenesisHash}`,
		);
	}
}

export function saveMonitorState(path: string, state: PersistedMonitorState): void {
	const temporary = `${path}.${process.pid}.tmp`;
	let file: number | undefined;
	try {
		file = openSync(temporary, "w", 0o600);
		writeFileSync(file, JSON.stringify(state));
		fsyncSync(file);
		closeSync(file);
		file = undefined;
		renameSync(temporary, path);
		const directory = openSync(dirname(path), "r");
		try {
			fsyncSync(directory);
		} finally {
			closeSync(directory);
		}
	} catch (error) {
		if (file !== undefined) closeSync(file);
		try {
			unlinkSync(temporary);
		} catch (cleanupError) {
			if (!isMissing(cleanupError)) console.error(`could not remove temporary state ${temporary}`, cleanupError);
		}
		throw new Error(`cannot durably write monitor state ${path}`, { cause: error });
	}
}
