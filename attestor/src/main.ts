/**
 * PEN migration attestor daemon (PRD §6.4).
 *
 * Watches RELAY-FINALIZED blocks on the operator's own Pendulum node for
 * `tokenMigration.MigrationInitiated` events and submits the matching
 * `approve(nonce, recipient, palletAmount)` transaction to the MigrationVault
 * on Base. The vault releases the tokens on the threshold-th approval.
 *
 * Design invariants:
 * - Only finalized blocks are read; blocks are processed strictly in order.
 * - The checkpoint file is advanced only after every event in a block has
 *   been handled, so a crash re-processes at most one block (idempotent:
 *   duplicate approvals revert harmlessly and are skipped by the pre-check).
 * - A decode failure is FATAL by design (PRD A5): the daemon alerts and
 *   exits rather than silently skipping an event; the checkpoint keeps the
 *   failing block next in line for after the operator intervenes.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { ApiPromise, WsProvider } from "@polkadot/api";
import {
	createPublicClient,
	createWalletClient,
	defineChain,
	encodeAbiParameters,
	http,
	keccak256,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { isUnreleasable } from "./checks.js";
import { config } from "./config.js";
import { vaultAbi } from "./vaultAbi.js";

interface Checkpoint {
	lastProcessedBlock: number;
}

/** Multiplier applied to the estimated gas for `approve`. See the note at the
 *  call site: the same call can take the cheap record path or the expensive
 *  threshold-crossing release path. */
const GAS_LIMIT_MULTIPLIER = 4n;

interface MigrationEvent {
	nonce: bigint;
	recipient: `0x${string}`;
	palletAmount: bigint;
}

const baseChain = defineChain({
	id: config.baseChainId,
	name: "base",
	nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
	rpcUrls: { default: { http: [config.baseRpcUrl] } },
});

const account = privateKeyToAccount(config.attestorPrivateKey);
const publicClient = createPublicClient({ chain: baseChain, transport: http(config.baseRpcUrl) });
const walletClient = createWalletClient({
	account,
	chain: baseChain,
	transport: http(config.baseRpcUrl),
});

function log(message: string, extra?: unknown): void {
	console.log(`${new Date().toISOString()} ${message}`, extra ?? "");
}

async function alert(subject: string, detail: unknown): Promise<void> {
	console.error(`${new Date().toISOString()} ALERT: ${subject}`, detail);
	if (!config.alertWebhookUrl) return;
	try {
		await fetch(config.alertWebhookUrl, {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify({ service: "pen-attestor", attestor: account.address, subject, detail: `${detail}` }),
		});
	} catch (webhookError) {
		console.error("alert webhook failed", webhookError);
	}
}

function loadCheckpoint(): Checkpoint {
	try {
		return JSON.parse(readFileSync(config.checkpointFile, "utf8")) as Checkpoint;
	} catch {
		return { lastProcessedBlock: config.startBlock - 1 };
	}
}

function saveCheckpoint(checkpoint: Checkpoint): void {
	writeFileSync(config.checkpointFile, JSON.stringify(checkpoint));
}

function payloadHash(event: MigrationEvent): `0x${string}` {
	return keccak256(
		encodeAbiParameters(
			[{ type: "uint64" }, { type: "address" }, { type: "uint256" }],
			[event.nonce, event.recipient, event.palletAmount],
		),
	);
}

/** Extract MigrationInitiated events from one finalized Pendulum block. */
async function migrationEventsInBlock(api: ApiPromise, blockNumber: number): Promise<MigrationEvent[]> {
	const blockHash = await api.rpc.chain.getBlockHash(blockNumber);
	const apiAt = await api.at(blockHash);
	const records = (await apiAt.query.system.events()) as unknown as {
		event: { section: string; method: string; data: unknown[] };
	}[];

	const events: MigrationEvent[] = [];
	for (const record of records) {
		const { section, method, data } = record.event;
		if (section !== "tokenMigration" || method !== "MigrationInitiated") continue;
		// Event shape: { nonce: u64, who: AccountId, base_address: H160, amount: u128 }.
		// Guard the shape explicitly: a runtime upgrade changing the event must
		// fail loudly (PRD A5), not decode garbage positionally.
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
		events.push({ nonce: nonce.toBigInt(), recipient, palletAmount: amount.toBigInt() });
	}
	return events;
}

/** True when this migration no longer needs our approval (released, or we
 *  already approved). Rechecked after failures: with 4 attestors
 *  racing to the same event, losing the race is the NORMAL case, not an error. */
async function alreadyHandled(event: MigrationEvent): Promise<boolean> {
	const consumed = await publicClient.readContract({
		address: config.vaultAddress,
		abi: vaultAbi,
		functionName: "nonceConsumed",
		args: [event.nonce],
	});
	if (consumed) return true;
	return publicClient.readContract({
		address: config.vaultAddress,
		abi: vaultAbi,
		functionName: "hasApproved",
		args: [payloadHash(event), account.address],
	});
}

/** Submit the approval for one migration event, skipping work already done. */
async function approve(event: MigrationEvent): Promise<void> {
	const label = `nonce=${event.nonce} recipient=${event.recipient} amount=${event.palletAmount}`;

	if (isUnreleasable(event.recipient, event.palletAmount, config.vaultAddress)) {
		await alert("CRITICAL: unreleasable migration event skipped permanently", label);
		return;
	}

	if (await alreadyHandled(event)) {
		log(`skip (already released or approved): ${label}`);
		return;
	}

	try {
		const { request } = await publicClient.simulateContract({
			account,
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "approve",
			args: [event.nonce, event.recipient, event.palletAmount],
		});
		// Pad the gas limit generously. The SAME approve call executes one of two
		// very different paths depending on what has landed by the time it is
		// mined: either it merely records an approval, or it is the one that
		// crosses the threshold and therefore performs the release, including an
		// ERC-20 transfer. Gas estimated while the cheap path applied is not
		// enough for the expensive one, and with several attestors racing the
		// same migration that reordering is the normal case, not an edge case.
		// An under-estimate reverts OutOfGas, which reads as an unexplained
		// failure and takes the daemon down.
		const gasLimit = (request.gas ?? (await publicClient.estimateContractGas({
			account,
			address: config.vaultAddress,
			abi: vaultAbi,
			functionName: "approve",
			args: [event.nonce, event.recipient, event.palletAmount],
		}))) * GAS_LIMIT_MULTIPLIER;
		const txHash = await walletClient.writeContract({ ...request, gas: gasLimit });
		const receipt = await publicClient.waitForTransactionReceipt({ hash: txHash });
		if (receipt.status !== "success") {
			throw new Error(`approve transaction reverted: ${txHash} (${label})`);
		}
		log(`approved: ${label} tx=${txHash}`);
	} catch (error) {
		// Expected race: the release landed (or our own retried tx landed)
		// between our pre-check and the transaction. Benign — anything else
		// is a genuine failure and propagates to the fatal handler.
		if (await alreadyHandled(event)) {
			log(`skip (raced, resolved on-chain): ${label}`);
			return;
		}
		throw error;
	}
}

async function checkGasBalance(): Promise<void> {
	const balance = await publicClient.getBalance({ address: account.address });
	if (balance < config.minGasBalanceWei) {
		await alert("gas balance low", `${account.address} holds ${balance} wei`);
	}
}

async function main(): Promise<void> {
	const isAttestor = await publicClient.readContract({
		address: config.vaultAddress,
		abi: vaultAbi,
		functionName: "isAttestor",
		args: [account.address],
	});
	if (!isAttestor) {
		throw new Error(`${account.address} is not an attestor of ${config.vaultAddress}`);
	}
	await checkGasBalance();

	const api = await ApiPromise.create({ provider: new WsProvider(config.pendulumWs) });
	const checkpoint = loadCheckpoint();
	log(`attestor ${account.address} starting after block ${checkpoint.lastProcessedBlock}`);

	let processing = Promise.resolve();
	await api.rpc.chain.subscribeFinalizedHeads((head) => {
		const finalized = head.number.toNumber();
		// Serialize: a slow Base transaction must not let block processing overlap.
		processing = processing.then(async () => {
			for (let block = checkpoint.lastProcessedBlock + 1; block <= finalized; block++) {
				const events = await migrationEventsInBlock(api, block);
				for (const event of events) {
					await approve(event);
				}
				checkpoint.lastProcessedBlock = block;
				saveCheckpoint(checkpoint);
			}
		}).catch(async (error) => {
			// PRD A5: never skip an event silently. Alert and exit; the process
			// manager restarts us and the checkpoint retries the failing block.
			await alert("fatal error, exiting", error);
			process.exit(1);
		});
	});

	setInterval(
		() => void checkGasBalance().catch((error) => console.error("gas balance check failed", error)),
		10 * 60 * 1000,
	);
}

main().catch(async (error) => {
	await alert("startup failed", error);
	process.exit(1);
});
