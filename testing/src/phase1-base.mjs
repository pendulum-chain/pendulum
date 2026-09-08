/**
 * Phase 1 of docs/pen-migration-local-test-plan.md — the Base side on Anvil.
 *
 * These assertions mirror the Foundry suite, but run against the *deployed
 * bytecode* produced by the real Deploy.s.sol, including its two-step admin
 * handover. That is the point: unit tests exercise the contract, this
 * exercises the thing we will actually ship and the script that ships it.
 *
 *   anvil --port 8545 --slots-in-an-epoch 0
 *   node testing/src/phase1-base.mjs
 */

import { assert, assertEq, check, section, summarise } from "./harness.mjs";
import { accounts, admin, attestors, expectRevert, guardian, pub, send, warp } from "./anvil.mjs";
import { erc20Abi, vaultAbi } from "./abi.mjs";
import { deployStack, PARAMS } from "./deploy.mjs";

const recipient = accounts[7].address;
const CF = PARAMS.CONVERSION_FACTOR;
let nonce = 0n;
const nextNonce = () => nonce++;

console.log("Phase 1 — Base contracts on Anvil");
console.log("  deploying via script/Deploy.s.sol …");
const { vault, pen } = deployStack();
console.log(`  vault ${vault}\n  PEN   ${pen}`);

const V = { address: vault, abi: vaultAbi };
const P = { address: pen, abi: erc20Abi };
const read = (c, functionName, args) => pub.readContract({ ...c, functionName, args });

/** Approve with attestors [from, to). Ranged rather than counted, because
 *  re-approving with an attestor that already signed a payload reverts
 *  AlreadyApproved -- which is the contract behaving correctly. */
async function approveRange(from, to, n, recipientAddr, palletAmount) {
	for (let i = from; i < to; i++) {
		await send(attestors[i], { ...V, functionName: "approve", args: [n, recipientAddr, palletAmount] });
	}
}
const approveWith = (count, n, to, amt) => approveRange(0, count, n, to, amt);

/** Fully refill the rolling daily bucket. It is shared state across checks, so
 *  any check that needs budget must claim it explicitly.
 *
 *  Call this AFTER any setCaps: the bucket refills at a rate proportional to
 *  the CURRENT dailyCap, so lowering the cap slows the decay of consumption
 *  already recorded. Warping first and lowering the cap afterwards leaves the
 *  old consumption largely undecayed. */
const refillBucket = () => warp(30 * 24 * 3600);

section("Deployment");

await check("the full supply is minted and sits in the vault", async () => {
	assertEq(await read(P, "totalSupply"), PARAMS.MAX_ISSUANCE, "totalSupply");
	assertEq(await read(P, "balanceOf", [vault]), PARAMS.MAX_ISSUANCE, "vault balance");
});

await check("attestor set and threshold match the deployment parameters", async () => {
	assertEq(await read(V, "threshold"), 3n, "threshold");
	assertEq(await read(V, "attestorCount"), 4n, "attestorCount");
});

await check("the vault is wired to the token, unpaused, with the guardian set", async () => {
	assertEq((await read(V, "token")).toLowerCase(), pen.toLowerCase(), "token");
	assertEq(await read(V, "paused"), false, "paused");
	assertEq((await read(V, "guardian")).toLowerCase(), guardian.address.toLowerCase(), "guardian");
});

await check("admin handover is two-step and completes only on acceptance", async () => {
	// Until the admin Safe accepts, the deployer still holds admin. This gap is
	// real on mainnet too, which is why it belongs in the deployment runbook.
	const before = await read(V, "admin");
	assert(before.toLowerCase() !== admin.address.toLowerCase(), "admin transferred without acceptance");
	await send(admin, { ...V, functionName: "acceptAdmin", args: [] });
	assertEq((await read(V, "admin")).toLowerCase(), admin.address.toLowerCase(), "admin after acceptance");
});

section("Release path");

await check("three matching approvals release; two do not", async () => {
	const amount = 5n * 10n ** 12n; // 5 PEN in 12-decimal pallet units
	const n = nextNonce();
	await approveRange(0, 2, n, recipient, amount);
	assertEq(await read(P, "balanceOf", [recipient]), 0n, "released below threshold");

	await approveRange(2, 3, n, recipient, amount);
	assertEq(await read(P, "balanceOf", [recipient]), amount * CF, "12->18 decimal conversion");
	assertEq(await read(V, "nonceConsumed", [n]), true, "nonceConsumed");
});

await check("a consumed nonce cannot be released twice", async () => {
	const amount = 5n * 10n ** 12n;
	const n = 0n; // already consumed above
	const reason = await expectRevert(attestors[3], { ...V, functionName: "approve", args: [n, recipient, amount] });
	assert(reason?.includes("NonceAlreadyConsumed"), `expected NonceAlreadyConsumed, got: ${reason}`);
});

section("Caps");

await check("an amount above the per-release cap defers instead of reverting", async () => {
	const palletAmount = PARAMS.PER_RELEASE_CAP / CF + 1n; // just over the cap
	const n = nextNonce();
	const pendingBefore = await read(V, "pendingApprovedAmount");
	await approveWith(3, n, recipient, palletAmount);

	assertEq(await read(V, "nonceConsumed", [n]), false, "must not release above the cap");
	const pendingAfter = await read(V, "pendingApprovedAmount");
	assert(pendingAfter > pendingBefore, "pendingApprovedAmount did not increase");

	// It cannot self-heal — only a governance cap raise clears it.
	const reason = await expectRevert(accounts[0], { ...V, functionName: "release", args: [n, recipient, palletAmount] });
	assert(reason?.includes("ExceedsPerReleaseCap"), `expected ExceedsPerReleaseCap, got: ${reason}`);

	await send(admin, { ...V, functionName: "setCaps", args: [PARAMS.PER_RELEASE_CAP * 4n, PARAMS.DAILY_CAP * 4n] });
	await send(accounts[0], { ...V, functionName: "release", args: [n, recipient, palletAmount] });
	assertEq(await read(V, "nonceConsumed", [n]), true, "release after the cap raise");
	assertEq(await read(V, "pendingApprovedAmount"), pendingBefore, "pending accounting not cleared");
});

await check("the daily cap is a rolling bucket that refills over time", async () => {
	// Reset to a small, known daily budget FIRST, then refill at that new rate.
	const daily = 100_000n * 10n ** 18n;
	await send(admin, { ...V, functionName: "setCaps", args: [daily, daily] });
	await refillBucket();
	const palletAmount = daily / CF;

	const n = nextNonce();
	await approveWith(3, n, recipient, palletAmount);
	assertEq(await read(V, "nonceConsumed", [n]), true, "first release should fit the budget");
	assertEq(await read(V, "availableDailyAllowance"), 0n, "budget should be exhausted");

	// A second release now defers, and self-heals as the bucket refills.
	const n2 = nextNonce();
	await approveWith(3, n2, recipient, palletAmount);
	assertEq(await read(V, "nonceConsumed", [n2]), false, "should defer on an empty bucket");

	// Half a day back gives roughly half the budget -- gradual, not a reset.
	await warp(12 * 3600);
	const half = await read(V, "availableDailyAllowance");
	assert(half > daily / 3n && half < daily, `half-day refill looks wrong: ${half}`);

	// Still not enough for a full-cap release: the bucket refills, it does not
	// jump. Only once it is full does the deferred release go through, and the
	// releaser service is what retries it in production.
	const early = await expectRevert(accounts[0], { ...V, functionName: "release", args: [n2, recipient, palletAmount] });
	assert(early?.includes("ExceedsDailyCap"), `expected ExceedsDailyCap at half refill, got: ${early}`);

	await warp(13 * 3600);
	await send(accounts[0], { ...V, functionName: "release", args: [n2, recipient, palletAmount] });
	assertEq(await read(V, "nonceConsumed", [n2]), true, "deferred release after full refill");
});

section("Guardian and sweep");

await check("the guardian can pause but cannot unpause; the admin can", async () => {
	await refillBucket();
	await send(guardian, { ...V, functionName: "pause", args: [] });
	assertEq(await read(V, "paused"), true, "paused");

	const n = nextNonce();
	const amount = 1n * 10n ** 12n;
	await approveWith(3, n, recipient, amount);
	assertEq(await read(V, "nonceConsumed", [n]), false, "must not release while paused");

	const reason = await expectRevert(guardian, { ...V, functionName: "unpause", args: [] });
	assert(reason?.includes("NotAdmin"), `guardian should not unpause, got: ${reason}`);

	await send(admin, { ...V, functionName: "unpause", args: [] });
	assertEq(await read(V, "paused"), false, "unpaused by admin");
	await send(accounts[0], { ...V, functionName: "release", args: [n, recipient, amount] });
	assertEq(await read(V, "nonceConsumed", [n]), true, "deferred release after unpause");
});

await check("the remainder cannot be swept before the earliest sweep timestamp", async () => {
	const reason = await expectRevert(admin, { ...V, functionName: "sweepRemainder", args: [recipient, 1n] });
	assert(reason?.includes("SweepNotYetAllowed"), `expected SweepNotYetAllowed, got: ${reason}`);
});

await check("conservation holds: vault balance + released + swept == total supply", async () => {
	const [bal, released, swept, supply] = await Promise.all([
		read(P, "balanceOf", [vault]), read(V, "totalReleased"), read(V, "totalSwept"), read(P, "totalSupply"),
	]);
	assertEq(bal + released + swept, supply, "conservation identity");
});

process.exit(summarise() ? 0 : 1);
