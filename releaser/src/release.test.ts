import assert from "node:assert/strict";
import { test } from "node:test";
import {
	ContractFunctionExecutionError,
	ContractFunctionRevertedError,
	encodeErrorResult,
	TransactionReceiptNotFoundError,
	WaitForTransactionReceiptTimeoutError,
	type Hash,
	type PublicClient,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { drainPending, type InFlightRelease, type PendingRelease, type Releaser } from "./release.js";
import { vaultAbi } from "./vaultAbi.js";

const RECIPIENT = "0x2222222222222222222222222222222222222222";
/** 12-decimal pallet amounts become 18-decimal tokens. */
const CONVERSION_FACTOR = 1_000_000n;
const DAILY_CAP = 10_000n * 10n ** 18n;

type Answer = (args: Record<string, unknown>) => unknown;
type Write = InFlightRelease["call"] & { nonce: number; maxFeePerGas: bigint; maxPriorityFeePerGas: bigint };

/** The hash the wallet answers for its nth send. */
const txHash = (index: number): Hash => `0x${(index + 1).toString(16).padStart(64, "0")}`;

/** A releaser whose public client answers each call with `answers`. Unless
 *  overridden: the finality boundary is at block 100, every nonce unconsumed,
 *  a release simulates fine, the account's nonce is 4, and a sent release is
 *  mined at once. */
function releaserWith(answers: Record<string, Answer>, entries: PendingRelease[]) {
	const alerts: string[] = [];
	const logs: string[] = [];
	const writes: Write[] = [];
	const methods: Record<string, Answer> = {
		getBlock: () => ({ number: 100n }),
		readContract: () => false,
		simulateContract: () => ({}),
		estimateContractGas: () => 50_000n,
		getTransactionCount: () => 4,
		estimateFeesPerGas: () => ({ maxFeePerGas: 7_000_000n, maxPriorityFeePerGas: 1_000_000n }),
		waitForTransactionReceipt: ({ hash }) => ({ status: "success", transactionHash: hash }),
		...answers,
	};
	let clock = 1_000_000;
	const releaser: Releaser = {
		publicClient: Object.fromEntries(
			Object.entries(methods).map(([name, answer]) => [name, async (args: Record<string, unknown> = {}) => answer(args)]),
		) as unknown as PublicClient,
		walletClient: {
			writeContract: async (call: Write) => {
				writes.push(call);
				return txHash(writes.length - 1);
			},
		} as unknown as Releaser["walletClient"],
		feeBump: { afterMs: 60_000, percent: 20, max: 3, maxFeePerGas: 1_000_000_000n },
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
	const { releaser, alerts, logs, writes } = releaserWith({}, [ENTRY]);
	await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	// The vault's release with the entry's tuple, at the pending nonce, the
	// estimated fees and a doubled gas limit.
	assert.deepEqual(
		writes.map(({ address, functionName, args, gas, nonce, maxFeePerGas, maxPriorityFeePerGas }) => ({
			address,
			functionName,
			args,
			gas,
			nonce,
			maxFeePerGas,
			maxPriorityFeePerGas,
		})),
		[
			{
				address: releaser.vaultAddress,
				functionName: "release",
				args: [ENTRY.nonce, ENTRY.recipient, ENTRY.palletAmount],
				gas: 100_000n,
				nonce: 4,
				maxFeePerGas: 7_000_000n,
				maxPriorityFeePerGas: 1_000_000n,
			},
		],
	);
	assert.deepEqual([...releaser.pending.values()], [ENTRY]);
	assert.ok(logs.some((line) => line.startsWith("released (awaiting safe): nonce=7 ")));
	assert.deepEqual(alerts, []);
	assert.equal(releaser.inFlight, undefined);
});

/** A node that never mines what it is sent. */
const notMined: Record<string, Answer> = {
	waitForTransactionReceipt: ({ hash }) => {
		throw new WaitForTransactionReceiptTimeoutError({ hash: hash as Hash });
	},
	getTransactionReceipt: ({ hash }) => {
		throw new TransactionReceiptNotFoundError({ hash: hash as Hash });
	},
};

test("a release not mined in time is followed at its nonce: nothing is sent behind it, it is bumped, then paged", async () => {
	let minedHash: Hash | undefined;
	const { releaser, alerts, writes, advance } = releaserWith(
		{
			...notMined,
			simulateContract: ({ args }) => {
				// Once it is mined, the latest state has the nonce consumed.
				if (minedHash && (args as bigint[])[0] === ENTRY.nonce) {
					throw vaultRevert({ errorName: "NonceAlreadyConsumed", args: [ENTRY.nonce] });
				}
				return {};
			},
			getTransactionReceipt: ({ hash }) => {
				if (hash !== minedHash) throw new TransactionReceiptNotFoundError({ hash: hash as Hash });
				return { status: "success", transactionHash: hash };
			},
		},
		[ENTRY, { ...ENTRY, nonce: 8n }],
	);
	const drain = () => drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	await drain();
	advance(30_000);
	await drain();
	assert.equal(writes.length, 1, "not sent again before the bump interval, and nothing sent behind it");
	for (let cycle = 0; cycle < 5; cycle++) {
		advance(60_000);
		await drain();
	}
	// The identical call at the same nonce, both fees raised, three times.
	assert.deepEqual(
		writes.map((w) => [w.args[0], w.nonce, w.gas, w.maxFeePerGas, w.maxPriorityFeePerGas]),
		[
			[7n, 4, 100_000n, 7_000_000n, 1_000_000n],
			[7n, 4, 100_000n, 8_400_000n, 1_200_000n],
			[7n, 4, 100_000n, 10_080_000n, 1_440_000n],
			[7n, 4, 100_000n, 12_096_000n, 1_728_000n],
		],
	);
	// Then one page, not repeated within the interval.
	assert.deepEqual(alerts, [
		`critical release transaction not mined: 3 fee bump(s) used up: transaction nonce 4, account's next mined nonce 4, ` +
			`maxFeePerGas 0.012096 gwei (cap 1 gwei); nonce=7 recipient=${RECIPIENT} amount=${ENTRY.palletAmount}`,
	]);
	// The second bump is mined: its entry waits for the boundary, and the next release goes out.
	minedHash = txHash(2);
	await drain();
	assert.deepEqual(
		writes.map((w) => w.args[0]),
		[7n, 7n, 7n, 7n, 8n],
	);
	assert.equal(releaser.inFlight?.entry.nonce, 8n);
	assert.equal(releaser.pending.size, 2);
	assert.equal(alerts.length, 1);
});

test("a release whose nonce another transaction used, or that an empty nonce below keeps from being mined, is sent anew", async () => {
	let latest = 4;
	let pendingCount = 5;
	const { releaser, writes, logs } = releaserWith(
		{ ...notMined, getTransactionCount: ({ blockTag }) => (blockTag === "latest" ? latest : pendingCount) },
		[ENTRY],
	);
	const drain = () => drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
	await drain();
	// Queued behind a transaction at nonce 4, which then leaves the pool unmined.
	pendingCount = 4;
	await drain();
	// Another transaction takes nonce 4.
	latest = 5;
	pendingCount = 5;
	await drain();
	assert.deepEqual(
		writes.map((w) => w.nonce),
		[5, 4, 5],
	);
	assert.equal(logs.filter((line) => line.startsWith("release transaction at nonce ")).length, 2);
});

test("the fee cap stops a release's bump and pages; no attempt offers more than the cap", async () => {
	const { releaser, alerts, writes, advance } = releaserWith(notMined, [ENTRY]);
	releaser.feeBump = { ...releaser.feeBump, maxFeePerGas: 8_000_000n };
	for (let cycle = 0; cycle < 3; cycle++) {
		await drainPending(releaser, CONVERSION_FACTOR, DAILY_CAP);
		advance(60_000);
	}
	// 7 → 8.4 is capped at 8; 8 → 8 is no replacement a pool takes.
	assert.deepEqual(
		writes.map((w) => [w.maxFeePerGas, w.maxPriorityFeePerGas]),
		[
			[7_000_000n, 1_000_000n],
			[8_000_000n, 1_200_000n],
		],
	);
	assert.equal(alerts.length, 1);
	assert.match(alerts[0], /^critical release transaction not mined: the fee cap stops the next bump: transaction nonce 4, .*\(cap 0\.008 gwei\); nonce=7 /);
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
