import assert from "node:assert/strict";
import { test } from "node:test";
import {
	ContractFunctionExecutionError,
	ContractFunctionRevertedError,
	encodeErrorResult,
	type PublicClient,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { drainPending, type PendingRelease, type Releaser } from "./release.js";
import { vaultAbi } from "./vaultAbi.js";

const RECIPIENT = "0x2222222222222222222222222222222222222222";
/** 12-decimal pallet amounts become 18-decimal tokens. */
const CONVERSION_FACTOR = 1_000_000n;
const DAILY_CAP = 10_000n * 10n ** 18n;

/** A releaser whose public client answers with `answers` (the finality
 *  boundary at block 100 and every nonce unconsumed unless overridden). */
function releaserWith(answers: Record<string, () => unknown>, entries: PendingRelease[]) {
	const alerts: string[] = [];
	const logs: string[] = [];
	const writes: unknown[] = [];
	const methods: Record<string, () => unknown> = { getBlock: () => ({ number: 100n }), readContract: () => false, ...answers };
	const releaser: Releaser = {
		publicClient: Object.fromEntries(
			Object.entries(methods).map(([name, answer]) => [name, async () => answer()]),
		) as unknown as PublicClient,
		walletClient: {
			writeContract: async (request: unknown) => {
				writes.push(request);
				return `0x${"ab".repeat(32)}`;
			},
		} as unknown as Releaser["walletClient"],
		account: privateKeyToAccount(`0x${"11".repeat(32)}`),
		vaultAddress: "0x1111111111111111111111111111111111111111",
		baseFinalityTag: "safe",
		blockedAlertIntervalMs: 6 * 3600_000,
		pending: new Map(entries.map((entry) => [entry.nonce, entry])),
		blockedAlertedAt: new Map(),
		alert: async (subject, detail, severity = "warning") => {
			alerts.push(`${severity} ${subject}: ${detail}`);
		},
		log: (message) => logs.push(message),
	};
	return { releaser, alerts, logs, writes };
}

/** What simulation raises for the vault's `ExceedsDailyCap`. */
function dailyCapRevert(): Error {
	const data = encodeErrorResult({ abi: vaultAbi, errorName: "ExceedsDailyCap", args: [DAILY_CAP + 1n, 0n] });
	return new ContractFunctionExecutionError(
		new ContractFunctionRevertedError({ abi: vaultAbi, data, functionName: "release" }),
		{ abi: vaultAbi, functionName: "release", args: [7n, RECIPIENT, 0n] },
	);
}

test("a successful release keeps its entry until the nonce reads consumed at the finality boundary", async () => {
	const entry = { nonce: 7n, recipient: RECIPIENT, palletAmount: 5_000n * 10n ** 12n } as const;
	const { releaser, alerts, logs, writes } = releaserWith(
		{
			simulateContract: () => ({ request: { functionName: "release" } }),
			estimateContractGas: () => 50_000n,
			waitForTransactionReceipt: () => ({ status: "success" }),
		},
		[entry],
	);
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	assert.deepEqual(writes, [{ functionName: "release", gas: 100_000n }]);
	assert.deepEqual([...releaser.pending.values()], [entry]);
	assert.ok(logs.some((line) => line.startsWith("released (awaiting safe): nonce=7 ")));
	assert.deepEqual(alerts, []);
});

test("a daily-cap revert pages as blocked when the token amount is above the daily cap itself, and waits quietly otherwise", async () => {
	// Both pallet amounts are far below the token-denominated cap.
	const above = { nonce: 7n, recipient: RECIPIENT, palletAmount: 20_000n * 10n ** 12n } as const;
	const within = { nonce: 8n, recipient: RECIPIENT, palletAmount: 10_000n * 10n ** 12n } as const;
	const { releaser, alerts } = releaserWith(
		{
			simulateContract: () => {
				throw dailyCapRevert();
			},
		},
		[above, within],
	);
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	assert.deepEqual(alerts, [
		`critical release blocked and needs operator action: nonce=7 recipient=${RECIPIENT} amount=${above.palletAmount} — ExceedsDailyCapPermanently; it cannot self-heal`,
	]);
	assert.equal(releaser.pending.size, 2);
});
