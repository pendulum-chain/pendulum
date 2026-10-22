/**
 * Unit tests for the attestor's tuple-classification predicate.
 *
 * Run with `npm test` (compiles, then `node --test`). These lock in the round-6
 * fix: `isUnreleasable` must flag BOTH the zero address and the vault's own
 * address (case-insensitively), so a `migrate(_, <vault address>)` event is
 * skipped with a critical alert instead of crash-looping the whole attestor
 * fleet on the vault's `RecipientIsVault` revert.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
	BlockNotFoundError,
	ContractFunctionExecutionError,
	ContractFunctionRevertedError,
	ContractFunctionZeroDataError,
	encodeErrorResult,
	HttpRequestError,
	InsufficientFundsError,
	InvalidInputRpcError,
	LimitExceededRpcError,
	ResourceNotFoundRpcError,
	RpcRequestError,
	TimeoutError,
	WaitForTransactionReceiptTimeoutError,
} from "viem";
import {
	decodeMigrationEvent,
	isLostRace,
	isTransientRpcError,
	isUnreleasable,
	ownApprovalMissed,
	sendRejection,
	ZERO_ADDRESS,
} from "./checks.js";
import { vaultAbi } from "./vaultAbi.js";

const VAULT = "0x1111111111111111111111111111111111111111";
const NORMAL = "0x00000000000000000000000000000000deadbeef";
const AMOUNT = 5_000_000_000_000n; // 5 PEN in 12-decimal pallet units

// What viem raises for a JSON-RPC error answer: the mapped error type with the
// node's text as its details.
const rpcError = (code: number, message: string) =>
	new RpcRequestError({ body: {}, error: { code, message }, url: "https://rpc" });

test("a normal recipient with a non-zero amount is releasable", () => {
	assert.equal(isUnreleasable(NORMAL, AMOUNT, VAULT), false);
});

test("the zero address is unreleasable (vault reverts ZeroAddress)", () => {
	assert.equal(isUnreleasable(ZERO_ADDRESS, AMOUNT, VAULT), true);
});

test("the vault's own address is unreleasable (vault reverts RecipientIsVault)", () => {
	// The round-6 finding: without this, a 1-PEN migration to the vault address
	// crash-loops the entire fleet, since the pallet cannot reject it.
	assert.equal(isUnreleasable(VAULT, AMOUNT, VAULT), true);
});

test("the vault address is matched case-insensitively", () => {
	// The event decodes the H160 lower-cased; the configured vault address may
	// be EIP-55 checksummed. The comparison must not depend on casing either way.
	const vaultChecksummed = "0xAbCdEf0000000000000000000000000000000001";
	assert.equal(isUnreleasable(vaultChecksummed.toLowerCase(), AMOUNT, vaultChecksummed), true);
	assert.equal(isUnreleasable(vaultChecksummed, AMOUNT, vaultChecksummed.toLowerCase()), true);
});

test("a zero amount is unreleasable (vault reverts ZeroAmount)", () => {
	assert.equal(isUnreleasable(NORMAL, 0n, VAULT), true);
});

test("an unreleasable condition still wins when combined with a normal one", () => {
	assert.equal(isUnreleasable(ZERO_ADDRESS, 0n, VAULT), true);
	assert.equal(isUnreleasable(VAULT, 0n, VAULT), true);
});

test("a fatal error whose label embeds nonce 429/502 stays fatal", () => {
	// Nonces are sequential, so 429, 502, 503 and 504 all occur. An error text
	// embedding them (every submission error carries the tuple label) must not
	// be reclassified as an endpoint failure — the post-drills L1 class.
	assert.equal(
		isTransientRpcError(new Error("approve transaction reverted: 0xabc (nonce=429 recipient=0x11 amount=1000)")),
		false,
	);
	assert.equal(isTransientRpcError(new Error("unexpected state for nonce=502")), false);
});

test("HTTP-status endpoint failures are transient, matched structurally", () => {
	assert.equal(isTransientRpcError(new HttpRequestError({ url: "https://rpc", status: 429 })), true);
	assert.equal(isTransientRpcError(new HttpRequestError({ url: "https://rpc", status: 503 })), true);
	assert.equal(isTransientRpcError(new HttpRequestError({ url: "https://rpc", status: 403 })), false);
});

test("transient causes are found anywhere in the error chain", () => {
	const wrapped = new Error("request failed", { cause: new HttpRequestError({ url: "https://rpc", status: 502 }) });
	assert.equal(isTransientRpcError(wrapped), true);
	assert.equal(isTransientRpcError(new TimeoutError({ body: {}, url: "https://rpc" })), true);
});

test("socket-level failures are transient via their structured code", () => {
	const refused = new Error("connect failed") as Error & { code: string };
	refused.code = "ECONNREFUSED";
	assert.equal(isTransientRpcError(refused), true);
});

test("text-only transport failures (polkadot-js) are still recognized", () => {
	assert.equal(isTransientRpcError(new Error("WebSocket is not connected")), true);
	assert.equal(isTransientRpcError(new Error("disconnected from wss://node:443: 1006")), true);
	assert.equal(isTransientRpcError(new Error("fetch failed")), true);
	assert.equal(isTransientRpcError(new Error("socket hang up")), true);
	// A safe-block state read outside the node's retained window heals as the
	// safe head catches up; it must wait, not exit.
	assert.equal(isTransientRpcError(new Error("missing trie node 0xabc (path ) <nil>")), true);
});

test("a lagging backend's missing-block answers at the finality boundary are transient", () => {
	// op-geth (-32000) and reth (-32001) asked for a block they have not imported.
	assert.equal(isTransientRpcError(new InvalidInputRpcError(rpcError(-32000, "header not found"))), true);
	assert.equal(isTransientRpcError(new ResourceNotFoundRpcError(rpcError(-32001, "header not found"))), true);
	assert.equal(isTransientRpcError(new InvalidInputRpcError(rpcError(-32000, "unknown block"))), true);
	// A backend without a safe head: an error, or a null block.
	assert.equal(isTransientRpcError(new InvalidInputRpcError(rpcError(-32000, "safe block not found"))), true);
	assert.equal(isTransientRpcError(new BlockNotFoundError({})), true);
});

test("a receipt lookup on a backend still indexing its transactions is transient", () => {
	assert.equal(isTransientRpcError(new InvalidInputRpcError(rpcError(-32000, "transaction indexing is in progress"))), true);
});

test("a pruned Base state window, an endpoint limit and a silent Pendulum node are transient", () => {
	// reth asked for state at the pinned safe block outside its retained window.
	assert.equal(isTransientRpcError(new InvalidInputRpcError(rpcError(-32000, "state at block #36000000 is pruned"))), true);
	// JSON-RPC -32005 answered at HTTP 200, whatever text the provider uses.
	assert.equal(
		isTransientRpcError(new LimitExceededRpcError(rpcError(-32005, "exceeded its compute units per second capacity"))),
		true,
	);
	// polkadot-js when its node does not answer a request in time.
	assert.equal(isTransientRpcError(new Error("No response received from RPC endpoint in 60s")), true);
});

test("an eth_call answered with no data by a syncing backend is transient", () => {
	const zeroData = new ContractFunctionZeroDataError({ functionName: "hasApproved" });
	assert.equal(isTransientRpcError(zeroData), true);
	// How readContract raises it.
	assert.equal(
		isTransientRpcError(
			new ContractFunctionExecutionError(zeroData, {
				abi: vaultAbi,
				functionName: "hasApproved",
				args: [`0x${"00".repeat(32)}`, NORMAL],
			}),
		),
		true,
	);
});

test("a pruned Pendulum node stays fatal", () => {
	assert.equal(
		isTransientRpcError(new Error("-32000: Client error: UnknownBlock: State already discarded for 0xabc")),
		false,
	);
	// The runtime-API phrasing of the same condition (e.g. state_getMetadata).
	assert.equal(
		isTransientRpcError(
			new Error("-32000: Client error: Api called for an unknown Block: State already discarded for 0xabc"),
		),
		false,
	);
});

test("a node's refusal of a sent transaction is told apart from any other send failure", () => {
	// What eth_sendRawTransaction raises for the node's answer.
	const refused = (message: string) => new InvalidInputRpcError(rpcError(-32000, message));
	assert.equal(sendRejection(refused("already known")), "known");
	assert.equal(sendRejection(refused("nonce too low: next nonce 6, tx nonce 5")), "nonce");
	assert.equal(sendRejection(refused("replacement transaction underpriced")), "nonce");
	assert.equal(sendRejection(refused("transaction underpriced: tip needed 1, tip permitted 0")), "fees");
	assert.equal(sendRejection(refused("max fee per gas less than block base fee: address 0x1, maxFeePerGas: 1, baseFee: 2")), "fees");
	assert.equal(sendRejection(refused("fee cap below base fee")), "fees");
	assert.equal(sendRejection(refused("txpool is full")), "fees");
	// Not the pool's verdict on this transaction: these keep their usual handling.
	assert.equal(sendRejection(refused("insufficient funds for gas * price + value")), undefined);
	assert.equal(sendRejection(new HttpRequestError({ url: "https://rpc", status: 503 })), undefined);
	assert.equal(sendRejection(new Error("nonce=429 approve transaction reverted")), undefined);
});

test("only a vault rejection for a released or already-approved migration is a lost race", () => {
	const args = [7n, "0x00000000000000000000000000000000deadbeef", AMOUNT] as const;
	// What simulateContract and estimateContractGas raise for a revert.
	const reverted = (data: `0x${string}`) =>
		new ContractFunctionExecutionError(new ContractFunctionRevertedError({ abi: vaultAbi, data, functionName: "approve" }), {
			abi: vaultAbi,
			functionName: "approve",
			args,
		});
	const consumed = encodeErrorResult({ abi: vaultAbi, errorName: "NonceAlreadyConsumed", args: [7n] });
	const approved = encodeErrorResult({ abi: vaultAbi, errorName: "AlreadyApproved", args: [NORMAL] });
	assert.equal(isLostRace(reverted(consumed)), true);
	assert.equal(isLostRace(reverted(approved)), true);
	// Our own submission failed: these must surface even when the others
	// released the migration meanwhile.
	assert.equal(isLostRace(reverted("0xdde328f8")), false, "NotAttestor() after a removal");
	assert.equal(isLostRace(new InsufficientFundsError()), false);
	assert.equal(isLostRace(new WaitForTransactionReceiptTimeoutError({ hash: `0x${"ab".repeat(32)}` })), false);
	assert.equal(isLostRace(new HttpRequestError({ url: "https://rpc", status: 503 })), false);
	assert.equal(isLostRace(new Error("approve transaction reverted: 0xabc (nonce=7)")), false);
});

test("a resolved migration counts as our miss only when our approval provably did not land", async () => {
	const lostRace = new ContractFunctionExecutionError(
		new ContractFunctionRevertedError({
			abi: vaultAbi,
			data: encodeErrorResult({ abi: vaultAbi, errorName: "NonceAlreadyConsumed", args: [7n] }),
			functionName: "approve",
		}),
		{ abi: vaultAbi, functionName: "approve", args: [7n, NORMAL, AMOUNT] },
	);
	const sendFailed = new InsufficientFundsError();
	const recorded = (value: boolean) => async () => value;
	const unreadable = async (): Promise<boolean> => {
		throw new ContractFunctionZeroDataError({ functionName: "hasApproved" });
	};
	assert.equal(await ownApprovalMissed(new Error("approve transaction reverted"), true, unreadable), false, "mined");
	assert.equal(await ownApprovalMissed(lostRace, false, unreadable), false, "lost race");
	assert.equal(await ownApprovalMissed(sendFailed, false, recorded(true)), false, "recorded anyway");
	assert.equal(await ownApprovalMissed(sendFailed, false, recorded(false)), true, "never landed");
	// The migration is resolved, so a failing diagnostic read must neither
	// count a miss nor escape as an error.
	assert.equal(await ownApprovalMissed(sendFailed, false, unreadable), undefined, "unknown");
});

test("a decode failure is never transient", () => {
	assert.equal(isTransientRpcError(new Error("MigrationInitiated in block 7 has 5 fields, expected 4")), false);
});

test("a MigrationInitiated event decodes to the approval it asks for, and any other shape fails loudly", () => {
	// polkadot-js codecs, reduced to the accessors the decoder uses.
	const u = (value: bigint) => ({ toBigInt: () => value });
	const h160 = (hex: string) => ({ toHex: () => hex });
	const who = { toHex: () => `0x${"aa".repeat(32)}` };
	assert.deepEqual(decodeMigrationEvent([u(7n), who, h160(NORMAL), u(AMOUNT)], 12), {
		nonce: 7n,
		recipient: NORMAL,
		palletAmount: AMOUNT,
	});
	// A runtime upgrade that drops or adds a field must not decode positionally.
	assert.throws(() => decodeMigrationEvent([u(7n), h160(NORMAL), u(AMOUNT)], 12), /block 12 has 3 fields, expected 4/);
	assert.throws(
		() => decodeMigrationEvent([u(7n), who, h160(NORMAL), u(AMOUNT), u(1n)], 12),
		/block 12 has 5 fields, expected 4/,
	);
	// The account id in the address position is not a Base address.
	assert.throws(() => decodeMigrationEvent([u(7n), who, who, u(AMOUNT)], 12), /cannot decode base_address in block 12/);
});
