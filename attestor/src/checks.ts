/**
 * Pure tuple-classification predicates for the attestor.
 *
 * Isolated from the chain-client plumbing in `main.ts` so they can be unit
 * tested exhaustively (see checks.test.ts). The subtlety that has bitten a
 * review round — which (nonce, recipient, amount) tuples the vault will
 * *deterministically* reject — lives here, and must stay in exact lockstep with
 * the input reverts at the top of `MigrationVault.approve()`.
 */

export const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";

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
