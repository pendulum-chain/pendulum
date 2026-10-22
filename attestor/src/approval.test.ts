/**
 * The approval path against an in-memory vault: what is approved, how failures
 * are classified, and when the checkpoint may pass a block.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
	ContractFunctionExecutionError,
	ContractFunctionRevertedError,
	ContractFunctionZeroDataError,
	encodeErrorResult,
	InsufficientFundsError,
	type Hash,
	type Hex,
	type TransactionReceipt,
} from "viem";
import {
	approve,
	confirmDurable,
	GAS_LIMIT_MULTIPLIER,
	payloadHash,
	processBlock,
	type Attestor,
	type UnconfirmedBlock,
	type VaultChain,
} from "./approval.js";
import type { MigrationEvent } from "./checks.js";
import { vaultAbi } from "./vaultAbi.js";

const VAULT = "0x1111111111111111111111111111111111111111";
const EVENT: MigrationEvent = {
	nonce: 7n,
	recipient: "0x00000000000000000000000000000000deadbeef",
	palletAmount: 5_000_000_000_000n,
};
const OTHER: MigrationEvent = { ...EVENT, nonce: 8n };

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

/** The vault at the latest state and at the finality boundary block. */
class FakeVault implements VaultChain {
	consumed = new Set<bigint>();
	approved = new Set<Hex>();
	boundary = 100n;
	durableConsumed = new Set<bigint>();
	durableApproved = new Set<Hex>();
	estimate = 90_000n;
	sent: { event: MigrationEvent; gas: bigint }[] = [];
	/** What a sent approval does: mined and recorded, mined and reverted, or a send error. */
	onSend: (event: MigrationEvent) => "success" | "reverted" | Error = () => "success";
	/** Runs on every latest-state read, so a test can change state between them. */
	onRead = () => {};
	private outcomes = new Map<Hash, "success" | "reverted">();

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
		if (this.consumed.has(event.nonce)) throw vaultRevert("NonceAlreadyConsumed");
		if (this.approved.has(payloadHash(event))) throw vaultRevert("AlreadyApproved");
		return this.estimate;
	}
	async sendApprove(event: MigrationEvent, gas: bigint) {
		this.sent.push({ event, gas });
		const outcome = this.onSend(event);
		if (outcome instanceof Error) throw outcome;
		if (outcome === "success") this.approved.add(payloadHash(event));
		const hash = `0x${this.sent.length.toString(16).padStart(64, "0")}` as Hash;
		this.outcomes.set(hash, outcome);
		return hash;
	}
	async waitForReceipt(hash: Hash) {
		return { status: this.outcomes.get(hash), transactionHash: hash } as unknown as TransactionReceipt;
	}
	async boundaryBlock() {
		return this.boundary;
	}
}

function attestorOn(chain: FakeVault, overrides: Partial<Attestor> = {}) {
	const alerts: { subject: string; detail: string }[] = [];
	const logs: string[] = [];
	let clock = 1_000_000;
	const attestor: Attestor = {
		chain,
		vaultAddress: VAULT,
		baseFinalityTag: "safe",
		baseFinalityTimeoutMs: 900_000,
		missedApprovalAlertCount: 2,
		alert: async (subject, detail) => {
			alerts.push({ subject, detail: `${detail}` });
		},
		log: (message, extra) => {
			logs.push(extra === undefined ? message : `${message} ${extra}`);
		},
		now: () => clock,
		sleep: async (ms) => {
			clock += ms;
		},
		missedApprovals: 0,
		...overrides,
	};
	return { attestor, alerts, logs, advance: (ms: number) => (clock += ms) };
}

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

test("an approval carries the event's own nonce, recipient and pallet amount with a padded gas limit", async () => {
	const chain = new FakeVault();
	const { attestor, logs } = attestorOn(chain, { missedApprovals: 1 });
	assert.equal(await approve(attestor, EVENT), true);
	assert.deepEqual(chain.sent, [{ event: EVENT, gas: chain.estimate * 4n }]);
	assert.equal(GAS_LIMIT_MULTIPLIER, 4n);
	assert.equal(attestor.missedApprovals, 0, "a mined approval resets the miss count");
	assert.ok(logs.some((line) => line.startsWith("approved: nonce=7 ")));
});

test("a mined approval that reverted because the others released first is a lost race", async () => {
	const chain = new FakeVault();
	chain.onSend = () => {
		chain.consumed.add(EVENT.nonce);
		return "reverted";
	};
	const { attestor, alerts, logs } = attestorOn(chain, { missedApprovals: 1 });
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
	chain.onSend = (event) => {
		// The others release while our own send fails.
		chain.consumed.add(event.nonce);
		return new InsufficientFundsError();
	};
	const { attestor, alerts } = attestorOn(chain);
	for (const nonce of [1n, 2n, 3n, 4n]) assert.equal(await approve(attestor, { ...EVENT, nonce }), true);
	assert.equal(attestor.missedApprovals, 4);
	assert.deepEqual(
		alerts.map((a) => a.subject),
		["own approvals are not landing", "own approvals are not landing"],
	);
	assert.match(alerts[1].detail, /^4 consecutive migrations resolved without this attestor; last nonce=4 /);
});

test("when our own approval cannot be read after the others released, the miss is skipped and the read error logged", async () => {
	const chain = new FakeVault();
	const readError = new Error("backend answered 0x for hasApproved");
	chain.onSend = (event) => {
		chain.consumed.add(event.nonce);
		chain.hasApproved = async () => {
			throw readError;
		};
		return new InsufficientFundsError();
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
	const failure = new InsufficientFundsError();
	chain.onSend = () => failure;
	let slept = 0;
	const { attestor } = attestorOn(chain, {
		sleep: async () => {
			slept += 1;
		},
	});
	await assert.rejects(approve(attestor, EVENT), (error) => error === failure);
	assert.equal(slept, 4, "five rechecks, four waits between them");
	assert.equal(attestor.missedApprovals, 0);
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
	assert.equal(await confirmDurable(attestor, unconfirmed), undefined);
	assert.deepEqual(
		alerts.map((a) => a.subject),
		["approvals not durable at the Base finality boundary, re-submitting"],
	);
	assert.equal(chain.sent.length, 1, "our reorged-out approval is sent again");
	assert.equal(unconfirmed[0].lastAttemptMs, attestor.now());
});
