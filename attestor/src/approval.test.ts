/**
 * The approval path against an in-memory vault and node: what is approved, how
 * an unmined approval is followed up, how failures are classified, and when
 * the checkpoint may pass a block.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
	ContractFunctionExecutionError,
	ContractFunctionRevertedError,
	ContractFunctionZeroDataError,
	decodeFunctionData,
	encodeErrorResult,
	HttpRequestError,
	InvalidInputRpcError,
	keccak256,
	parseTransaction,
	RpcRequestError,
	type Hash,
	type Hex,
	type TransactionReceipt,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import {
	approve,
	bumpedFees,
	cappedFees,
	confirmDurable,
	GAS_LIMIT_MULTIPLIER,
	payloadHash,
	processBlock,
	type Attestor,
	type Fees,
	type UnconfirmedBlock,
	type VaultChain,
} from "./approval.js";
import type { MigrationEvent } from "./checks.js";
import { vaultAbi } from "./vaultAbi.js";

const VAULT = "0x1111111111111111111111111111111111111111";
const CHAIN_ID = 8453;
const EVENT: MigrationEvent = {
	nonce: 7n,
	recipient: "0x00000000000000000000000000000000deadbeef",
	palletAmount: 5_000_000_000_000n,
};
const OTHER: MigrationEvent = { ...EVENT, nonce: 8n };
const AFTER_MS = 60_000;

/** What simulation and gas estimation raise for a vault revert. */
function vaultRevert(errorName: "NonceAlreadyConsumed" | "AlreadyApproved"): Error {
	const data =
		errorName === "NonceAlreadyConsumed"
			? encodeErrorResult({ abi: vaultAbi, errorName, args: [EVENT.nonce] })
			: encodeErrorResult({ abi: vaultAbi, errorName, args: [VAULT] });
	return new ContractFunctionExecutionError(
		new ContractFunctionRevertedError({ abi: vaultAbi, data, functionName: "approve" }),
		{ abi: vaultAbi, functionName: "approve", args: [EVENT.nonce, EVENT.recipient, EVENT.palletAmount] },
	);
}

/** A node's JSON-RPC error answer, as viem raises it from eth_sendRawTransaction. */
const nodeError = (message: string) =>
	new InvalidInputRpcError(new RpcRequestError({ body: {}, error: { code: -32000, message }, url: "http://rpc" }));

interface Sent extends Fees {
	hash: Hash;
	nonce: number;
	to?: string;
	data?: Hex;
	value: bigint;
	gas?: bigint;
	chainId?: number;
}

/** The vault (at the latest state and at the finality boundary block) and a
 *  node with a one-sender pool: transactions are mined in nonce order once
 *  their fee cap covers the base fee, and a pending one is replaced only by a
 *  transaction raising both fees by 10 %, as op-geth does. */
class FakeVault implements VaultChain {
	consumed = new Set<bigint>();
	approved = new Set<Hex>();
	boundary = 100n;
	durableConsumed = new Set<bigint>();
	durableApproved = new Set<Hex>();
	estimate = 90_000n;
	approveGasCalls = 0;
	baseFee = 5_000_000n;
	estimateFees: Fees = { maxFeePerGas: 7_000_000n, maxPriorityFeePerGas: 1_000_000n };
	minedNonce = 5;
	/** Overrides the pending count the node reports (a lagging backend). */
	pendingNonce?: number;
	pool = new Map<number, Sent>();
	receipts = new Map<Hash, TransactionReceipt>();
	receiptsVisible = true;
	/** Every transaction the node was handed, refused or not. */
	sent: Sent[] = [];
	/** A node answer refusing the nth send, if any. */
	refuse: (index: number, tx: Sent) => string | undefined = () => undefined;
	/** Runs on every send, after the refusal check. */
	onSend: (tx: Sent) => void = () => {};
	/** An endpoint that answers with the hash but never forwards the send,
	 *  while still counting its nonce as pending. */
	swallow: (tx: Sent) => boolean = () => false;
	/** Runs on every latest-state vault read, so a test can change state between them. */
	onRead = () => {};

	private atBlock(blockNumber: bigint | undefined, latest: boolean, durable: boolean): boolean {
		if (blockNumber === undefined) {
			this.onRead();
			return latest;
		}
		assert.equal(blockNumber, this.boundary, "durability is read at the boundary block");
		return durable;
	}
	async nonceConsumed(nonce: bigint, blockNumber?: bigint) {
		return this.atBlock(blockNumber, this.consumed.has(nonce), this.durableConsumed.has(nonce));
	}
	async hasApproved(payload: Hex, blockNumber?: bigint) {
		return this.atBlock(blockNumber, this.approved.has(payload), this.durableApproved.has(payload));
	}
	async approveGas(event: MigrationEvent) {
		this.approveGasCalls += 1;
		if (this.consumed.has(event.nonce)) throw vaultRevert("NonceAlreadyConsumed");
		if (this.approved.has(payloadHash(event))) throw vaultRevert("AlreadyApproved");
		return this.estimate;
	}
	async accountNonce(blockTag: "latest" | "pending") {
		if (blockTag === "latest") return this.minedNonce;
		if (this.pendingNonce !== undefined) return this.pendingNonce;
		let nonce = this.minedNonce;
		while (this.pool.has(nonce)) nonce += 1;
		return nonce;
	}
	async fees() {
		return this.estimateFees;
	}
	async sendRawTransaction(serialized: Hex) {
		const parsed = parseTransaction(serialized);
		const tx: Sent = {
			hash: keccak256(serialized),
			nonce: parsed.nonce as number,
			to: parsed.to ?? undefined,
			data: parsed.data,
			value: parsed.value ?? 0n,
			gas: parsed.gas,
			chainId: parsed.chainId,
			maxFeePerGas: parsed.maxFeePerGas as bigint,
			maxPriorityFeePerGas: parsed.maxPriorityFeePerGas as bigint,
		};
		this.sent.push(tx);
		const refusal = this.refuse(this.sent.length - 1, tx);
		if (refusal) throw nodeError(refusal);
		this.onSend(tx);
		if (this.swallow(tx)) {
			this.pendingNonce = tx.nonce + 1;
			return tx.hash;
		}
		if (tx.nonce < this.minedNonce) throw nodeError("nonce too low");
		const held = this.pool.get(tx.nonce);
		if (held?.hash === tx.hash) throw nodeError("already known");
		const outbids = (fee: bigint, old: bigint) => fee > old && fee >= (old * 110n) / 100n;
		if (held && !(outbids(tx.maxFeePerGas, held.maxFeePerGas) && outbids(tx.maxPriorityFeePerGas, held.maxPriorityFeePerGas))) {
			throw nodeError("replacement transaction underpriced");
		}
		this.pool.set(tx.nonce, tx);
		return tx.hash;
	}
	async receipt(hash: Hash) {
		return this.receiptsVisible ? this.receipts.get(hash) : undefined;
	}
	async boundaryBlock() {
		return this.boundary;
	}
	/** Mine every pooled transaction that is next in line and pays the base fee.
	 *  An approval records ours unless the migration is already resolved. */
	mine() {
		for (let tx = this.pool.get(this.minedNonce); tx && tx.maxFeePerGas >= this.baseFee; tx = this.pool.get(this.minedNonce)) {
			this.pool.delete(this.minedNonce);
			this.minedNonce += 1;
			let status = "reverted";
			if (tx.to?.toLowerCase() === VAULT && tx.data) {
				const call = decodeFunctionData({ abi: vaultAbi, data: tx.data });
				assert.equal(call.functionName, "approve");
				const [nonce, recipient, palletAmount] = call.args as [bigint, `0x${string}`, bigint];
				const payload = payloadHash({ nonce, recipient, palletAmount });
				if (!this.consumed.has(nonce) && !this.approved.has(payload)) {
					this.approved.add(payload);
					status = "success";
				}
			}
			this.receipts.set(tx.hash, { status, transactionHash: tx.hash } as unknown as TransactionReceipt);
		}
	}
}

/** An attestor on `chain` whose clock only moves when it sleeps; every sleep
 *  runs `tick` (by default: the chain mines what it can). */
function attestorOn(chain: FakeVault, overrides: Partial<Attestor> = {}, tick?: (now: number) => void) {
	const alerts: { subject: string; detail: string }[] = [];
	const logs: string[] = [];
	const start = 1_000_000;
	let clock = start;
	const attestor: Attestor = {
		chain,
		account: privateKeyToAccount(`0x${"11".repeat(32)}`),
		baseChainId: CHAIN_ID,
		vaultAddress: VAULT,
		baseFinalityTag: "safe",
		baseFinalityTimeoutMs: 900_000,
		missedApprovalAlertCount: 2,
		feeBump: { afterMs: AFTER_MS, percent: 20, max: 3, maxFeePerGas: 1_000_000_000n },
		alert: async (subject, detail) => {
			alerts.push({ subject, detail: `${detail}` });
		},
		log: (message, extra) => {
			logs.push(extra === undefined ? message : `${message} ${extra}`);
		},
		now: () => clock,
		sleep: async (ms) => {
			clock += ms;
			if (clock - start > 24 * 3600_000) throw new Error("a day of test time passed without the flow settling");
			if (tick) tick(clock - start);
			else chain.mine();
		},
		missedApprovals: 0,
		submissions: new Map(),
		nextNonce: 0,
		...overrides,
	};
	return { attestor, alerts, logs, advance: (ms: number) => (clock += ms) };
}

/** The approve call a sent transaction carries, with lower-case addresses. */
function approveArgs(tx: Sent): readonly unknown[] {
	const call = decodeFunctionData({ abi: vaultAbi, data: tx.data as Hex });
	assert.equal(call.functionName, "approve");
	return (call.args as readonly unknown[]).map((arg) => (typeof arg === "string" ? arg.toLowerCase() : arg));
}

/** The fields a replacement must keep from the attempt it replaces. */
const callOf = (tx: Sent) => ({ nonce: tx.nonce, to: tx.to, data: tx.data, value: tx.value, gas: tx.gas, chainId: tx.chainId });

test("the payload hash is the vault's payloadHash(nonce, recipient, palletAmount)", () => {
	// keccak256(abi.encode(uint64 7, address 0x…deadbeef, uint256 5e12)), from Foundry's cast.
	assert.equal(payloadHash(EVENT), "0xe214823af04c4036de666eabddeae5ad457bbd53132a20cf8509f918e8591844");
});

test("an unreleasable event is skipped with a critical alert and never sent", async () => {
	const chain = new FakeVault();
	const { attestor, alerts } = attestorOn(chain);
	assert.equal(await approve(attestor, { ...EVENT, recipient: VAULT }), false);
	assert.equal(await approve(attestor, { ...EVENT, palletAmount: 0n }), false);
	assert.equal(chain.sent.length, 0);
	assert.deepEqual(
		alerts.map((a) => a.subject),
		["CRITICAL: unreleasable migration event skipped permanently", "CRITICAL: unreleasable migration event skipped permanently"],
	);
});

test("a migration already released or approved by us is not sent again", async () => {
	const chain = new FakeVault();
	const { attestor, alerts } = attestorOn(chain);
	chain.consumed.add(EVENT.nonce);
	assert.equal(await approve(attestor, EVENT), true);
	chain.consumed.clear();
	chain.approved.add(payloadHash(EVENT));
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 0);
	assert.equal(alerts.length, 0);
});

test("an approval is the vault's approve with the event's own tuple, signed at the pending nonce with a padded gas limit", async () => {
	const chain = new FakeVault();
	const { attestor, logs } = attestorOn(chain, { missedApprovals: 1 });
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 1);
	const [tx] = chain.sent;
	assert.deepEqual(callOf(tx), {
		nonce: 5,
		to: VAULT,
		data: tx.data,
		value: 0n,
		gas: chain.estimate * 4n,
		chainId: CHAIN_ID,
	});
	assert.equal(GAS_LIMIT_MULTIPLIER, 4n);
	// Pallet units as emitted: the vault converts, the attestor never does.
	assert.deepEqual(approveArgs(tx), [EVENT.nonce, EVENT.recipient, EVENT.palletAmount]);
	assert.deepEqual({ maxFeePerGas: tx.maxFeePerGas, maxPriorityFeePerGas: tx.maxPriorityFeePerGas }, chain.estimateFees);
	assert.ok(chain.approved.has(payloadHash(EVENT)));
	assert.equal(attestor.missedApprovals, 0, "a mined approval resets the miss count");
	assert.equal(attestor.nextNonce, 6);
	assert.equal(attestor.submissions.size, 0);
	assert.ok(logs.some((line) => line.startsWith("approved: nonce=7 ")));
});

test("a mined approval that reverted because the others released first is a lost race", async () => {
	const chain = new FakeVault();
	const { attestor, alerts, logs } = attestorOn(chain, { missedApprovals: 1 }, () => {
		chain.consumed.add(EVENT.nonce);
		chain.mine();
	});
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(attestor.missedApprovals, 0);
	assert.equal(alerts.length, 0);
	assert.ok(logs.some((line) => line.startsWith("skip (raced, resolved on-chain)")));
});

test("a simulation that finds the migration released meanwhile is a lost race, not a miss", async () => {
	const chain = new FakeVault();
	let reads = 0;
	chain.onRead = () => {
		// The others' release lands right after the pre-check's two reads.
		if (++reads === 2) chain.consumed.add(EVENT.nonce);
	};
	const { attestor, alerts } = attestorOn(chain);
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 0);
	assert.equal(attestor.missedApprovals, 0);
	assert.equal(alerts.length, 0);
});

test("own approvals that never land are counted and paged every MISSED_APPROVAL_ALERT_COUNT", async () => {
	const chain = new FakeVault();
	// Our own send fails while the others release.
	chain.refuse = (_, tx) => {
		chain.consumed.add(approveArgs(tx)[0] as bigint);
		return "insufficient funds for gas * price + value";
	};
	const { attestor, alerts } = attestorOn(chain);
	for (const nonce of [1n, 2n, 3n, 4n]) assert.equal(await approve(attestor, { ...EVENT, nonce }), true);
	assert.equal(attestor.missedApprovals, 4);
	assert.deepEqual(
		alerts.map((a) => a.subject),
		["own approvals are not landing", "own approvals are not landing"],
	);
	assert.match(alerts[1].detail, /^4 consecutive migrations resolved without this attestor; last nonce=4 /);
	assert.equal(attestor.submissions.size, 0);
});

test("when our own approval cannot be read after the others released, the miss is skipped and the read error logged", async () => {
	const chain = new FakeVault();
	const readError = new Error("backend answered 0x for hasApproved");
	chain.refuse = () => {
		chain.consumed.add(EVENT.nonce);
		chain.hasApproved = async () => {
			throw readError;
		};
		return "insufficient funds for gas * price + value";
	};
	const { attestor, alerts, logs } = attestorOn(chain);
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(attestor.missedApprovals, 0);
	assert.equal(alerts.length, 0);
	assert.ok(logs.includes(`own-approval read failed: nonce=7 recipient=${EVENT.recipient} amount=${EVENT.palletAmount} ${readError}`));
	assert.ok(logs.some((line) => line.startsWith("skip (resolved on-chain; could not read whether our own approval is recorded)")));
});

test("a failed approval of an unresolved migration propagates after the rechecks", async () => {
	const chain = new FakeVault();
	chain.refuse = () => "insufficient funds for gas * price + value";
	let slept = 0;
	const { attestor } = attestorOn(chain, {
		sleep: async () => {
			slept += 1;
		},
	});
	await assert.rejects(approve(attestor, EVENT), /insufficient funds/);
	assert.equal(slept, 4, "five rechecks, four waits between them");
	assert.equal(attestor.missedApprovals, 0);
	assert.equal(attestor.nextNonce, 0, "a refused first attempt holds no nonce");
});

test("an approval not mined in time is re-sent: the identical call at the same nonce, both fees raised", async () => {
	const chain = new FakeVault();
	chain.baseFee = 8_000_000n; // above the estimate's fee cap: stuck
	const { attestor, alerts } = attestorOn(chain);
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 2);
	const [first, bump] = chain.sent;
	assert.deepEqual(callOf(bump), callOf(first));
	assert.equal(bump.maxFeePerGas, 8_400_000n);
	assert.equal(bump.maxPriorityFeePerGas, 1_200_000n);
	assert.ok(chain.receipts.has(bump.hash));
	assert.ok(chain.approved.has(payloadHash(EVENT)));
	assert.equal(alerts.length, 0);
	assert.equal(attestor.nextNonce, first.nonce + 1);
});

test("nothing is re-sent before the bump interval has passed", async () => {
	const chain = new FakeVault();
	chain.baseFee = 8_000_000n;
	const { attestor } = attestorOn(chain, {}, (elapsed) => {
		if (elapsed >= AFTER_MS - 2000) chain.baseFee = 5_000_000n;
		chain.mine();
	});
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 1, "mined in the last poll before the bump was due");
});

test("a bump catches up with a fee estimate that rose past the bump percent", async () => {
	const chain = new FakeVault();
	chain.onSend = () => {
		if (chain.sent.length === 1) {
			chain.baseFee = 30_000_000n;
			chain.estimateFees = { maxFeePerGas: 37_000_000n, maxPriorityFeePerGas: 1_000_000n };
		}
	};
	chain.baseFee = 8_000_000n;
	const { attestor } = attestorOn(chain);
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 2);
	assert.equal(chain.sent[1].maxFeePerGas, 37_000_000n);
	assert.equal(chain.sent[1].maxPriorityFeePerGas, 1_200_000n);
});

test("bumping stops once the others have released: no raised fee, and our miss is counted", async () => {
	const chain = new FakeVault();
	chain.baseFee = 100_000_000n;
	const { attestor, alerts } = attestorOn(chain, {}, (elapsed) => {
		if (elapsed > AFTER_MS / 2) chain.consumed.add(EVENT.nonce);
		chain.mine();
	});
	assert.equal(await approve(attestor, EVENT), true);
	// Only the unchanged re-send of the same attempt, which the node already holds.
	assert.equal(chain.sent.length, 2);
	assert.equal(chain.sent[1].hash, chain.sent[0].hash);
	assert.equal(attestor.missedApprovals, 1);
	assert.equal(alerts.length, 0);
	assert.equal(attestor.submissions.size, 0);
	assert.equal(attestor.nextNonce, 6, "the abandoned transaction keeps its nonce");
});

test("an abandoned approval the node dropped is re-sent unchanged, so later approvals are not stuck behind its nonce", async () => {
	const chain = new FakeVault();
	chain.baseFee = 100_000_000n;
	let dropped = false;
	const { attestor } = attestorOn(chain, {}, (elapsed) => {
		if (elapsed > AFTER_MS / 2 && !dropped) {
			// The sequencer lost it while the endpoint still counts it as pending.
			dropped = true;
			chain.pool.delete(5);
			chain.pendingNonce = 6;
			chain.consumed.add(EVENT.nonce);
			chain.baseFee = 5_000_000n;
		}
		chain.mine();
	});
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent[1].hash, chain.sent[0].hash);
	chain.pendingNonce = undefined;
	assert.equal(await approve(attestor, OTHER), true);
	assert.deepEqual(
		chain.sent.map((tx) => tx.nonce),
		[5, 5, 6],
	);
	assert.equal(chain.minedNonce, 7);
	assert.ok(chain.approved.has(payloadHash(OTHER)));
});

test("bumping stops once our own approval is recorded, even without a visible receipt", async () => {
	const chain = new FakeVault();
	chain.receiptsVisible = false;
	const { attestor, alerts } = attestorOn(chain);
	assert.equal(await approve(attestor, EVENT), true);
	assert.ok(chain.sent.every((tx) => tx.hash === chain.sent[0].hash), "never bumped");
	assert.equal(attestor.missedApprovals, 0, "our approval landed");
	assert.equal(alerts.length, 0);
});

test("after the last bump it pages once and keeps waiting until the approval is mined", async () => {
	const chain = new FakeVault();
	chain.baseFee = 100_000_000n;
	const { attestor, alerts } = attestorOn(chain, {}, (elapsed) => {
		// Fees fall again ten minutes in.
		if (elapsed >= 10 * AFTER_MS) chain.baseFee = 5_000_000n;
		chain.mine();
	});
	assert.equal(await approve(attestor, EVENT), true);
	assert.ok(chain.sent.every((tx) => tx.nonce === 5));
	// The first attempt and three bumps; after that only the last one, unchanged.
	assert.deepEqual(
		chain.sent.slice(0, 4).map((tx) => tx.maxFeePerGas),
		[7_000_000n, 8_400_000n, 10_080_000n, 12_096_000n],
	);
	assert.ok(chain.sent.length > 4 && chain.sent.slice(4).every((tx) => tx.hash === chain.sent[3].hash));
	assert.deepEqual(
		alerts.map((a) => a.subject),
		["own approval not mined"],
	);
	assert.match(
		alerts[0].detail,
		/^3 fee bump\(s\) used up: transaction nonce 5, account's next mined nonce 5, maxFeePerGas 0\.012096 gwei \(cap 1 gwei\); nonce=7 /,
	);
	assert.ok(chain.approved.has(payloadHash(EVENT)));
	assert.equal(attestor.missedApprovals, 0);
});

test("the fee cap stops a bump and pages once; no attempt offers more than the cap", async () => {
	const chain = new FakeVault();
	chain.baseFee = 100_000_000n;
	chain.estimateFees = { maxFeePerGas: 20_000_000n, maxPriorityFeePerGas: 1_000_000n };
	const cap = 25_000_000n;
	const { attestor, alerts } = attestorOn(chain, { feeBump: { afterMs: AFTER_MS, percent: 20, max: 3, maxFeePerGas: cap } }, (elapsed) => {
		if (elapsed >= 10 * AFTER_MS) chain.consumed.add(EVENT.nonce);
		chain.mine();
	});
	assert.equal(await approve(attestor, EVENT), true);
	// 20 → 24 (+20 %) → 25 would be only +4 %, which no pool takes as a
	// replacement. Every later send is the last attempt again, unchanged.
	assert.deepEqual(
		chain.sent.slice(0, 2).map((tx) => tx.maxFeePerGas),
		[20_000_000n, 24_000_000n],
	);
	assert.ok(chain.sent.slice(2).every((tx) => tx.hash === chain.sent[1].hash));
	assert.ok(chain.sent.every((tx) => tx.maxFeePerGas <= cap));
	assert.deepEqual(
		alerts.map((a) => a.subject),
		["own approval not mined"],
	);
	assert.match(alerts[0].detail, /^the fee cap stops the next bump: transaction nonce 5,/);
});

test("an attempt at the cap that the node dropped is brought back by re-sending it unchanged", async () => {
	const chain = new FakeVault();
	chain.estimateFees = { maxFeePerGas: 2_000_000_000n, maxPriorityFeePerGas: 1_000_000_000n };
	chain.baseFee = 1_000_000n;
	chain.swallow = () => chain.sent.length === 1;
	const { attestor, alerts } = attestorOn(chain);
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 2);
	assert.equal(chain.sent[1].hash, chain.sent[0].hash, "the same transaction, at the cap");
	assert.equal(chain.sent[0].maxFeePerGas, 1_000_000_000n);
	assert.deepEqual(
		alerts.map((a) => a.subject),
		["own approval not mined"],
	);
	assert.ok(chain.approved.has(payloadHash(EVENT)));
});

test("the first attempt never offers more than the cap, even when the estimate does", () => {
	assert.deepEqual(cappedFees({ maxFeePerGas: 30n, maxPriorityFeePerGas: 40n }, 25n), {
		maxFeePerGas: 25n,
		maxPriorityFeePerGas: 25n,
	});
	assert.deepEqual(cappedFees({ maxFeePerGas: 20n, maxPriorityFeePerGas: 1n }, 25n), {
		maxFeePerGas: 20n,
		maxPriorityFeePerGas: 1n,
	});
});

test("bumped fees rise by the percent, rounded up and by at least 1 wei, and up to the current estimate", () => {
	const policy = { afterMs: AFTER_MS, percent: 20, max: 3, maxFeePerGas: 1_000_000n };
	const fees = (maxFeePerGas: bigint, maxPriorityFeePerGas: bigint): Fees => ({ maxFeePerGas, maxPriorityFeePerGas });
	assert.deepEqual(bumpedFees(fees(1000n, 100n), fees(0n, 0n), policy), fees(1200n, 120n));
	assert.deepEqual(bumpedFees(fees(1001n, 101n), fees(0n, 0n), policy), fees(1202n, 122n), "rounded up");
	assert.deepEqual(bumpedFees(fees(1n, 0n), fees(0n, 0n), policy), fees(2n, 1n), "a tiny or zero tip still rises");
	assert.deepEqual(bumpedFees(fees(1000n, 100n), fees(5000n, 300n), policy), fees(5000n, 300n), "the estimate wins");
	// Clamped to the cap while that still outbids the last attempt by 10 %…
	assert.deepEqual(bumpedFees(fees(900_000n, 100n), fees(0n, 0n), policy), fees(1_000_000n, 120n));
	// …and no bump at all once it would not.
	assert.equal(bumpedFees(fees(910_000n, 100n), fees(0n, 0n), policy), undefined);
	assert.equal(bumpedFees(fees(1_000_000n, 100n), fees(2_000_000n, 100n), policy), undefined);
});

test("a refused replacement keeps the earlier attempt, and the next bump goes higher", async () => {
	const chain = new FakeVault();
	chain.baseFee = 100_000_000n;
	const refusals = [
		undefined,
		"replacement transaction underpriced",
		"max fee per gas less than block base fee: address 0x19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A, maxFeePerGas: 1, baseFee: 2",
		"txpool is full",
	];
	chain.refuse = (index) => refusals[index];
	chain.onSend = () => {
		if (chain.sent.length === 5) chain.baseFee = 5_000_000n;
	};
	const { attestor, alerts } = attestorOn(chain, {
		feeBump: { afterMs: AFTER_MS, percent: 20, max: 5, maxFeePerGas: 1_000_000_000n },
	});
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 5);
	assert.ok(chain.sent.every((tx) => tx.nonce === 5));
	const caps = chain.sent.map((tx) => tx.maxFeePerGas);
	assert.ok(caps.every((cap, i) => i === 0 || cap > caps[i - 1]), "every attempt offers more than the one before");
	assert.ok(chain.receipts.has(chain.sent[4].hash));
	assert.equal(alerts.length, 0);
});

test("a first attempt refused for its price or a full pool is tried again at the same nonce", async () => {
	const chain = new FakeVault();
	chain.refuse = (index) => (index === 0 ? "txpool is full" : undefined);
	const { attestor } = attestorOn(chain);
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.sent.length, 2);
	assert.equal(chain.sent[1].nonce, chain.sent[0].nonce);
	assert.ok(chain.sent[1].maxFeePerGas > chain.sent[0].maxFeePerGas);
	assert.ok(chain.approved.has(payloadHash(EVENT)));
});

test("a nonce held by a transaction this process did not send is skipped, never replaced", async () => {
	const chain = new FakeVault();
	const foreign: Sent = {
		hash: `0x${"ee".repeat(32)}`,
		nonce: 5,
		value: 0n,
		maxFeePerGas: 50_000_000n,
		maxPriorityFeePerGas: 2_000_000n,
	};
	chain.pool.set(5, foreign);
	chain.pendingNonce = 5; // the backend answering the count has not seen it
	chain.baseFee = 60_000_000n;
	chain.onSend = () => {
		if (chain.sent.length === 2) {
			chain.pendingNonce = undefined;
			chain.baseFee = 5_000_000n;
		}
	};
	const { attestor } = attestorOn(chain);
	assert.equal(await approve(attestor, EVENT), true);
	assert.deepEqual(
		chain.sent.map((tx) => tx.nonce),
		[5, 6],
	);
	assert.ok(chain.receipts.has(foreign.hash), "the other transaction kept its nonce");
	assert.ok(chain.receipts.has(chain.sent[1].hash));
	assert.ok(chain.approved.has(payloadHash(EVENT)));
});

test("a second payload is never signed at the first one's nonce, even when the node reports it free", async () => {
	const chain = new FakeVault();
	chain.baseFee = 100_000_000n;
	const { attestor } = attestorOn(chain, {}, (elapsed) => {
		if (elapsed > AFTER_MS / 2) chain.consumed.add(EVENT.nonce);
		chain.mine();
	});
	assert.equal(await approve(attestor, EVENT), true);
	assert.deepEqual(
		chain.sent.map((tx) => tx.nonce),
		[5, 5],
	);
	// The backend answering the count lost the abandoned transaction.
	chain.pendingNonce = 5;
	chain.onSend = () => {
		chain.baseFee = 5_000_000n;
	};
	assert.equal(await approve(attestor, OTHER), true);
	assert.equal(chain.sent.length, 3);
	assert.equal(chain.sent[2].nonce, 6);
	assert.deepEqual(approveArgs(chain.sent[2]), [OTHER.nonce, OTHER.recipient, OTHER.palletAmount]);
	assert.ok(chain.approved.has(payloadHash(OTHER)));
});

test("a retry after a failed read resumes the same transaction at its nonce", async () => {
	const chain = new FakeVault();
	chain.baseFee = 8_000_000n;
	const receipt = chain.receipt.bind(chain);
	let failures = 1;
	chain.receipt = async (hash) => {
		if (failures-- > 0) throw new HttpRequestError({ url: "http://rpc", status: 503 });
		return receipt(hash);
	};
	const { attestor } = attestorOn(chain);
	await assert.rejects(approve(attestor, EVENT), HttpRequestError);
	assert.equal(attestor.submissions.size, 1);
	assert.equal(await approve(attestor, EVENT), true);
	assert.equal(chain.approveGasCalls, 1, "no second transaction was prepared");
	assert.ok(chain.sent.length >= 2 && chain.sent.every((tx) => tx.nonce === 5));
	assert.ok(chain.approved.has(payloadHash(EVENT)));
	assert.equal(attestor.submissions.size, 0);
});

test("when another transaction takes the nonce, the approval is sent again at a new one", async () => {
	const chain = new FakeVault();
	chain.baseFee = 100_000_000n;
	const { attestor } = attestorOn(chain, {}, (elapsed) => {
		if (elapsed === AFTER_MS / 2) {
			// The operator replaced the stuck transaction by hand.
			chain.pool.delete(5);
			chain.minedNonce = 6;
			chain.baseFee = 5_000_000n;
		}
		chain.mine();
	});
	assert.equal(await approve(attestor, EVENT), true);
	assert.deepEqual(
		chain.sent.map((tx) => tx.nonce),
		[5, 6],
	);
	assert.ok(chain.approved.has(payloadHash(EVENT)));
});

test("unreleasable events are not tracked for durability; every other event is", async () => {
	const chain = new FakeVault();
	const { attestor } = attestorOn(chain);
	const unconfirmed: UnconfirmedBlock[] = [];
	await processBlock(attestor, 42, [{ ...EVENT, recipient: VAULT }, EVENT], unconfirmed);
	assert.deepEqual(unconfirmed, [{ block: 42, events: [EVENT], lastAttemptMs: attestor.now() }]);
});

test("the checkpoint passes only leading blocks whose events are durable at the finality boundary", async () => {
	const chain = new FakeVault();
	const { attestor } = attestorOn(chain);
	const at = attestor.now();
	const unconfirmed: UnconfirmedBlock[] = [
		{ block: 10, events: [], lastAttemptMs: at },
		{ block: 11, events: [EVENT], lastAttemptMs: at },
		{ block: 12, events: [], lastAttemptMs: at },
		{ block: 13, events: [OTHER], lastAttemptMs: at },
	];
	// Approved at latest is not enough.
	chain.approved.add(payloadHash(EVENT));
	assert.equal(await confirmDurable(attestor, unconfirmed), 10);
	assert.deepEqual(
		unconfirmed.map((entry) => entry.block),
		[11, 12, 13],
	);
	// Our approval at the boundary, or the nonce released there, is.
	chain.durableApproved.add(payloadHash(EVENT));
	chain.durableConsumed.add(OTHER.nonce);
	assert.equal(await confirmDurable(attestor, unconfirmed), 13);
	assert.equal(unconfirmed.length, 0);
	assert.equal(await confirmDurable(attestor, unconfirmed), undefined);
});

test("a boundary block without vault state means not yet, never fatal", async () => {
	const chain = new FakeVault();
	chain.nonceConsumed = async () => {
		throw new ContractFunctionExecutionError(new ContractFunctionZeroDataError({ functionName: "nonceConsumed" }), {
			abi: vaultAbi,
			functionName: "nonceConsumed",
			args: [EVENT.nonce],
		});
	};
	const { attestor, logs } = attestorOn(chain);
	const unconfirmed: UnconfirmedBlock[] = [{ block: 11, events: [EVENT], lastAttemptMs: attestor.now() }];
	assert.equal(await confirmDurable(attestor, unconfirmed), undefined);
	assert.equal(unconfirmed.length, 1);
	assert.ok(logs.includes("vault has no state at safe block 100 yet; checkpoint waits"));
});

test("approvals that stay outside the boundary are re-submitted after the timeout, with an alert", async () => {
	const chain = new FakeVault();
	const { attestor, alerts, advance } = attestorOn(chain);
	const unconfirmed: UnconfirmedBlock[] = [{ block: 11, events: [EVENT], lastAttemptMs: attestor.now() }];
	advance(attestor.baseFinalityTimeoutMs - 1);
	assert.equal(await confirmDurable(attestor, unconfirmed), undefined);
	assert.equal(chain.sent.length, 0);
	advance(1);
	const resubmittedAt = attestor.now();
	assert.equal(await confirmDurable(attestor, unconfirmed), undefined);
	assert.deepEqual(
		alerts.map((a) => a.subject),
		["approvals not durable at the Base finality boundary, re-submitting"],
	);
	assert.equal(chain.sent.length, 1, "our reorged-out approval is sent again");
	assert.equal(unconfirmed[0].lastAttemptMs, resubmittedAt);
});
