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

export interface PersistedPendingRelease {
	nonce: string;
	recipient: string;
	palletAmount: string;
}

export interface PersistedState {
	version: 1;
	baseChainId: number;
	vaultAddress: `0x${string}`;
	/** Next safe/finalized Base block to scan `ReleasePending` from. */
	fromBlock: string;
	pending: PersistedPendingRelease[];
}

function isMissing(error: unknown): boolean {
	return (error as NodeJS.ErrnoException)?.code === "ENOENT";
}

export function loadState(path: string): PersistedState | undefined {
	let raw: string;
	try {
		raw = readFileSync(path, "utf8");
	} catch (error) {
		if (isMissing(error)) return undefined;
		throw new Error(`cannot read releaser state ${path}`, { cause: error });
	}
	let value: unknown;
	try {
		value = JSON.parse(raw);
	} catch (error) {
		throw new Error(`releaser state ${path} is not valid JSON; refusing to reset progress`, { cause: error });
	}
	const state = value as Partial<PersistedState>;
	if (
		state.version !== 1 ||
		!Number.isSafeInteger(state.baseChainId) ||
		typeof state.vaultAddress !== "string" ||
		!/^0x[0-9a-fA-F]{40}$/.test(state.vaultAddress) ||
		typeof state.fromBlock !== "string" ||
		!/^\d+$/.test(state.fromBlock) ||
		!Array.isArray(state.pending) ||
		state.pending.some(
			(entry) =>
				typeof entry?.nonce !== "string" ||
				!/^\d+$/.test(entry.nonce) ||
				typeof entry?.recipient !== "string" ||
				!/^0x[0-9a-fA-F]{40}$/.test(entry.recipient) ||
				typeof entry?.palletAmount !== "string" ||
				!/^\d+$/.test(entry.palletAmount),
			)
	) {
		throw new Error(`releaser state ${path} has an invalid or legacy schema; refusing to reset progress`);
	}
	return state as PersistedState;
}

export function assertStateIdentity(
	state: PersistedState,
	expected: { baseChainId: number; vaultAddress: string },
): void {
	if (state.baseChainId !== expected.baseChainId) {
		throw new Error(`state Base chain ${state.baseChainId} does not match configured chain ${expected.baseChainId}`);
	}
	if (state.vaultAddress.toLowerCase() !== expected.vaultAddress.toLowerCase()) {
		throw new Error(`state vault ${state.vaultAddress} does not match configured vault ${expected.vaultAddress}`);
	}
}

export function saveState(path: string, state: PersistedState): void {
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
		throw new Error(`cannot durably write releaser state ${path}`, { cause: error });
	}
}
