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

type Answer = (args: Record<string, unknown>) => unknown;

/** A releaser whose public client answers each call with `answers` (the
 *  finality boundary at block 100 and every nonce unconsumed unless
 *  overridden). */
function releaserWith(answers: Record<string, Answer>, entries: PendingRelease[]) {
	const alerts: string[] = [];
	const logs: string[] = [];
	const writes: unknown[] = [];
	const methods: Record<string, Answer> = { getBlock: () => ({ number: 100n }), readContract: () => false, ...answers };
	let clock = 1_000_000;
	const releaser: Releaser = {
		publicClient: Object.fromEntries(
			Object.entries(methods).map(([name, answer]) => [name, async (args: Record<string, unknown> = {}) => answer(args)]),
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
		alertedAt: new Map(),
		alert: async (subject, detail, severity = "warning") => {
			alerts.push(`${severity} ${subject}: ${detail}`);
		},
		log: (message) => logs.push(message),
		now: () => clock,
	};
	return { releaser, alerts, logs, writes, advance: (ms: number) => (clock += ms) };
}

/** What simulation raises for a vault revert. */
function vaultRevert(
	error:
		| { errorName: "ExceedsDailyCap" | "ExceedsPerReleaseCap"; args: [bigint, bigint] }
		| { errorName: "NonceAlreadyConsumed"; args: [bigint] },
): Error {
	const data = encodeErrorResult({ abi: vaultAbi, ...error });
	return new ContractFunctionExecutionError(
		new ContractFunctionRevertedError({ abi: vaultAbi, data, functionName: "release" }),
		{ abi: vaultAbi, functionName: "release", args: [7n, RECIPIENT, 0n] },
	);
}

const dailyCapRevert = () => vaultRevert({ errorName: "ExceedsDailyCap", args: [DAILY_CAP + 1n, 0n] });

const ENTRY = { nonce: 7n, recipient: RECIPIENT, palletAmount: 5_000n * 10n ** 12n } as const;

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

test("a consumption seen only at the latest state keeps the entry until the finality boundary shows it", async () => {
	const atBoundary = (args: Record<string, unknown>) => args.blockNumber === 100n;
	for (const [simulated, consumedLatest] of [
		// Our release, or a peer's, is mined but not yet inside the boundary.
		[new Error("execution reverted"), true],
		// A simulation served by a node ahead of the ones the reads reach.
		[vaultRevert({ errorName: "NonceAlreadyConsumed", args: [7n] }), false],
	] as const) {
		const { releaser, alerts, writes } = releaserWith(
			{
				simulateContract: () => {
					throw simulated;
				},
				readContract: (args) => !atBoundary(args) && consumedLatest,
			},
			[ENTRY],
		);
		await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
		assert.deepEqual([...releaser.pending.keys()], [7n], `${simulated.name}`);
		assert.deepEqual([alerts, writes], [[], []]);
	}
	const { releaser, logs } = releaserWith(
		{
			simulateContract: () => {
				throw vaultRevert({ errorName: "NonceAlreadyConsumed", args: [7n] });
			},
			readContract: atBoundary,
		},
		[ENTRY],
	);
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	assert.equal(releaser.pending.size, 0, "consumed at the boundary: dropped");
	assert.ok(logs.some((line) => line.startsWith("skip (already consumed): nonce=7 ")));
});

test("a blocked release pages once per interval, not once per cycle", async () => {
	const { releaser, alerts, advance } = releaserWith(
		{
			simulateContract: () => {
				throw vaultRevert({ errorName: "ExceedsPerReleaseCap", args: [ENTRY.palletAmount * CONVERSION_FACTOR, 1n] });
			},
		},
		[ENTRY],
	);
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	assert.deepEqual(alerts, [
		`critical release blocked and needs operator action: nonce=7 recipient=${RECIPIENT} amount=${ENTRY.palletAmount} — ExceedsPerReleaseCap; it cannot self-heal`,
	]);
	advance(releaser.blockedAlertIntervalMs);
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	assert.equal(alerts.length, 2);
	assert.equal(releaser.pending.size, 1);
});

test("an unexpected release failure pages once per interval, and again at once when the failure changes", async () => {
	let failure: Error = new Error("execution reverted: TokenNotSet");
	const { releaser, alerts, advance } = releaserWith(
		{
			simulateContract: () => {
				throw failure;
			},
		},
		[ENTRY],
	);
	for (let cycle = 0; cycle < 5; cycle++) await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	assert.deepEqual(alerts, [
		`critical unexpected release failure: nonce=7 recipient=${RECIPIENT} amount=${ENTRY.palletAmount} — execution reverted: TokenNotSet`,
	]);
	advance(releaser.blockedAlertIntervalMs);
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	assert.equal(alerts.length, 2);
	failure = vaultRevert({ errorName: "ExceedsPerReleaseCap", args: [ENTRY.palletAmount * CONVERSION_FACTOR, 1n] });
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	assert.equal(alerts.length, 3);
	assert.match(alerts[2], /^critical release blocked and needs operator action: nonce=7 /);
	assert.equal(releaser.pending.size, 1);
});
