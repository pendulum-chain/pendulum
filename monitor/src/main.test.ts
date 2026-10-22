/**
 * Call-site tests for the poll loop in main.ts. Each test loads a fresh copy
 * of the module (it reads its configuration at import) against stand-in
 * chains: a JSON-RPC server for Base and a plain object shaped like the
 * polkadot.js API for Pendulum. The same server receives every page.
 */

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync } from "node:fs";
import { createServer, type IncomingMessage } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import type { ApiPromise } from "@polkadot/api";
import {
	type AbiEvent,
	type AbiFunction,
	decodeFunctionData,
	encodeAbiParameters,
	encodeEventTopics,
	type Hex,
	parseAbi,
	toHex,
} from "viem";

(globalThis as { penMonitorUnderTest?: boolean }).penMonitorUnderTest = true;

const VAULT = "0x1111111111111111111111111111111111111111";
const TOKEN = "0x2222222222222222222222222222222222222222";
const RECIPIENT = "0x4444444444444444444444444444444444444444";
const ATTESTORS = ["0xa1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1", "0xa2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2", "0xa3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3"];
const CF = 1_000_000n;
const SUPPLY = 150_000_000n * 10n ** 18n;
const PEN = 10n ** 12n;

const abi = parseAbi([
	"function totalReleased() view returns (uint256)",
	"function totalSwept() view returns (uint256)",
	"function conversionFactor() view returns (uint256)",
	"function token() view returns (address)",
	"function paused() view returns (bool)",
	"function threshold() view returns (uint256)",
	"function activeApprovals(bytes32 payload) view returns (uint256)",
	"function totalSupply() view returns (uint256)",
	"function balanceOf(address owner) view returns (uint256)",
	"function getTimestamp(bytes32 id) view returns (uint256)",
	"event Approved(uint64 indexed nonce, address indexed recipient, uint256 palletAmount, address indexed attestor)",
	"event Released(uint64 indexed nonce, address indexed recipient, uint256 palletAmount, uint256 tokenAmount)",
	"event CallScheduled(bytes32 indexed id, uint256 indexed index, address target, uint256 value, bytes data, bytes32 predecessor, uint256 delay)",
]);

interface Log {
	address: string;
	eventName: string;
	args: Record<string, unknown>;
	blockNumber: bigint;
}

/** The Base side: the vault, its token and the governance timelock. */
function baseChain() {
	return {
		chainId: 8453,
		safe: 1_010n,
		/** How far the confirmed head trails the wall clock. */
		safeAgeSeconds: 2,
		paused: false,
		threshold: 3n,
		totalReleased: 0n,
		totalSwept: 0n,
		vaultBalance: SUPPLY,
		approvals: new Map<string, bigint>(),
		readyAt: new Map<string, bigint>(),
		logs: [] as Log[],
	};
}
type Base = ReturnType<typeof baseChain>;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function answer(base: Base, method: string, params: any[]): unknown {
	switch (method) {
		case "eth_chainId":
			return toHex(base.chainId);
		case "eth_getCode":
			return "0x";
		case "eth_getBlockByNumber": {
			const number = String(params[0]).startsWith("0x") ? BigInt(params[0]) : base.safe;
			const timestamp = Math.floor(Date.now() / 1000) - base.safeAgeSeconds;
			return { number: toHex(number), hash: toHex(number, { size: 32 }), timestamp: toHex(timestamp), transactions: [] };
		}
		case "eth_call": {
			const { functionName, args } = decodeFunctionData({ abi, data: params[0].data });
			const key = (args as readonly unknown[] | undefined)?.[0] as string;
			const result = {
				totalReleased: base.totalReleased,
				totalSwept: base.totalSwept,
				conversionFactor: CF,
				token: TOKEN,
				paused: base.paused,
				threshold: base.threshold,
				activeApprovals: base.approvals.get(key) ?? 0n,
				totalSupply: SUPPLY,
				balanceOf: base.vaultBalance,
				getTimestamp: base.readyAt.get(key) ?? 0n,
			}[functionName];
			const { outputs } = abi.find((item) => item.type === "function" && item.name === functionName) as AbiFunction;
			return encodeAbiParameters(outputs, [result]);
		}
		case "eth_getLogs": {
			const [{ address, topics, fromBlock, toBlock }] = params;
			const wanted = [topics?.[0] ?? []].flat() as string[];
			return base.logs.flatMap((log, index) => {
				if (log.address.toLowerCase() !== String(address).toLowerCase()) return [];
				if (log.blockNumber < BigInt(fromBlock) || log.blockNumber > BigInt(toBlock)) return [];
				const logTopics = encodeEventTopics({ abi, eventName: log.eventName, args: log.args } as never) as Hex[];
				if (wanted.length && !wanted.includes(logTopics[0])) return [];
				const { inputs } = abi.find((item) => item.type === "event" && item.name === log.eventName) as AbiEvent;
				const data = inputs.filter((input) => !input.indexed);
				return [{
					address: log.address,
					topics: logTopics,
					data: encodeAbiParameters(data, data.map((input) => log.args[input.name!])),
					blockNumber: toHex(log.blockNumber),
					blockHash: toHex(log.blockNumber, { size: 32 }),
					transactionHash: toHex(index + 1, { size: 32 }),
					transactionIndex: "0x0",
					logIndex: toHex(index),
					removed: false,
				}];
			});
		}
	}
	throw new Error(`unmocked ${method}`);
}

/** A migration approved by three attestors and released on Base. */
function release(base: Base, nonce: bigint, palletAmount: bigint, blockNumber = 1_005n): void {
	for (const attestor of ATTESTORS) {
		base.logs.push({ address: VAULT, eventName: "Approved", args: { nonce, recipient: RECIPIENT, palletAmount, attestor }, blockNumber });
	}
	const tokenAmount = palletAmount * CF;
	base.logs.push({ address: VAULT, eventName: "Released", args: { nonce, recipient: RECIPIENT, palletAmount, tokenAmount }, blockNumber });
	base.totalReleased += tokenAmount;
	base.vaultBalance -= tokenAmount;
}

interface Call {
	section: string;
	method: string;
	args: unknown[];
}

/** Calls the stand-in registry decodes; a call's bytes are its name. */
const CALLS: Record<string, Call> = {
	remark: { section: "system", method: "remark", args: ["0x"] },
	mint: { section: "utility", method: "batch", args: [[{ section: "tokens", method: "setBalance", args: [] }]] },
	setTreasuryDestination: { section: "tokenMigration", method: "setTreasuryDestination", args: ["0x55"] },
};
const encodeCall = (name: string) => new TextEncoder().encode(name);
function decodeCall(bytes: Uint8Array): Call {
	const call = CALLS[new TextDecoder().decode(bytes)];
	if (!call) throw new Error("Unable to find Call");
	return call;
}
const codec = (value: number | bigint) => ({
	toString: () => String(value),
	toNumber: () => Number(value),
	toBigInt: () => BigInt(value),
});
const hash = (hex: string) => ({ toHex: () => hex });
const inline = (name: string) => ({ isInline: true, asInline: encodeCall(name), isLookup: false, isLegacy: false, toString: () => `inline ${name}` });
const lookup = (hex: string, len: number) => ({
	isInline: false,
	isLookup: true,
	asLookup: { hash_: hash(hex), len: codec(len) },
	isLegacy: false,
	toString: () => `lookup ${hex}`,
});
type Bounded = ReturnType<typeof inline> | ReturnType<typeof lookup>;

interface PendulumEvent {
	section: string;
	method: string;
	data: unknown[];
}

/** The Pendulum side: finalized blocks with their events, the migration
 *  pallet's totals and the governance queues. Block numbers double as hashes. */
function pendulumChain() {
	const chain = {
		finalized: 100,
		/** Age of the finalized head; earlier blocks are 12 s apart. */
		headAgeMs: 0,
		failure: undefined as Error | undefined,
		events: new Map<number, PendulumEvent[]>(),
		totalMigrated: 0n,
		nextNonce: 0n,
		publicProps: [] as Array<[number, Bounded]>,
		referenda: new Map<number, Bounded>(),
		/** Noted preimages by `${hash}:${len}`. */
		preimages: new Map<string, string>(),
		preimageReads: 0,
	};
	const optional = <T>(value: T | undefined) => (value === undefined ? { isSome: false } : { isSome: true, unwrap: () => value });
	const at = (block: number) => ({
		registry: { createType: (_type: string, bytes: Uint8Array) => decodeCall(bytes) },
		consts: { democracy: { launchPeriod: codec(36_000) } },
		query: {
			timestamp: { now: async () => codec(Date.now() - chain.headAgeMs - (chain.finalized - block) * 12_000) },
			system: { events: async () => (chain.events.get(block) ?? []).map((event) => ({ event })) },
			tokenMigration: {
				totalMigrated: async () => codec(chain.totalMigrated),
				nextNonce: async () => codec(chain.nextNonce),
			},
			balances: { totalIssuance: async () => codec(100_000_000n * PEN - chain.totalMigrated) },
			democracy: {
				publicProps: async () => chain.publicProps.map(([index, bounded]) => [codec(index), bounded, "5Proposer"]),
				nextExternal: async () => optional(undefined),
				referendumCount: async () => codec(chain.referenda.size ? Math.max(...chain.referenda.keys()) + 1 : 0),
				lowestUnbaked: async () => codec(chain.referenda.size ? Math.min(...chain.referenda.keys()) : 0),
				referendumInfoOf: async (index: number) => {
					const proposal = chain.referenda.get(index);
					return optional(proposal && { isOngoing: true, asOngoing: { proposal, end: codec(9_000_000) } });
				},
			},
			preimage: {
				preimageFor: async ([preimage, len]: [{ toHex(): string }, unknown]) => {
					chain.preimageReads++;
					const name = chain.preimages.get(`${preimage.toHex()}:${len}`);
					return optional(name === undefined ? undefined : encodeCall(name));
				},
			},
		},
	});
	const api = {
		genesisHash: hash(`0x${"5d".repeat(32)}`),
		rpc: {
			chain: {
				getFinalizedHead: async () => {
					if (chain.failure) throw chain.failure;
					return chain.finalized;
				},
				getHeader: async (block: number) => ({ number: codec(block) }),
				getBlockHash: async (block: number) => block,
			},
		},
		at: async (block: number) => at(block),
	};
	return { chain, api: api as unknown as ApiPromise };
}
type Pendulum = ReturnType<typeof pendulumChain>["chain"];

/** A finalized burn on Pendulum, in block 101 unless stated. */
function burn(pendulum: Pendulum, nonce: bigint, palletAmount: bigint, block = 101): void {
	pendulum.finalized = Math.max(pendulum.finalized, block);
	const events = pendulum.events.get(block) ?? [];
	events.push({
		section: "tokenMigration",
		method: "MigrationInitiated",
		data: [codec(nonce), "5Migrator", hash(RECIPIENT), codec(palletAmount)],
	});
	pendulum.events.set(block, events);
	pendulum.totalMigrated += palletAmount;
	pendulum.nextNonce = nonce + 1n;
}

interface Page {
	subject: string;
	detail: string;
	status?: number;
}

async function body(request: IncomingMessage): Promise<string> {
	let text = "";
	for await (const chunk of request) text += chunk;
	return text;
}

let instances = 0;
let monitorEnv: string[] = [];

/** A started monitor against fresh stand-ins. `webhook` answers each page
 *  with an HTTP status; `pages` lists them in arrival order from after
 *  startup, `startup` the ones before. */
async function setup(
	t: { after(fn: () => void): void },
	env: Record<string, string> = {},
	webhook: (page: Page) => number | Promise<number> = () => 200,
) {
	const base = baseChain();
	const { chain: pendulum, api } = pendulumChain();
	const pages: Page[] = [];
	const server = createServer(async (request, response) => {
		const text = await body(request);
		if (request.url === "/alert") {
			const { subject, detail } = JSON.parse(text) as Page;
			const page: Page = { subject, detail };
			pages.push(page);
			page.status = await webhook(page);
			response.writeHead(page.status).end();
			return;
		}
		const { id, method, params } = JSON.parse(text);
		let reply: object;
		try {
			reply = { result: answer(base, method, params) };
		} catch (error) {
			reply = { error: { code: -32603, message: String(error) } };
		}
		response.setHeader("content-type", "application/json");
		response.end(JSON.stringify({ jsonrpc: "2.0", id, ...reply }));
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	t.after(() => {
		server.closeAllConnections();
		server.close();
	});
	const url = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
	const stateFile = join(mkdtempSync(join(tmpdir(), "pen-monitor-test-")), "state.json");
	const variables: Record<string, string> = {
		PENDULUM_WS: "ws://127.0.0.1:1",
		BASE_RPC_URL: url,
		VAULT_ADDRESS: VAULT,
		BASE_START_BLOCK: "1000",
		PENDULUM_START_BLOCK: "101",
		STATE_FILE: stateFile,
		ALERT_WEBHOOK_URL: `${url}/alert`,
		...env,
	};
	for (const name of monitorEnv) delete process.env[name];
	monitorEnv = Object.keys(variables);
	Object.assign(process.env, variables);
	const monitor: typeof import("./main.js") = await import(`./main.js?instance=${++instances}`);
	await monitor.start(api);
	const startup = pages.splice(0);
	return {
		base,
		pendulum,
		api,
		pages,
		startup,
		monitor,
		subjects: () => pages.map((page) => page.subject),
		persisted: () => JSON.parse(readFileSync(stateFile, "utf8")) as { baseFromBlock: string; migrations: Array<{ nonce: string }> },
	};
}

test("a released migration reconciles and the poll pages nothing", async (t) => {
	const { base, pendulum, api, monitor, subjects, persisted } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(subjects(), []);
	assert.deepEqual(persisted().migrations, []);
	assert.equal(persisted().baseFromBlock, "1011");
});

test("a vault balance deficit pages and asks for the auto-pause", async (t) => {
	const { base, api, monitor, subjects } = await setup(t);
	base.vaultBalance -= 1n;
	await monitor.poll(api);
	assert.deepEqual(subjects(), ["VAULT BALANCE DEFICIT", "AUTO-PAUSE UNAVAILABLE"]);
});
