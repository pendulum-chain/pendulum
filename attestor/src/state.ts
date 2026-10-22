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

export interface Checkpoint {
	version: 1;
	baseChainId: number;
	vaultAddress: `0x${string}`;
	pendulumGenesisHash: `0x${string}`;
	/** The signer, and its vault attestor generation, whose approvals this
	 *  progress stands for. Optional only so that checkpoints written before
	 *  they were recorded still load; the daemon binds those on startup. */
	attestorAddress?: `0x${string}`;
	attestorGeneration?: number;
	lastProcessedBlock: number;
}

function isMissing(error: unknown): boolean {
	return (error as NodeJS.ErrnoException)?.code === "ENOENT";
}

export function loadCheckpoint(path: string): Checkpoint | undefined {
	let raw: string;
	try {
		raw = readFileSync(path, "utf8");
	} catch (error) {
		if (isMissing(error)) return undefined;
		throw new Error(`cannot read checkpoint ${path}`, { cause: error });
	}

	let value: unknown;
	try {
		value = JSON.parse(raw);
	} catch (error) {
		throw new Error(`checkpoint ${path} is not valid JSON; refusing to reset progress`, { cause: error });
	}
	const checkpoint = value as Partial<Checkpoint>;
	if (
		checkpoint.version !== 1 ||
		!Number.isSafeInteger(checkpoint.baseChainId) ||
		typeof checkpoint.vaultAddress !== "string" ||
		!/^0x[0-9a-fA-F]{40}$/.test(checkpoint.vaultAddress) ||
		typeof checkpoint.pendulumGenesisHash !== "string" ||
		!/^0x[0-9a-fA-F]{64}$/.test(checkpoint.pendulumGenesisHash) ||
		(checkpoint.attestorAddress !== undefined &&
			(typeof checkpoint.attestorAddress !== "string" || !/^0x[0-9a-fA-F]{40}$/.test(checkpoint.attestorAddress))) ||
		(checkpoint.attestorGeneration !== undefined &&
			!(Number.isSafeInteger(checkpoint.attestorGeneration) && checkpoint.attestorGeneration >= 1)) ||
		!Number.isSafeInteger(checkpoint.lastProcessedBlock) ||
		(checkpoint.lastProcessedBlock ?? -1) < -1
	) {
		throw new Error(`checkpoint ${path} has an invalid or legacy schema; refusing to reset progress`);
	}
	return checkpoint as Checkpoint;
}

/** Replace a checkpoint durably, so a crash leaves the old or new complete
 * document rather than a truncated file that looks like a first run. */
export function saveCheckpoint(path: string, checkpoint: Checkpoint): void {
	const temporary = `${path}.${process.pid}.tmp`;
	let file: number | undefined;
	try {
		file = openSync(temporary, "w", 0o600);
		writeFileSync(file, JSON.stringify(checkpoint));
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
			if (!isMissing(cleanupError)) console.error(`could not remove temporary checkpoint ${temporary}`, cleanupError);
		}
		throw new Error(`cannot durably write checkpoint ${path}`, { cause: error });
	}
}

export function assertCheckpointIdentity(
	checkpoint: Checkpoint,
	expected: {
		baseChainId: number;
		vaultAddress: string;
		pendulumGenesisHash: string;
		attestorAddress: string;
		attestorGeneration: number;
	},
): void {
	if (checkpoint.baseChainId !== expected.baseChainId) {
		throw new Error(
			`checkpoint Base chain ${checkpoint.baseChainId} does not match configured chain ${expected.baseChainId}`,
		);
	}
	if (checkpoint.vaultAddress.toLowerCase() !== expected.vaultAddress.toLowerCase()) {
		throw new Error(
			`checkpoint vault ${checkpoint.vaultAddress} does not match configured vault ${expected.vaultAddress}`,
		);
	}
	if (checkpoint.pendulumGenesisHash.toLowerCase() !== expected.pendulumGenesisHash.toLowerCase()) {
		throw new Error(
			`checkpoint Pendulum genesis ${checkpoint.pendulumGenesisHash} does not match connected chain ${expected.pendulumGenesisHash}`,
		);
	}
	// Progress made by another key, or by this key before it was removed and
	// re-added, says nothing about the approvals that count now: resuming from
	// it would silently skip every unreleased migration before it.
	if (
		checkpoint.attestorAddress !== undefined &&
		checkpoint.attestorAddress.toLowerCase() !== expected.attestorAddress.toLowerCase()
	) {
		throw new Error(
			`checkpoint belongs to attestor ${checkpoint.attestorAddress}, but this daemon signs as ${expected.attestorAddress}; ` +
				"a new key must backfill (runbook RB-6): move the checkpoint file aside and start with no checkpoint " +
				"and START_BLOCK before the oldest unreleased migration",
		);
	}
	if (checkpoint.attestorGeneration !== undefined && checkpoint.attestorGeneration !== expected.attestorGeneration) {
		throw new Error(
			`checkpoint was written under attestor generation ${checkpoint.attestorGeneration}, but ` +
				`${expected.attestorAddress} is now at generation ${expected.attestorGeneration}, so its earlier approvals ` +
				"no longer count; rewind (runbook RB-6): lower lastProcessedBlock in the checkpoint file to a block before " +
				`the oldest unreleased migration, set attestorGeneration to ${expected.attestorGeneration}, keep every other ` +
				"field, and restart",
		);
	}
}
