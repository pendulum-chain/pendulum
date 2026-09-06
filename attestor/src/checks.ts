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

import { HttpRequestError, TimeoutError } from "viem";

export const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";

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
	/rate limit|too many requests|timeout|timed out|socket hang up|fetch failed|service unavailable|internal error|disconnected|websocket is not connected|connection (?:closed|refused|reset|terminated)|ETIMEDOUT|ECONNRESET|ECONNREFUSED|EAI_AGAIN|ENOTFOUND|EPIPE|missing trie node|historical state|state (?:is )?not available|state unavailable/i;
// The last four: a state read pinned at the `safe` block can fall outside a
// node's retained-state window while the batcher lags (safe trails latest by
// more blocks than the node keeps). It heals as the safe head catches up, so
// it is a wait, not a verdict on any event — and not a reason to crash-loop.

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
		if (cause instanceof TimeoutError) return true;
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
