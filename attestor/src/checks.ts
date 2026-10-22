/**
 * Pure classification predicates for the attestor.
 *
 * Isolated from the chain-client plumbing in `main.ts` so they can be unit
 * tested exhaustively (see checks.test.ts). Two subtleties that have bitten
 * review rounds live here: which (nonce, recipient, amount) tuples the vault
 * will *deterministically* reject (must stay in exact lockstep with the input
 * reverts at the top of `MigrationVault.approve()`), and which failures are
 * transport-level noise rather than statements about an event.
 */

import {
	BaseError,
	BlockNotFoundError,
	ContractFunctionRevertedError,
	ContractFunctionZeroDataError,
	HttpRequestError,
	LimitExceededRpcError,
	TimeoutError,
} from "viem";

export const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";

/** The approval one `MigrationInitiated` event asks for, in pallet units. */
export interface MigrationEvent {
	nonce: bigint;
	recipient: `0x${string}`;
	palletAmount: bigint;
}

/**
 * Decode one `tokenMigration.MigrationInitiated` event's data:
 * `{ nonce: u64, who: AccountId, base_address: H160, amount: u128 }`. The shape
 * is guarded explicitly: a runtime upgrade changing the event must fail loudly
 * (PRD A5), not decode garbage positionally.
 */
export function decodeMigrationEvent(data: unknown[], blockNumber: number): MigrationEvent {
	if (data.length !== 4) {
		throw new Error(`MigrationInitiated in block ${blockNumber} has ${data.length} fields, expected 4`);
	}
	const [nonce, , baseAddress, amount] = data as [
		{ toBigInt(): bigint },
		unknown,
		{ toHex(): string },
		{ toBigInt(): bigint },
	];
	const recipient = baseAddress.toHex() as `0x${string}`;
	if (!/^0x[0-9a-fA-F]{40}$/.test(recipient)) {
		throw new Error(`cannot decode base_address in block ${blockNumber}: ${recipient}`);
	}
	return { nonce: nonce.toBigInt(), recipient, palletAmount: amount.toBigInt() };
}

/** HTTP statuses that signal endpoint pressure or transient upstream failure —
 *  they carry no information about the migration event itself. */
const TRANSIENT_HTTP_STATUS = new Set([408, 429, 500, 502, 503, 504]);

/** Node/undici socket-level failure codes (structured `error.code`). */
const TRANSIENT_SOCKET_CODES = new Set([
	"ETIMEDOUT",
	"ECONNRESET",
	"ECONNREFUSED",
	"ECONNABORTED",
	"EAI_AGAIN",
	"ENOTFOUND",
	"EPIPE",
	"UND_ERR_SOCKET",
	"UND_ERR_CONNECT_TIMEOUT",
]);

/** Word-anchored phrases for transports that surface failures only as message
 *  text (polkadot-js in particular). Bare numeric status codes are deliberately
 *  NOT matched: error messages embed tuple fields (`nonce=429 …`), so a
 *  digit-only pattern would reclassify a genuine failure at those nonces as
 *  transient (the post-drills L1 class). Numeric codes are matched structurally
 *  against `HttpRequestError.status` instead. */
const TRANSIENT_TEXT =
	/rate limit|too many requests|timeout|timed out|socket hang up|fetch failed|service unavailable|internal error|disconnected|websocket is not connected|no response received|connection (?:closed|refused|reset|terminated)|ETIMEDOUT|ECONNRESET|ECONNREFUSED|EAI_AGAIN|ENOTFOUND|EPIPE|missing trie node|historical state|state (?:is )?not available|state unavailable|is pruned|header not found|(?<!an )unknown block|block not found/i;
// The state phrasings (reth's is `state at block #N is pruned`): a state read
// pinned at the `safe` block can fall outside a node's retained-state window
// while the batcher lags (safe trails latest by more blocks than the node
// keeps). It heals as the safe head catches up, so it is a wait, not a verdict
// on any event — and not a reason to crash-loop.
// The block phrasings: behind a load-balanced endpoint the boundary block can
// come from one backend and the read pinned at it land on another that has not
// imported it yet, or that has no safe head at all. That heals as the backend
// catches up. Substrate's `UnknownBlock:` (no space) and its runtime API's
// `called for an unknown Block` are deliberately not matched: a pruned
// Pendulum node stays fatal.

/**
 * Transport-level failures that say nothing about the migration itself.
 *
 * PRD A5 requires the daemon to die rather than silently skip an event, and a
 * decode failure still does exactly that. But a rate limit or a dropped socket
 * carries no information about the event, and exiting on one turns every
 * transient RPC hiccup into an attestor outage. The checkpoint only advances
 * once a block is durably handled, so leaving a block unprocessed is safe —
 * the next finalized head simply re-processes it.
 *
 * Classification is structural first (error types and codes survive message
 * rewording and cannot collide with values embedded in labels), with the text
 * patterns as a fallback for string-only transports.
 */
export function isTransientRpcError(error: unknown): boolean {
	let depth = 0;
	for (let cause: unknown = error; cause && depth < 10; cause = (cause as { cause?: unknown }).cause, depth++) {
		if (cause instanceof HttpRequestError && cause.status !== undefined && TRANSIENT_HTTP_STATUS.has(cause.status)) {
			return true;
		}
		// BlockNotFoundError: viem's answer to a `null` block, e.g. a backend
		// without a safe head yet.
		if (cause instanceof TimeoutError || cause instanceof BlockNotFoundError) return true;
		// JSON-RPC -32005: the endpoint's request or rate limit, answered at HTTP 200.
		if (cause instanceof LimitExceededRpcError) return true;
		// An eth_call answered with no data ("0x"): a syncing backend without the
		// vault's state yet. Startup has read the vault, so its code is there.
		if (cause instanceof ContractFunctionZeroDataError) return true;
		const code = (cause as { code?: unknown }).code;
		if (typeof code === "string" && TRANSIENT_SOCKET_CODES.has(code)) return true;
		const texts = [
			(cause as { message?: unknown }).message,
			(cause as { details?: unknown }).details,
			(cause as { shortMessage?: unknown }).shortMessage,
		];
		if (texts.some((text) => typeof text === "string" && TRANSIENT_TEXT.test(text))) return true;
	}
	return false;
}

/** Why a node refused a transaction it was sent. */
export type SendRejection =
	/** The identical transaction is already in its pool. */
	| "known"
	/** The nonce is taken: mined, or held by a pending transaction that the
	 *  sent one does not out-bid. */
	| "nonce"
	/** Priced below what the node takes now, or no room in its pool. */
	| "fees";

/** Checked in this order: a replacement that is underpriced is a nonce answer. */
const SEND_REJECTIONS: [SendRejection, RegExp][] = [
	["known", /already known|transaction already imported/i],
	["nonce", /nonce too low|replacement transaction underpriced/i],
	["fees", /transaction underpriced|fee cap (?:less than|below) (?:block )?base fee|max fee per gas less than block base fee|txpool is full/i],
];

/** How the node refused a sent transaction, or undefined for any other failure
 *  (a transport error, an empty wallet, a malformed transaction). */
export function sendRejection(error: unknown): SendRejection | undefined {
	const texts: string[] = [];
	let depth = 0;
	for (let cause: unknown = error; cause && depth < 10; cause = (cause as { cause?: unknown }).cause, depth++) {
		for (const key of ["details", "shortMessage", "message"] as const) {
			const text = (cause as Record<string, unknown>)[key];
			if (typeof text === "string") texts.push(text);
		}
	}
	return SEND_REJECTIONS.find(([, pattern]) => texts.some((text) => pattern.test(text)))?.[0];
}

/** Vault reverts that only mean another submission got there first. */
const LOST_RACE_ERRORS = new Set(["NonceAlreadyConsumed", "AlreadyApproved"]);

/**
 * True when a failed approval merely lost the k-of-n race: the vault rejected
 * it in simulation or gas estimation because the migration is already
 * released or our approval is already recorded. Anything else (a send that
 * failed, a receipt that never came, an empty gas wallet, a removed attestor)
 * means our own submission did not land, even when the migration resolved
 * without us. A mined revert carries no reason and is told apart by the caller.
 */
export function isLostRace(error: unknown): boolean {
	if (!(error instanceof BaseError)) return false;
	const reverted = error.walk((cause) => cause instanceof ContractFunctionRevertedError);
	return reverted instanceof ContractFunctionRevertedError && LOST_RACE_ERRORS.has(reverted.data?.errorName ?? "");
}

/**
 * Whether a failed approval of a migration that resolved on-chain meanwhile
 * means our own approval never landed: not when our transaction was mined,
 * merely lost the race, or our approval is recorded anyway. `undefined` when
 * that last read fails: the migration is resolved either way, so the case is
 * neither counted as a miss nor a reason to exit.
 */
export async function ownApprovalMissed(
	error: unknown,
	mined: boolean,
	ownApprovalRecorded: () => Promise<boolean>,
): Promise<boolean | undefined> {
	if (mined || isLostRace(error)) return false;
	try {
		return !(await ownApprovalRecorded());
	} catch {
		return undefined;
	}
}

/**
 * True when the vault will deterministically reject this tuple, no matter who
 * submits it or when. Such an event must be SKIPPED (with a critical alert),
 * never retried: because every attestor hits the identical revert at the same
 * finalized block, crash-looping on it would halt the entire fleet and block
 * every migration behind it (the round-2 zero-address DoS class).
 *
 * `MigrationVault.approve` reverts up-front on exactly three input conditions:
 *   - a zero recipient            (`ZeroAddress`)
 *   - the vault's own address     (`RecipientIsVault` — a self-transfer would
 *                                   break the monitor's conservation identity)
 *   - a zero amount               (`ZeroAmount`)
 *
 * The pallet rejects the zero address and sub-minimum amounts before an event
 * is ever emitted, but it CANNOT know the vault's Base address (it has no
 * knowledge of Base state), so a `migrate(_, <vault address>)` reaches the
 * attestor as a well-formed event. This gate is therefore the ONLY line of
 * defence against a vault-recipient event bricking the fleet.
 */
export function isUnreleasable(recipient: string, palletAmount: bigint, vaultAddress: string): boolean {
	const to = recipient.toLowerCase();
	return to === ZERO_ADDRESS || to === vaultAddress.toLowerCase() || palletAmount === 0n;
}
