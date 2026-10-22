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
	keccak256,
	parseAbi,
	toEventSelector,
	toFunctionSelector,
	toHex,
} from "viem";
import { privateKeyToAccount } from "viem/accounts";

(globalThis as { penMonitorUnderTest?: boolean }).penMonitorUnderTest = true;

const VAULT = "0x1111111111111111111111111111111111111111";
const TOKEN = "0x2222222222222222222222222222222222222222";
const TIMELOCK = "0x3333333333333333333333333333333333333333";
const RECIPIENT = "0x4444444444444444444444444444444444444444";
const ATTESTORS = ["0xa1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1", "0xa2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2", "0xa3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3"];
const CF = 1_000_000n;
const SUPPLY = 150_000_000n * 10n ** 18n;
const PEN = 10n ** 12n;
const GUARDIAN_KEY = `0x${"11".repeat(32)}` as const;
const GUARDIAN = privateKeyToAccount(GUARDIAN_KEY).address;

const abi = parseAbi([
	"function totalReleased() view returns (uint256)",
	"function totalSwept() view returns (uint256)",
	"function conversionFactor() view returns (uint256)",
	"function token() view returns (address)",
	"function paused() view returns (bool)",
	"function threshold() view returns (uint256)",
	"function guardian() view returns (address)",
	"function activeApprovals(bytes32 payload) view returns (uint256)",
	"function totalSupply() view returns (uint256)",
	"function balanceOf(address owner) view returns (uint256)",
	"function getTimestamp(bytes32 id) view returns (uint256)",
	"event Approved(uint64 indexed nonce, address indexed recipient, uint256 palletAmount, address indexed attestor)",
	"event Released(uint64 indexed nonce, address indexed recipient, uint256 palletAmount, uint256 tokenAmount)",
	"event RemainderSwept(address indexed to, uint256 amount)",
	"event GuardianUpdated(address indexed guardian)",
	"event CapsUpdated(uint256 perReleaseCap, uint256 dailyCap)",
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
		guardian: GUARDIAN as string,
		totalReleased: 0n,
		totalSwept: 0n,
		vaultBalance: SUPPLY,
		approvals: new Map<string, bigint>(),
		readyAt: new Map<string, bigint>(),
		/** Ether balances by lowercased address. */
		balances: new Map<string, bigint>([[GUARDIAN.toLowerCase(), 10n ** 18n]]),
		logs: [] as Log[],
		/** Where set, log queries answer as a backend whose head is this block. */
		logsHead: undefined as bigint | undefined,
		/** Where set and it returns a block, the log query for these topics
		 *  answers as a backend whose head is that block. */
		logsHeadFor: undefined as ((topics: string[]) => bigint | undefined) | undefined,
		/** Where set, the vault has no code before this block. */
		deployedAt: undefined as bigint | undefined,
		/** Requests to answer with an RPC error. */
		failing: undefined as ((method: string, params: unknown[]) => boolean) | undefined,
	};
}

/** A vault total at `block`: its current value less what later events added
 *  (`sign` -1 for the balance, which those events lowered). */
function totalAt(base: Base, total: bigint, eventName: string, field: string, block: bigint | undefined, sign = 1n): bigint {
	const later = base.logs.filter((log) => log.eventName === eventName && block !== undefined && log.blockNumber > block);
	return later.reduce((sum, log) => sum - sign * (log.args[field] as bigint), total);
}
type Base = ReturnType<typeof baseChain>;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function answer(base: Base, method: string, params: any[]): unknown {
	if (base.failing?.(method, params)) throw new Error("rate limited");
	switch (method) {
		case "eth_chainId":
			return toHex(base.chainId);
		case "eth_getCode": {
			const block = String(params[1]).startsWith("0x") ? BigInt(params[1]) : undefined;
			const deployed = base.deployedAt !== undefined && (block === undefined || block >= base.deployedAt);
			return String(params[0]).toLowerCase() === VAULT && deployed ? "0x6001" : "0x";
		}
		case "eth_getBalance":
			return toHex(base.balances.get(String(params[0]).toLowerCase()) ?? 0n);
		case "eth_getBlockByNumber": {
			const number = String(params[0]).startsWith("0x") ? BigInt(params[0]) : base.safe;
			const timestamp = Math.floor(Date.now() / 1000) - base.safeAgeSeconds;
			return { number: toHex(number), hash: toHex(number, { size: 32 }), timestamp: toHex(timestamp), transactions: [] };
		}
		case "eth_call": {
			const { functionName, args } = decodeFunctionData({ abi, data: params[0].data });
			const key = (args as readonly unknown[] | undefined)?.[0] as string;
			const block = String(params[1]).startsWith("0x") ? BigInt(params[1]) : undefined;
			const undeployed = base.deployedAt !== undefined && block !== undefined && block < base.deployedAt;
			if (params[0].to.toLowerCase() === VAULT && undeployed) return "0x";
			const result = {
				totalReleased: totalAt(base, base.totalReleased, "Released", "tokenAmount", block),
				totalSwept: totalAt(base, base.totalSwept, "RemainderSwept", "amount", block),
				conversionFactor: CF,
				token: TOKEN,
				paused: base.paused,
				threshold: base.threshold,
				guardian: base.guardian,
				activeApprovals: base.approvals.get(key) ?? 0n,
				totalSupply: SUPPLY,
				balanceOf: totalAt(
					base,
					totalAt(base, base.vaultBalance, "Released", "tokenAmount", block, -1n),
					"RemainderSwept",
					"amount",
					block,
					-1n,
				),
				getTimestamp: base.readyAt.get(key) ?? 0n,
			}[functionName];
			const { outputs } = abi.find((item) => item.type === "function" && item.name === functionName) as AbiFunction;
			return encodeAbiParameters(outputs, [result]);
		}
		case "eth_getLogs": {
			const [{ address, topics, fromBlock, toBlock }] = params;
			const wanted = [topics?.[0] ?? []].flat() as string[];
			const head = base.logsHeadFor?.(wanted) ?? base.logsHead;
			return base.logs.flatMap((log, index) => {
				if (log.address.toLowerCase() !== String(address).toLowerCase()) return [];
				if (log.blockNumber < BigInt(fromBlock) || log.blockNumber > BigInt(toBlock)) return [];
				if (head !== undefined && log.blockNumber > head) return [];
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
const hash = (hex: string) => ({ toHex: () => hex, toString: () => hex, isEmpty: /^0x0*$/.test(hex) });
const inline = (name: string) => ({ isInline: true, asInline: encodeCall(name), isLookup: false, isLegacy: false, toString: () => `inline ${name}` });
const lookup = (hex: string, len: number) => ({
	isInline: false,
	isLookup: true,
	asLookup: { hash_: hash(hex), len: codec(len) },
	isLegacy: false,
	toString: () => `lookup ${hex}`,
});
const legacy = (hex: string) => ({
	isInline: false,
	isLookup: false,
	isLegacy: true,
	asLegacy: { hash_: hash(hex) },
	toString: () => `legacy ${hex}`,
});
type Bounded = ReturnType<typeof inline> | ReturnType<typeof lookup> | ReturnType<typeof legacy>;

interface PendulumEvent {
	section: string;
	method: string;
	data: unknown[];
}

/** The Pendulum side: finalized blocks with their events, the migration
 *  pallet's totals and the governance queues. A block hash encodes the block
 *  number, and whether it is on the fork the node serves from `forkFrom` on. */
function pendulumState() {
	return {
		/** Where set, the node serves another chain from this block on. */
		forkFrom: undefined as number | undefined,
		/** Blocks whose header the node alters, so it no longer hashes to its hash. */
		alteredHeaders: new Set<number>(),
		/** Blocks whose header names the next block number. */
		renumberedHeaders: new Set<number>(),
		/** Whether the node names block hashes by number from the chain without the fork. */
		canonicalBlockHashes: false,
		finalized: 100,
		/** The newest block the node imported, where it is past the finalized one. */
		best: undefined as number | undefined,
		/** Age of the finalized head; earlier blocks are 12 s apart. */
		headAgeMs: 0,
		failure: undefined as Error | undefined,
		/** Holds every finalized-head read until it settles, as a slow node would. */
		gate: undefined as Promise<void> | undefined,
		/** Finalized-head reads so far. */
		finalizedReads: 0,
		events: new Map<number, PendulumEvent[]>(),
		totalMigrated: 0n,
		nextNonce: 0n,
		publicProps: [] as Array<[number, Bounded]>,
		external: undefined as Bounded | undefined,
		referenda: new Map<number, Bounded>(),
		/** Noted preimages by `${hash}:${len}`, and their lengths by hash (old:
		 *  noted before the pallet's status migration). */
		preimages: new Map<string, string>(),
		notedLengths: new Map<string, number>(),
		oldNotedLengths: new Map<string, number>(),
		preimageReads: 0,
		/** Preimage reads still to fail, as an RPC error would. */
		preimageFailures: 0,
		/** Open motions by hash, their index where it is not 0, the stored size
		 *  where it is not the call's, and the motions whose call was fetched. */
		motions: { council: new Map<string, string>(), technicalCommittee: new Map<string, string>() },
		motionIndices: new Map<string, number>(),
		motionSizes: new Map<string, number>(),
		motionReads: new Set<string>(),
	};
}
type Pendulum = ReturnType<typeof pendulumState>;

/** A node serving `chain`, as much of the polkadot.js API as the monitor uses. */
function pendulumChain(chain: Pendulum = pendulumState()) {
	const collective = (motions: Map<string, string>) => ({
		proposals: async () => [...motions.keys()].map(hash),
		voting: async (motion: { toHex(): string }) => ({
			toJSON: () => (motions.has(motion.toHex()) ? { index: chain.motionIndices.get(motion.toHex()) ?? 0 } : null),
		}),
		proposalOf: Object.assign(
			async (motion: { toHex(): string }) => {
				chain.motionReads.add(motion.toHex());
				const name = motions.get(motion.toHex());
				return optional(name === undefined ? undefined : decodeCall(encodeCall(name)));
			},
			{
				size: async (motion: { toHex(): string }) =>
					codec(chain.motionSizes.get(motion.toHex()) ?? encodeCall(motions.get(motion.toHex()) ?? "").length),
			},
		),
	});
	const optional = <T>(value: T | undefined) => (value === undefined ? { isSome: false } : { isSome: true, unwrap: () => value });
	/** The migration totals at `block`: the current ones less the burns of later blocks. */
	const totalsAt = (block: number) => {
		let { nextNonce, totalMigrated } = chain;
		for (const [later, events] of chain.events) {
			for (const { method, data } of later > block ? events : []) {
				if (method !== "MigrationInitiated") continue;
				nextNonce--;
				totalMigrated -= (data[3] as { toBigInt(): bigint }).toBigInt();
			}
		}
		return { nextNonce, totalMigrated };
	};
	const at = (block: number) => ({
		registry: { createType: (_type: string, bytes: Uint8Array) => decodeCall(bytes) },
		consts: { democracy: { launchPeriod: codec(36_000) } },
		query: {
			timestamp: { now: async () => codec(Date.now() - chain.headAgeMs - (chain.finalized - block) * 12_000) },
			system: { events: async () => (chain.events.get(block) ?? []).map((event) => ({ event })) },
			tokenMigration: {
				totalMigrated: async () => codec(totalsAt(block).totalMigrated),
				nextNonce: async () => codec(totalsAt(block).nextNonce),
			},
			balances: { totalIssuance: async () => codec(100_000_000n * PEN - totalsAt(block).totalMigrated) },
			democracy: {
				publicProps: async () => chain.publicProps.map(([index, bounded]) => [codec(index), bounded, "5Proposer"]),
				nextExternal: async () => optional(chain.external && [chain.external, "SimpleMajority"]),
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
					if (chain.preimageFailures > 0 && chain.preimageFailures--) throw new Error("request timed out");
					const name = chain.preimages.get(`${preimage.toHex()}:${len}`);
					return optional(name === undefined ? undefined : encodeCall(name));
				},
				requestStatusFor: async (preimage: { toHex(): string }) => {
					const len = chain.notedLengths.get(preimage.toHex());
					return { toJSON: () => (len === undefined ? null : { unrequested: { ticket: ["5Noter", 1], len } }) };
				},
				statusFor: async (preimage: { toHex(): string }) => {
					const len = chain.oldNotedLengths.get(preimage.toHex());
					return { toJSON: () => (len === undefined ? null : { unrequested: { deposit: ["5Noter", 1], len } }) };
				},
			},
			council: collective(chain.motions.council),
			technicalCommittee: collective(chain.motions.technicalCommittee),
		},
	});
	const api = {
		genesisHash: hash(`0x${"5d".repeat(32)}`),
		rpc: {
			chain: {
				getFinalizedHead: async () => {
					chain.finalizedReads++;
					await chain.gate;
					if (chain.failure) throw chain.failure;
					return hash(pendulumHash(chain, chain.finalized));
				},
				getHeader: async (value: string | { toHex(): string }) => {
					const requested = typeof value === "string" ? value : value.toHex();
					const block = blockOf(requested);
					const onFork = BigInt(requested) >> 64n !== 0n;
					return {
						number: codec(chain.renumberedHeaders.has(block) ? block + 1 : block),
						parentHash: hash(pendulumHash(onFork ? chain : {}, block - 1)),
						hash: hash(chain.alteredHeaders.has(block) ? pendulumHash(chain, block + 1_000_000) : requested),
					};
				},
				getBlockHash: async (block: number) =>
					hash(
						block > (chain.best ?? chain.finalized)
							? toHex(0, { size: 32 })
							: pendulumHash(chain.canonicalBlockHashes ? {} : chain, block),
					),
			},
		},
		/** Decodes the head data of the stand-in relay chain. */
		createType: (_type: string, bytes: Uint8Array) => {
			const header = JSON.parse(new TextDecoder().decode(bytes)) as { number: number; hash: string };
			return { number: codec(header.number), hash: hash(header.hash) };
		},
		at: async (value: string | { toHex(): string }) => at(blockOf(typeof value === "string" ? value : value.toHex())),
	};
	return { chain, api: api as unknown as ApiPromise };
}

/** A relay chain whose finalized block records `head()` as the Pendulum head;
 *  every read fails while `failing()` holds. */
function relayChain(head: () => { number: number; hash: string }, failing = () => false): ApiPromise {
	const check = () => {
		if (failing()) throw new Error("relay RPC unavailable");
	};
	const relay = {
		rpc: { chain: { getFinalizedHead: async () => (check(), hash(`0x${"77".repeat(32)}`)) } },
		at: async () => ({
			query: {
				paras: {
					heads: async (paraId: number) => {
						check();
						const data = new TextEncoder().encode(JSON.stringify(head()));
						return paraId === 2094 ? { isSome: true, unwrap: () => ({ toU8a: () => data }) } : { isSome: false };
					},
				},
			},
		}),
	};
	return relay as unknown as ApiPromise;
}

/** The hash of `block` on the chain the node serves. */
function pendulumHash(chain: { forkFrom?: number }, block: number): string {
	const fork = chain.forkFrom !== undefined && block >= chain.forkFrom ? 1n << 64n : 0n;
	return toHex(fork + BigInt(block), { size: 32 });
}
const blockOf = (blockHash: string) => Number(BigInt(blockHash) & 0xffff_ffff_ffff_ffffn);

const MIGRATOR = `0x${"ab".repeat(32)}`;
const TREASURY_ACCOUNT = `0x${Buffer.from("modlpy/trsry").toString("hex")}${"00".repeat(20)}`;

/** A finalized burn on Pendulum by `who` for `recipient`, in block 101 unless stated. */
function burn(
	pendulum: Pendulum,
	nonce: bigint,
	palletAmount: bigint,
	block = 101,
	{ who = MIGRATOR, recipient = RECIPIENT } = {},
): void {
	pendulum.finalized = Math.max(pendulum.finalized, block);
	const events = pendulum.events.get(block) ?? [];
	events.push({
		section: "tokenMigration",
		method: "MigrationInitiated",
		data: [codec(nonce), hash(who), hash(recipient), codec(palletAmount)],
	});
	pendulum.events.set(block, events);
	pendulum.totalMigrated += palletAmount;
	pendulum.nextNonce = nonce + 1n;
}

interface Page {
	subject: string;
	detail: string;
	severity: string;
	/** The Slack message as sent. */
	message: Record<string, unknown>;
	status?: number;
}

/** A page from the Slack message the monitor sent, with Slack's escapes undone. */
function readPage(message: Record<string, unknown>): Page {
	const [, severity, subject, detail] = /^\[(CRITICAL|WARNING)\] pen-monitor: (.*)\n([\s\S]*)$/.exec(String(message.text)) ?? [];
	assert.ok(subject, `not a monitor page: ${JSON.stringify(message)}`);
	const unescaped = detail.replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&amp;", "&");
	return { subject, detail: unescaped, severity, message };
}

async function body(request: IncomingMessage): Promise<string> {
	let text = "";
	for await (const chunk of request) text += chunk;
	return text;
}

let instances = 0;
let monitorEnv: string[] = [];

/** A started monitor against fresh stand-ins, which `prepare` can change
 *  before startup. `webhook` answers each page with an HTTP status; `pages`
 *  lists them in arrival order from after startup, `startup` the ones before.
 *  `heartbeats` lists the heartbeat requests, which `heartbeat` answers. */
async function setup(
	t: { after(fn: () => void): void },
	{
		env = {},
		webhook = () => 200,
		heartbeat = () => 200,
		prepare = () => {},
		connect,
	}: {
		env?: Record<string, string>;
		webhook?: (page: Page) => number | Promise<number>;
		heartbeat?: () => number;
		prepare?: (base: Base, pendulum: Pendulum) => void;
		/** Reaches the other nodes: the second Pendulum node and relay RPCs. */
		connect?: (url: string) => Promise<ApiPromise>;
	} = {},
) {
	const base = baseChain();
	const { chain: pendulum, api } = pendulumChain();
	prepare(base, pendulum);
	const pages: Page[] = [];
	const heartbeats: string[] = [];
	const server = createServer(async (request, response) => {
		const text = await body(request);
		if (request.url === "/heartbeat") {
			heartbeats.push(request.method ?? "");
			response.writeHead(heartbeat()).end();
			return;
		}
		if (request.url === "/alert") {
			const page = readPage(JSON.parse(text));
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
		HEARTBEAT_URL: `${url}/heartbeat`,
		...env,
	};
	for (const name of monitorEnv) delete process.env[name];
	monitorEnv = Object.keys(variables);
	Object.assign(process.env, variables);
	const launch = async () => {
		const monitor: typeof import("./main.js") = await import(`./main.js?instance=${++instances}`);
		await monitor.start(api, connect ?? (async (url) => assert.fail(`unexpected connection to ${url}`)));
		return monitor;
	};
	const monitor = await launch();
	const startup = pages.splice(0);
	return {
		base,
		pendulum,
		api,
		pages,
		startup,
		heartbeats,
		monitor,
		/** A new monitor process on the same state file and stand-ins. */
		restart: launch,
		subjects: () => pages.map((page) => page.subject),
		persisted: () => JSON.parse(readFileSync(stateFile, "utf8")) as {
			lastPendulumBlock: number;
			lastPendulumHash: string;
			relayAnchor?: { number: number; hash: string };
			baseFromBlock: string;
			baseAdminFromBlock: string;
			migrations: Array<{ nonce: string }>;
			mismatchedApprovals: Array<{ nonce: string; attestor: string }>;
		},
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

/** Pages sent concurrently can arrive in either order. */
const sorted = (subjects: string[]) => [...subjects].sort();

test("pages are Slack messages carrying severity, subject and detail in their text only", async (t) => {
	const { base, api, monitor, pages } = await setup(t);
	base.vaultBalance -= 1n;
	base.safeAgeSeconds = 601;
	await monitor.poll(api);
	assert.ok(pages.length >= 3, JSON.stringify(pages));
	for (const page of pages) assert.deepEqual(Object.keys(page.message), ["text"]);
	const severity = (subject: string) => pages.find((page) => page.subject === subject)?.severity;
	assert.equal(severity("VAULT BALANCE DEFICIT"), "CRITICAL");
	assert.equal(severity("AUTO-PAUSE UNAVAILABLE"), "CRITICAL");
	assert.equal(severity("BASE VIEW STALLED"), "WARNING");
	assert.match(pages.find((page) => page.subject === "VAULT BALANCE DEFICIT")!.detail, /^balance \d+ \+ released 0 \+ swept 0 < supply/);
});

test("only a poll that runs every check requests the heartbeat", async (t) => {
	const { base, pendulum, api, monitor, heartbeats } = await setup(t);
	await monitor.poll(api);
	assert.deepEqual(heartbeats, ["GET"]);
	pendulum.failure = new Error("node unreachable");
	await monitor.poll(api);
	pendulum.failure = undefined;
	const guardianUpdated = toEventSelector("GuardianUpdated(address)"); // only the admin scan asks for it
	base.failing = (method, params) => method === "eth_getLogs" && JSON.stringify(params).includes(guardianUpdated);
	base.safe = 1_020n; // a new range for the admin scan
	await monitor.poll(api);
	assert.deepEqual(heartbeats, ["GET"]);
	base.failing = undefined;
	await monitor.poll(api);
	assert.deepEqual(heartbeats, ["GET", "GET"]);
});

test("a failing heartbeat never stops the monitor", async (t) => {
	const { api, monitor, heartbeats, subjects } = await setup(t, { heartbeat: () => 500 });
	await monitor.poll(api);
	await monitor.poll(api);
	assert.deepEqual(heartbeats, ["GET", "GET"]);
	const unreachable = await setup(t, { env: { HEARTBEAT_URL: "http://127.0.0.1:1/heartbeat" } });
	await unreachable.monitor.poll(unreachable.api);
	assert.deepEqual([...subjects(), ...unreachable.subjects()], []);
});

test("a vault balance deficit pages and asks for the auto-pause", async (t) => {
	const { base, api, monitor, subjects } = await setup(t);
	base.vaultBalance -= 1n;
	await monitor.poll(api);
	assert.deepEqual(sorted(subjects()), ["AUTO-PAUSE UNAVAILABLE", "VAULT BALANCE DEFICIT"]);
});

test("a Pendulum failure pages on its own and the vault balance check still runs", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t);
	pendulum.failure = new Error("node unreachable");
	base.vaultBalance -= 1n;
	await monitor.poll(api);
	assert.deepEqual(sorted(subjects()), [
		"AUTO-PAUSE UNAVAILABLE",
		"PENDULUM SOURCE UNREADABLE",
		"VAULT BALANCE DEFICIT",
		"pendulum governance watch failed",
	]);
});

test("a failing Base log read does not skip the vault balance check", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t, { env: { GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY } });
	pendulum.failure = new Error("node unreachable");
	const released = toEventSelector("Released(uint64,address,uint256,uint256)"); // only reconciliation asks for it
	base.failing = (method, params) => method === "eth_getLogs" && JSON.stringify(params).includes(released);
	base.vaultBalance -= 1n;
	base.paused = true; // so the auto-pause only confirms
	await monitor.poll(api);
	assert.ok(subjects().includes("VAULT BALANCE DEFICIT"), JSON.stringify(subjects()));
	assert.ok(subjects().includes("vault pause confirmed"), JSON.stringify(subjects()));
});

/** Waits until `predicate` holds, failing after `ms`. */
async function until(predicate: () => boolean, label: string, ms = 2_000): Promise<void> {
	const deadline = Date.now() + ms;
	while (!predicate()) {
		if (Date.now() > deadline) assert.fail(`timed out waiting for ${label}`);
		await new Promise((resolve) => setTimeout(resolve, 10));
	}
}

test("a slow Pendulum source does not delay the vault balance check", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t);
	let respond = () => {};
	pendulum.gate = new Promise((resolve) => (respond = resolve));
	base.vaultBalance -= 1n;
	const polling = monitor.poll(api);
	try {
		await until(() => subjects().includes("VAULT BALANCE DEFICIT"), "the deficit page while Pendulum is still answering");
	} finally {
		respond();
		await polling;
	}
});

/** Holds every finalized-head read of `pendulum` until `respond` is called;
 *  `waiting` resolves once a read waits. */
function holdSource(pendulum: Pendulum) {
	let respond = () => {};
	pendulum.gate = new Promise((resolve) => (respond = resolve));
	const reads = pendulum.finalizedReads;
	return { respond, waiting: () => until(() => pendulum.finalizedReads > reads, "a poll waiting on the source") };
}

test("while a poll waits on a slow source, the vault balance check runs on its own", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t, { env: { POLL_INTERVAL_MS: "20" } });
	const { respond, waiting } = holdSource(pendulum);
	const polling = monitor.poll(api); // checks the balance, then waits on the source
	try {
		await waiting();
		await new Promise((resolve) => setTimeout(resolve, 30)); // a poll interval later
		base.vaultBalance -= 1n;
		await monitor.watchVaultBalance();
		assert.ok(subjects().includes("VAULT BALANCE DEFICIT"), JSON.stringify(subjects()));
	} finally {
		respond();
		await polling;
	}
});

test("the vault balance check does not run on its own within a poll interval of a poll's, or between polls", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t, { env: { POLL_INTERVAL_MS: "500" } });
	const { respond, waiting } = holdSource(pendulum);
	const polling = monitor.poll(api);
	try {
		await waiting();
		base.vaultBalance -= 1n;
		await monitor.watchVaultBalance(); // the poll checked the balance less than an interval ago
	} finally {
		respond();
		await polling;
	}
	await new Promise((resolve) => setTimeout(resolve, 550));
	await monitor.watchVaultBalance(); // no poll is running: the next one checks it
	assert.ok(!subjects().includes("VAULT BALANCE DEFICIT"), JSON.stringify(subjects()));
});

test("a pause attempt in flight is joined, not started again", async (t) => {
	let answerFailedPause = () => {};
	const failedPauseAnswered = new Promise<void>((resolve) => (answerFailedPause = resolve));
	const { base, pendulum, api, monitor, pages } = await setup(t, {
		env: { GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY, POLL_INTERVAL_MS: "20" },
		webhook: async ({ subject }) => {
			if (subject === "AUTO-PAUSE FAILED") await failedPauseAnswered; // holds that attempt in flight
			return 200;
		},
	});
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	const tokenAmount = 100n * PEN * CF; // a release of nonce 0 to another recipient
	base.logs.push({ address: VAULT, eventName: "Released", args: { nonce: 0n, recipient: TOKEN, palletAmount: 100n * PEN, tokenAmount }, blockNumber: 1_015n });
	base.totalReleased += tokenAmount;
	base.vaultBalance -= tokenAmount;
	base.safe = 1_020n;
	const { respond, waiting } = holdSource(pendulum);
	const polling = monitor.poll(api);
	const failedPauses = () => pages.filter((page) => page.subject === "AUTO-PAUSE FAILED").length;
	try {
		await waiting();
		await new Promise((resolve) => setTimeout(resolve, 30)); // a poll interval later
		base.vaultBalance -= 1n;
		const watching = monitor.watchVaultBalance(); // a deficit: the first pause attempt
		await until(() => failedPauses() === 1, "the first failed pause page");
		respond(); // the poll goes on to the release, a second violation
		await until(() => pages.some((page) => page.subject === "MIGRATION TUPLE VIOLATION"), "the second violation");
		await new Promise((resolve) => setTimeout(resolve, 50));
		answerFailedPause();
		await watching;
	} finally {
		respond();
		answerFailedPause();
		await polling;
	}
	assert.equal(failedPauses(), 1, JSON.stringify(pages.map((page) => page.subject)));
});

test("a source whose chain leaves the blocks ingested so far pages critical and ingests nothing", async (t) => {
	const { pendulum, api, monitor, restart, pages, subjects, persisted } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	const restarted = await restart();
	pendulum.forkFrom = 101; // the node now serves another chain from block 101 on
	burn(pendulum, 1n, 100n * PEN, 102);
	await restarted.poll(api);
	const page = pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED");
	assert.equal(page?.severity, "CRITICAL", JSON.stringify(pages));
	assert.match(page.detail, /chain has 0x0+10+65 at Pendulum block 101, where the monitor ingested 0x0+65;/);
	assert.ok(!subjects().includes("PENDULUM SOURCE UNREADABLE"), JSON.stringify(subjects()));
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
});

test("a source header that does not hash to its block hash pages critical", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	burn(pendulum, 0n, 100n * PEN, 102);
	pendulum.alteredHeaders.add(102);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/header for Pendulum block 102 does not hash to 0x0+66;/,
	);
});

test("a source header that names another block number pages critical", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await setup(t);
	burn(pendulum, 0n, 100n * PEN, 102);
	pendulum.renumberedHeaders.add(101);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/header for Pendulum block 101 does not hash to 0x0+65;/,
	);
	assert.equal(persisted().lastPendulumBlock, 100);
});

const SECOND_NODE = "ws://second-node.test:9944?apikey=secret";

/** A monitor whose second Pendulum node serves the source's chain, except for
 *  the values in `own`, which the test may change later; with `relayHead`,
 *  also against a relay chain recording it. */
async function crossCheckSetup(
	t: { after(fn: () => void): void },
	own: Partial<Pendulum> = {},
	relayHead?: () => { number: number; hash: string },
) {
	let source: Pendulum | undefined;
	const chain = new Proxy({} as Pendulum, {
		get: (_target, key) => (key in own ? own[key as keyof Pendulum] : source![key as keyof Pendulum]),
	});
	const second = pendulumChain(chain);
	const relay = relayHead && relayChain(relayHead);
	const monitor = await setup(t, {
		env: { PENDULUM_WS_CROSSCHECK: SECOND_NODE, ...(relay ? { POLKADOT_WS: "wss://relay.test" } : {}) },
		prepare: (_base, pendulum) => (source = pendulum),
		connect: async (url) => (url === SECOND_NODE ? second.api : (url === "wss://relay.test" && relay) || assert.fail(url)),
	});
	return { ...monitor, own };
}

test("a second node that disagrees on the newest block pages critical and nothing is ingested", async (t) => {
	const { pendulum, api, monitor, pages, persisted, own } = await crossCheckSetup(t, { finalized: 200 });
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.equal(persisted().lastPendulumBlock, 101);
	own.forkFrom = 102;
	burn(pendulum, 1n, 100n * PEN, 102);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/second Pendulum node has 0x0+10+66 at block 102, the source 0x0+66;/,
	);
	assert.equal(persisted().lastPendulumBlock, 101);
});

test("a second node that cannot answer only warns", async (t) => {
	const { pendulum, api, monitor, pages, persisted, heartbeats } = await setup(t, {
		env: { PENDULUM_WS_CROSSCHECK: SECOND_NODE },
		connect: async (url) => {
			throw new Error(`cannot connect to ${url}`);
		},
	});
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => [page.subject, page.severity]), [["PENDULUM CROSS-CHECK UNAVAILABLE", "WARNING"]]);
	assert.match(pages[0].detail, /cannot be read: Error: cannot connect to <url> /);
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(heartbeats, ["GET"]);
	await monitor.poll(api);
	assert.equal(pages.length, 1); // throttled
});

test("the URLs of the checking nodes are validated at startup", async (t) => {
	await assert.rejects(setup(t, { env: { POLKADOT_WS: "wss://relay.test, https://relay.test" } }), /invalid POLKADOT_WS/);
	await assert.rejects(
		setup(t, { env: { PENDULUM_WS_CROSSCHECK: "wss://a.test,wss://b.test" } }),
		/invalid PENDULUM_WS_CROSSCHECK: expected one ws/,
	);
});

test("a node connection that fails with an event says why", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t, {
		env: { PENDULUM_WS_CROSSCHECK: SECOND_NODE },
		connect: () => Promise.reject({ type: "error", message: "getaddrinfo ENOTFOUND second-node.test" }),
	});
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.match(pages[0]?.detail ?? "", /cannot be read: Error: getaddrinfo ENOTFOUND second-node\.test;/);
});

test("a block the second node has not finalized yet waits for it, without a page", async (t) => {
	const { pendulum, api, monitor, pages, persisted, own } = await crossCheckSetup(t, { finalized: 100 });
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.equal(persisted().lastPendulumBlock, 100);
	own.finalized = 101;
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
});

test("a second node whose best chain holds another newest block, not yet finalized, holds that block back", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await crossCheckSetup(t, { finalized: 100, best: 101, forkFrom: 101 });
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.equal(persisted().lastPendulumBlock, 100);
});

test("a source on another chain one block past the second node's head ingests nothing from that chain", async (t) => {
	const { base, pendulum, api, monitor, pages, persisted, own } = await crossCheckSetup(t, { finalized: 101, forkFrom: undefined });
	burn(pendulum, 0n, 100n * PEN);
	pendulum.forkFrom = 102; // the source serves another chain from block 102 on...
	burn(pendulum, 1n, 999n * PEN, 102); // ...with a fabricated burn, one block past the second node's head
	release(base, 1n, 999n * PEN);
	await monitor.poll(api);
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
	assert.equal(persisted().baseFromBlock, "1000"); // the release of the fabricated nonce awaits its burn
	assert.deepEqual([...new Set(pages.map((page) => page.subject))], ["UNVERIFIED BASE EVENT"]);
	own.finalized = 102; // the second node finalizes its own block 102...
	burn(pendulum, 2n, 999n * PEN, 103); // ...and the source stays one block ahead
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/second Pendulum node has 0x0+66 at block 102, the source 0x0+10+66;/,
	);
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
});

test("with the second node and the relay chain, a block past both their heads waits, and a fork there is caught", async (t) => {
	let recorded = 101;
	const { pendulum, api, monitor, pages, persisted, own } = await crossCheckSetup(
		t,
		{ finalized: 101, forkFrom: undefined },
		() => ({ number: recorded, hash: pendulumHash({}, recorded) }),
	);
	burn(pendulum, 0n, 100n * PEN);
	pendulum.forkFrom = 102; // the source finalizes another block 102...
	burn(pendulum, 1n, 999n * PEN, 102); // ...with a fabricated burn
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
	own.finalized = 102; // both checkers reach the real block 102
	recorded = 102;
	await monitor.poll(api);
	assert.ok(pages.some((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED"), JSON.stringify(pages));
	assert.equal(persisted().lastPendulumBlock, 101);
});

test("a second node whose finalized head is stale counts as unavailable", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await crossCheckSetup(t, { finalized: 100, headAgeMs: 400_000 });
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => [page.subject, page.severity]), [["PENDULUM CROSS-CHECK UNAVAILABLE", "WARNING"]]);
	assert.match(pages[0].detail, /has a finalized head 40\ds old;/);
	assert.equal(persisted().lastPendulumBlock, 101);
});

test("a second node with another nonce or total at the finalized head pages critical and nothing is ingested", async (t) => {
	for (const own of [{ nextNonce: 2n }, { totalMigrated: 7n * PEN }]) {
		const { pendulum, api, monitor, pages, persisted } = await crossCheckSetup(t, own);
		burn(pendulum, 0n, 100n * PEN);
		await monitor.poll(api);
		assert.match(
			pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
			/at Pendulum block 101 the second node has nextNonce (2|1) and totalMigrated (100|7)000000000000, the source 1 and 100000000000000;/,
		);
		assert.equal(persisted().lastPendulumBlock, 100);
	}
});

test("a second node with other migration events in a block pages critical and nothing is ingested", async (t) => {
	const { pendulum, api, monitor, pages, persisted, own } = await crossCheckSetup(t);
	burn(pendulum, 0n, 100n * PEN);
	const events = new Map(pendulum.events);
	own.events = events;
	burn(pendulum, 1n, 100n * PEN, 102);
	events.set(102, [{ ...pendulum.events.get(102)![0], data: [codec(1n), hash(MIGRATOR), hash(TOKEN), codec(100n * PEN)] }]);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		new RegExp(`in Pendulum block 102 the second node reports the migration events MigrationInitiated\\(1, ${MIGRATOR}, ${TOKEN}, `, "i"),
	);
	assert.equal(persisted().lastPendulumBlock, 100);
	assert.deepEqual(persisted().migrations, []);
});

test("a second node that cannot read a block's events only warns", async (t) => {
	const failing = new (class extends Map<number, PendulumEvent[]> {
		get(): never {
			throw new Error("state discarded");
		}
	})();
	const { pendulum, api, monitor, pages, persisted } = await crossCheckSetup(t, { events: failing });
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => page.subject), ["PENDULUM CROSS-CHECK UNAVAILABLE"]);
	assert.match(pages[0].detail, /cannot be read: .*state discarded/);
	assert.equal(persisted().lastPendulumBlock, 101);
});

const RELAY = "wss://relay.test";
type Head = { number: number; hash: string };

/** A monitor with the relay-chain anchor, against a relay chain recording
 *  `head` (by default the source's finalized head, on the chain without a fork). */
async function relaySetup(
	t: { after(fn: () => void): void },
	{ head, failing, env = {} }: { head?: (pendulum: Pendulum) => Head; failing?: () => boolean; env?: Record<string, string> } = {},
) {
	let chain: Pendulum | undefined;
	const honest = (pendulum: Pendulum) => ({ number: pendulum.finalized, hash: pendulumHash({}, pendulum.finalized) });
	const relay = relayChain(() => (head ?? honest)(chain!), failing);
	return setup(t, {
		env: { POLKADOT_WS: RELAY, ...env },
		prepare: (_base, pendulum) => (chain = pendulum),
		connect: async (url) => (url === RELAY ? relay : assert.fail(url)),
	});
}

test("a relay-chain record of the source's head anchors it and is kept", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.deepEqual(persisted().relayAnchor, { number: 101, hash: pendulumHash({}, 101) });
	assert.equal(persisted().lastPendulumBlock, 101);
});

test("a source that names another hash than the relay chain's record pages critical", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
		head: (pendulum) => ({ number: pendulum.finalized, hash: pendulumHash({ forkFrom: 0 }, pendulum.finalized) }),
	});
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/relay chain records 0x0+10+65 as Pendulum block 101, the source's chain has 0x0+65;/,
	);
	assert.equal(persisted().lastPendulumBlock, 100);
});

test("a source whose chain avoids the relay chain's record pages critical, whatever hash it names", async (t) => {
	const { pendulum, api, monitor, pages } = await relaySetup(t, {
		head: () => ({ number: 101, hash: pendulumHash({}, 101) }),
	});
	pendulum.forkFrom = 101;
	pendulum.canonicalBlockHashes = true; // it names the relay chain's hash for block 101
	burn(pendulum, 0n, 100n * PEN, 102);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/relay chain records 0x0+65 as Pendulum block 101, the source's chain has 0x0+10+65;/,
	);
});

test("a relay record past the source's finalized head anchors it, and its chain must keep passing through it", async (t) => {
	let relayDown = false;
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
		head: (pendulum) => ({ number: 102, hash: pendulumHash(pendulum, 102) }),
		failing: () => relayDown,
	});
	burn(pendulum, 0n, 100n * PEN);
	pendulum.best = 102; // imported, not yet finalized by the source
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.deepEqual(persisted().relayAnchor, { number: 102, hash: pendulumHash({}, 102) });
	assert.equal(persisted().lastPendulumBlock, 101);
	relayDown = true;
	pendulum.forkFrom = 102; // the source moves to another chain above its last ingested block
	burn(pendulum, 1n, 100n * PEN, 103);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/chain has 0x0+10+66 at Pendulum block 102, where the relay chain recorded 0x0+66;/,
	);
});

test("a kept relay record past the source's finalized head binds the source while no newer one arrives", async (t) => {
	let relayDown = false;
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
		head: () => ({ number: 103, hash: pendulumHash({}, 103) }),
		failing: () => relayDown,
	});
	burn(pendulum, 0n, 100n * PEN);
	pendulum.best = 103; // imported, not yet finalized by the source
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.deepEqual(persisted().relayAnchor, { number: 103, hash: pendulumHash({}, 103) });
	assert.equal(persisted().lastPendulumBlock, 101);
	relayDown = true;
	pendulum.forkFrom = 102; // the source finalizes another chain below the kept record...
	burn(pendulum, 1n, 999n * PEN, 102); // ...with a burn on it
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/chain has 0x0+66 at Pendulum block 102, where the source's finalized head is 0x0+10+66;/,
	);
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
});

test("a relay record past the source's finalized head must descend from that head", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
		head: () => ({ number: 102, hash: pendulumHash({}, 102) }),
	});
	burn(pendulum, 0n, 100n * PEN);
	pendulum.forkFrom = 101; // its finalized block 101 is on another chain...
	pendulum.canonicalBlockHashes = true; // ...though it names the relay chain's hash for block 102
	pendulum.best = 102;
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/records 0x0+66 as Pendulum block 102, whose chain has 0x0+65 at block 101, where the source has 0x0+10+65;/,
	);
	assert.equal(persisted().lastPendulumBlock, 100);
});

test("a relay record past the source's finalized head that its best chain does not hold yet holds the source back", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
		head: () => ({ number: 102, hash: pendulumHash({}, 102) }),
	});
	burn(pendulum, 0n, 100n * PEN);
	pendulum.forkFrom = 102; // its best chain holds another block 102 for now
	pendulum.best = 102;
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => [page.subject, page.severity]), [["RELAY ANCHOR UNAVAILABLE", "WARNING"]]);
	assert.match(
		pages[0].detail,
		/^relay RPC 1: the source has not imported block 102, the relay chain's Pendulum head, yet; no Pendulum block after 100 is ingested until a relay RPC anchors a newer head,/,
	);
	assert.equal(persisted().relayAnchor, undefined);
	assert.equal(persisted().lastPendulumBlock, 100);
});

test("a block past the relay chain's record waits for it, and a fork there is caught once it is recorded", async (t) => {
	let recorded = 101;
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
		head: () => ({ number: recorded, hash: pendulumHash({}, recorded) }),
	});
	burn(pendulum, 0n, 100n * PEN);
	pendulum.forkFrom = 102; // the source finalizes another block 102...
	burn(pendulum, 1n, 999n * PEN, 102); // ...with a fabricated burn, past the relay chain's record
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
	recorded = 102; // the relay chain records the real block 102
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/relay chain records 0x0+66 as Pendulum block 102, the source's chain has 0x0+10+66;/,
	);
	assert.equal(persisted().lastPendulumBlock, 101);
});

test("a source that finalizes below a relay record it does not hold ingests nothing past the kept record", async (t) => {
	let recorded = 101;
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
		head: () => ({ number: recorded, hash: pendulumHash({}, recorded) }),
	});
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api); // block 101 anchored
	pendulum.forkFrom = 102; // the source serves another chain from block 102 on...
	for (let block = 102; block <= 105; block++) burn(pendulum, BigInt(block - 101), 999n * PEN, block); // ...with fabricated burns
	pendulum.best = 106; // ...and holds another block 106 than the relay chain records
	recorded = 106;
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => page.subject), ["RELAY ANCHOR UNAVAILABLE"]);
	assert.match(pages[0].detail, /has not imported block 106, .*; no Pendulum block after 101 is ingested until a relay RPC anchors/);
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
	assert.deepEqual(persisted().relayAnchor, { number: 101, hash: pendulumHash({}, 101) });
});

test("a Pendulum view the relay chain holds back pages as stalled", async (t) => {
	const { pendulum, api, monitor, pages } = await relaySetup(t, { head: () => ({ number: 101, hash: pendulumHash({}, 101) }) });
	burn(pendulum, 0n, 100n * PEN);
	pendulum.finalized = 130; // 29 blocks of 12 s past the relay chain's record
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "PENDULUM SOURCE VIEW STALLED")?.detail ?? "",
		/^verified Pendulum block 101 is 34\ds old \(the source finalized 130, not confirmed yet\);/,
	);
});

test("a Base event whose burn the relay chain has not confirmed yet is not proven fabricated by a newer source head", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await relaySetup(t, {
		head: () => ({ number: 101, hash: pendulumHash({}, 101) }),
		env: { SOURCE_CLOCK_SKEW_MARGIN_SECONDS: "0" },
	});
	burn(pendulum, 0n, 100n * PEN);
	burn(pendulum, 1n, 100n * PEN, 125);
	pendulum.finalized = 130;
	release(base, 1n, 100n * PEN);
	await monitor.poll(api);
	await new Promise((resolve) => setTimeout(resolve, 5));
	await monitor.poll(api); // the source's own head has passed the first sighting, the confirmed view has not
	assert.ok(subjects().includes("UNVERIFIED BASE EVENT"), JSON.stringify(subjects()));
	assert.ok(!subjects().includes("MIGRATION TUPLE VIOLATION"), JSON.stringify(subjects()));
});

test("a relay chain that records another hash at a height it anchored pages critical", async (t) => {
	let recorded = pendulumHash({}, 101);
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
		head: () => ({ number: 101, hash: recorded }),
	});
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(persisted().relayAnchor, { number: 101, hash: pendulumHash({}, 101) });
	recorded = pendulumHash({ forkFrom: 0 }, 101);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
		/relay chain now records 0x0+10+65 as Pendulum block 101, where it recorded 0x0+65;/,
	);
});

test("a relay record below the block ingested last is checked down the source's chain", async (t) => {
	for (const recorded of [pendulumHash({}, 100), pendulumHash({ forkFrom: 0 }, 100)]) {
		let relayDown = true;
		const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, {
			head: () => ({ number: 100, hash: recorded }),
			failing: () => relayDown,
		});
		burn(pendulum, 0n, 100n * PEN);
		await monitor.poll(api); // ingested up to block 101 without the anchor
		relayDown = false;
		burn(pendulum, 1n, 100n * PEN, 102);
		await monitor.poll(api);
		if (recorded === pendulumHash({}, 100)) {
			assert.deepEqual(pages.map((page) => page.subject), ["RELAY ANCHOR UNAVAILABLE"]);
			assert.deepEqual(persisted().relayAnchor, { number: 100, hash: recorded });
			assert.equal(persisted().lastPendulumBlock, 101); // block 102 waits for a newer record
		} else {
			assert.match(
				pages.find((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED")?.detail ?? "",
				/relay chain records 0x0+10+64 as Pendulum block 100, the source's chain has 0x0+64;/,
			);
			assert.equal(persisted().lastPendulumBlock, 101);
		}
	}
});

test("an unreadable relay chain warns, the source is used, and it turns critical after the grace", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, { failing: () => true });
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => [page.subject, page.severity]), [["RELAY ANCHOR UNAVAILABLE", "WARNING"]]);
	assert.match(pages[0].detail, /relay RPC 1: Error: relay RPC unavailable;/);
	assert.equal(persisted().lastPendulumBlock, 101);
	const late = await relaySetup(t, { failing: () => true, env: { RELAY_ANCHOR_GRACE_SECONDS: "0" } });
	burn(late.pendulum, 0n, 100n * PEN);
	await new Promise((resolve) => setTimeout(resolve, 5));
	await late.monitor.poll(late.api);
	assert.ok(late.subjects().includes("CRITICAL: RELAY ANCHOR STALE"), JSON.stringify(late.subjects()));
});

test("a source behind the relay chain is not anchored, its blocks wait and it pages, critical after the grace", async (t) => {
	const ahead = (pendulum: Pendulum) => ({ number: pendulum.finalized + 1, hash: pendulumHash(pendulum, pendulum.finalized + 1) });
	const { pendulum, api, monitor, pages, persisted } = await relaySetup(t, { head: ahead });
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => page.subject), ["RELAY ANCHOR UNAVAILABLE"]);
	assert.equal(persisted().relayAnchor, undefined);
	assert.equal(persisted().lastPendulumBlock, 100);
	const late = await relaySetup(t, { head: ahead, env: { RELAY_ANCHOR_GRACE_SECONDS: "0" } });
	await new Promise((resolve) => setTimeout(resolve, 5));
	await late.monitor.poll(late.api);
	assert.match(
		late.pages.find((page) => page.subject === "CRITICAL: RELAY ANCHOR STALE")?.detail ?? "",
		/the source has not imported block 101, the relay chain's Pendulum head, yet/,
	);
});

test("the relay RPCs are tried in order", async (t) => {
	let chain: Pendulum | undefined;
	const relay = relayChain(() => ({ number: chain!.finalized, hash: pendulumHash({}, chain!.finalized) }));
	const { pendulum, api, monitor, pages, persisted } = await setup(t, {
		env: { POLKADOT_WS: "wss://down.test, wss://up.test" },
		prepare: (_base, pendulum) => (chain = pendulum),
		connect: async (url) => (url === "wss://up.test" ? relay : assert.fail(`cannot reach ${url}`)),
	});
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.equal(persisted().relayAnchor?.number, 101);
});

/** A monitor with two relay RPCs; each records `head` or, without one, is down. */
async function twoRelaySetup(t: { after(fn: () => void): void }, heads: Array<((pendulum: Pendulum) => Head) | undefined>) {
	let chain: Pendulum | undefined;
	const urls = ["wss://first.test", "wss://second.test"];
	const relays = heads.map((head) => relayChain(() => head!(chain!), () => !head));
	return setup(t, {
		env: { POLKADOT_WS: urls.join(",") },
		prepare: (_base, pendulum) => (chain = pendulum),
		connect: async (url) => relays[urls.indexOf(url)] ?? assert.fail(url),
	});
}
const honestHead = (pendulum: Pendulum) => ({ number: pendulum.finalized, hash: pendulumHash({}, pendulum.finalized) });
const forkedHead = (pendulum: Pendulum) => ({ number: pendulum.finalized, hash: pendulumHash({ forkFrom: 0 }, pendulum.finalized) });

test("a relay contradiction the next relay RPC outvotes warns, and that RPC's record anchors the source", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await twoRelaySetup(t, [forkedHead, honestHead]);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => [page.subject, page.severity]), [["RELAY RPCS DISAGREE", "WARNING"]]);
	assert.match(
		pages[0].detail,
		/^relay RPC 1 reports that the relay chain records 0x0+10+65 as Pendulum block 101, .*; relay RPC 2 records 0x0+65 as Pendulum block 101, on the source's chain, .* check both$/,
	);
	assert.deepEqual(persisted().relayAnchor, { number: 101, hash: pendulumHash({}, 101) });
	assert.equal(persisted().lastPendulumBlock, 101);
});

test("a relay record below a contradicting one does not outvote it", async (t) => {
	const below = (pendulum: Pendulum) => ({ number: pendulum.finalized - 1, hash: pendulumHash({}, pendulum.finalized - 1) });
	const { pendulum, api, monitor, pages, persisted } = await twoRelaySetup(t, [forkedHead, below]);
	burn(pendulum, 0n, 100n * PEN, 102);
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => page.subject), ["RELAY ANCHOR UNAVAILABLE"]);
	assert.match(pages[0].detail, /no other relay RPC confirms or outvotes it \(relay RPC 2: its Pendulum head 101 lies below the contradicting record\);/);
	assert.equal(persisted().relayAnchor, undefined);
	assert.equal(persisted().lastPendulumBlock, 100);
});

/** A relay RPC whose record stays at the source's block 101. */
const stuckHead = () => ({ number: 101, hash: pendulumHash({}, 101) });

test("a relay contradiction that only a stuck relay RPC answers stays unconfirmed", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await twoRelaySetup(t, [honestHead, stuckHead]);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api); // relay RPC 1 anchors block 101
	pendulum.forkFrom = 102; // the source moves to another chain...
	burn(pendulum, 1n, 999n * PEN, 102); // ...with a fabricated burn, which relay RPC 1 contradicts
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => page.subject), ["RELAY ANCHOR UNAVAILABLE"]);
	assert.match(
		pages[0].detail,
		/^relay RPC 1 reports that the relay chain records 0x0+66 as Pendulum block 102, the source's chain has 0x0+10+66, and no other relay RPC confirms or outvotes it \(relay RPC 2: the relay chain's Pendulum head 101 is not newer\);/,
	);
	assert.deepEqual(persisted().relayAnchor, { number: 101, hash: pendulumHash({}, 101) });
	assert.equal(persisted().lastPendulumBlock, 101);
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
});

test("a relay RPC whose record does not anchor the source leaves it to the next one", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await twoRelaySetup(t, [stuckHead, honestHead]);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api); // relay RPC 1 anchors block 101
	burn(pendulum, 1n, 100n * PEN, 102);
	await monitor.poll(api); // relay RPC 1 stays there
	assert.equal(pages.length, 0, JSON.stringify(pages));
	assert.deepEqual(persisted().relayAnchor, { number: 102, hash: pendulumHash({}, 102) });
	assert.equal(persisted().lastPendulumBlock, 102);
});

test("a relay contradiction the next relay RPC confirms pages critical", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await twoRelaySetup(t, [forkedHead, forkedHead]);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.ok(pages.some((page) => page.subject === "CRITICAL: PENDULUM SOURCE CONTRADICTED"), JSON.stringify(pages));
	assert.equal(persisted().lastPendulumBlock, 100);
});

test("a relay contradiction no other relay RPC can confirm warns, and the source's blocks wait", async (t) => {
	const { pendulum, api, monitor, pages, persisted } = await twoRelaySetup(t, [forkedHead, undefined]);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	assert.deepEqual(pages.map((page) => page.subject), ["RELAY ANCHOR UNAVAILABLE"]);
	assert.match(pages[0].detail, /and no other relay RPC confirms or outvotes it \(relay RPC 2: Error: relay RPC unavailable\)/);
	assert.equal(persisted().lastPendulumBlock, 100);
});

test("while Pendulum cannot be read, a Base event with an unknown nonce pauses after the grace", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t, { env: { UNMATCHED_EVENT_GRACE_SECONDS: "0" } });
	pendulum.failure = new Error("node unreachable");
	release(base, 7n, 5_000n * PEN); // no burn behind it
	await monitor.poll(api);
	assert.ok(subjects().includes("UNVERIFIED BASE EVENT"), JSON.stringify(subjects()));
	assert.ok(!subjects().includes("MIGRATION TUPLE VIOLATION"), JSON.stringify(subjects()));
	await new Promise((resolve) => setTimeout(resolve, 5));
	await monitor.poll(api); // the grace has run out
	assert.ok(subjects().includes("MIGRATION TUPLE VIOLATION"), JSON.stringify(subjects()));
});

test("while Pendulum cannot be read, a mismatching release of a known nonce pauses at once", async (t) => {
	const { base, pendulum, api, monitor, pages } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	pendulum.failure = new Error("node unreachable");
	const tokenAmount = 100n * PEN * CF;
	const released = { nonce: 0n, recipient: TOKEN, palletAmount: 100n * PEN, tokenAmount };
	base.logs.push({ address: VAULT, eventName: "Released", args: released, blockNumber: 1_012n });
	base.totalReleased += tokenAmount;
	base.vaultBalance -= tokenAmount;
	base.safe = 1_020n;
	await monitor.poll(api);
	assert.match(pages.find((page) => page.subject === "MIGRATION TUPLE VIOLATION")?.detail ?? "", /recipient/);
	assert.ok(pages.some((page) => page.subject === "AUTO-PAUSE UNAVAILABLE"), JSON.stringify(pages)); // the pause was asked for
});

test("a release the source never shows pauses after the grace, with no approval in view", async (t) => {
	const { base, pendulum, api, monitor, pages } = await setup(t, { env: { UNMATCHED_EVENT_GRACE_SECONDS: "0" } });
	pendulum.failure = new Error("node unreachable");
	const tokenAmount = 5n * PEN * CF;
	base.logs.push({ address: VAULT, eventName: "Released", args: { nonce: 7n, recipient: RECIPIENT, palletAmount: 5n * PEN, tokenAmount }, blockNumber: 1_005n });
	base.totalReleased += tokenAmount;
	base.vaultBalance -= tokenAmount;
	await monitor.poll(api);
	await new Promise((resolve) => setTimeout(resolve, 5));
	await monitor.poll(api); // the grace has run out
	assert.match(pages.find((page) => page.subject === "MIGRATION TUPLE VIOLATION")?.detail ?? "", /^Released at Base block 1005: nonce 7 has no finalized/);
});

test("a second release of one nonce is a violation", async (t) => {
	const { base, pendulum, api, monitor, pages } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN, 1_004n);
	release(base, 0n, 100n * PEN, 1_006n);
	await monitor.poll(api);
	assert.match(pages.find((page) => page.subject === "MIGRATION TUPLE VIOLATION")?.detail ?? "", /duplicate Released event for nonce 0/);
});

test("vault events past the confirmed Base block do not trip the balance check", async (t) => {
	const { base, pendulum, api, monitor, pages } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN, 1_015n); // after the safe block 1010
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
});

test("logs that miss a release hold the Base cursor and page until a read accounts for the totals", async (t) => {
	const { base, pendulum, api, monitor, pages, persisted } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN, 1_005n);
	base.logsHead = 1_003n; // logs from a backend that has not reached the release
	await monitor.poll(api);
	await until(() => pages.some((page) => page.subject === "BASE LOG SCAN INCOMPLETE"), "the incomplete scan page");
	assert.match(
		pages.find((page) => page.subject === "BASE LOG SCAN INCOMPLETE")!.detail,
		/in Base blocks 1000-1010 the Released events add up to 0 .* totalReleased grew by 100000000000000000000/,
	);
	assert.equal(persisted().baseFromBlock, "1000");
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0"]);
	base.logsHead = undefined;
	await monitor.poll(api);
	assert.equal(persisted().baseFromBlock, "1011");
	assert.deepEqual(persisted().migrations, []);
});

test("logs that miss a sweep hold the Base cursor too", async (t) => {
	const { base, api, monitor, pages, persisted } = await setup(t);
	base.logs.push({ address: VAULT, eventName: "RemainderSwept", args: { to: TOKEN, amount: 5n }, blockNumber: 1_005n });
	base.totalSwept += 5n;
	base.vaultBalance -= 5n;
	base.logsHead = 1_003n;
	await monitor.poll(api);
	await until(() => pages.some((page) => page.subject === "BASE LOG SCAN INCOMPLETE"), "the incomplete scan page");
	assert.match(pages.find((page) => page.subject === "BASE LOG SCAN INCOMPLETE")!.detail, /RemainderSwept events to 0, .* totalSwept by 5/);
	assert.equal(persisted().baseFromBlock, "1000");
	base.logsHead = undefined;
	await monitor.poll(api);
	assert.equal(persisted().baseFromBlock, "1011");
});

test("a Base log range held past the grace pages critical", async (t) => {
	const { base, pendulum, api, monitor, pages } = await setup(t, { env: { UNMATCHED_EVENT_GRACE_SECONDS: "0" } });
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN, 1_005n);
	base.logsHead = 1_003n;
	await monitor.poll(api);
	await new Promise((resolve) => setTimeout(resolve, 5));
	await monitor.poll(api);
	const held = () => pages.filter((page) => page.subject.endsWith("BASE LOG SCAN INCOMPLETE"));
	await until(() => held().length === 2, "two incomplete scan pages");
	assert.deepEqual(held().map((page) => [page.subject, page.severity]), [
		["BASE LOG SCAN INCOMPLETE", "WARNING"],
		["CRITICAL: BASE LOG SCAN INCOMPLETE", "CRITICAL"],
	]);
	assert.match(held()[1].detail, /stays at block 1000 \(for \d+s\): no later Base event is matched against its burn/);
	base.logsHead = undefined;
	await monitor.poll(api); // the range reads in full
	burn(pendulum, 1n, 100n * PEN, 102);
	release(base, 1n, 100n * PEN, 1_015n);
	base.logsHead = 1_012n;
	base.safe = 1_020n;
	await monitor.poll(api); // a new hold starts as a warning
	await until(() => held().length === 3, "the next incomplete scan page");
	assert.equal(held()[2].subject, "BASE LOG SCAN INCOMPLETE");
});

test("the conservation check still runs while a Base log range is held", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN, 1_005n);
	release(base, 0n, 100n * PEN, 1_007n); // a second release lifts totalReleased past TotalMigrated...
	base.logsHead = 1_006n; // ...while the log backend drops its block
	await monitor.poll(api);
	assert.ok(subjects().includes("CONSERVATION VIOLATION"), JSON.stringify(subjects()));
	assert.ok(subjects().includes("AUTO-PAUSE UNAVAILABLE"), JSON.stringify(subjects())); // the pause was asked for
});

test("the conservation check waits while a Base event's burn is not in the source view yet", async (t) => {
	const { base, api, monitor, subjects } = await setup(t);
	release(base, 0n, 100n * PEN); // the burn is not in the lagging source view yet
	await monitor.poll(api);
	assert.deepEqual([...new Set(subjects())], ["UNVERIFIED BASE EVENT"]);
});

test("logs that miss an event once and then account for the totals hold nothing and page nothing", async (t) => {
	const { base, pendulum, api, monitor, subjects, persisted } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN, 1_005n);
	const released = toEventSelector("Released(uint64,address,uint256,uint256)"); // only reconciliation asks for it
	let reads = 0;
	base.logsHeadFor = (topics) => (topics.includes(released) && reads++ === 0 ? 1_003n : undefined);
	await monitor.poll(api);
	assert.equal(reads, 2); // read once more
	assert.equal(persisted().baseFromBlock, "1011");
	assert.deepEqual(persisted().migrations, []);
	await new Promise((resolve) => setTimeout(resolve, 50)); // an incomplete scan page is sent without waiting
	assert.deepEqual(subjects(), []);
});

test("a vault total that cannot be read where the vault has code fails the poll, not counting as zero", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t, { prepare: (base) => (base.deployedAt = 900n) });
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN, 950n); // before the first Base block the monitor scans
	const totalReleased = toFunctionSelector("totalReleased()");
	base.failing = (method, params) =>
		method === "eth_call" && (params[0] as { data: string }).data === totalReleased && params[1] === toHex(999n);
	await monitor.poll(api);
	await new Promise((resolve) => setTimeout(resolve, 50)); // an incomplete scan page is sent without waiting
	assert.ok(subjects().includes("monitor check failed"), JSON.stringify(subjects()));
	assert.ok(!subjects().includes("BASE LOG SCAN INCOMPLETE"), JSON.stringify(subjects()));
});

test("releases and sweeps in the first and the last block of a range account for the totals", async (t) => {
	const { base, pendulum, api, monitor, subjects, persisted } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	burn(pendulum, 1n, 100n * PEN);
	release(base, 0n, 100n * PEN, 1_000n);
	release(base, 1n, 100n * PEN, 1_010n);
	for (const blockNumber of [1_000n, 1_010n]) {
		base.logs.push({ address: VAULT, eventName: "RemainderSwept", args: { to: TOKEN, amount: 5n }, blockNumber });
		base.totalSwept += 5n;
		base.vaultBalance -= 5n;
	}
	await monitor.poll(api);
	assert.equal(persisted().baseFromBlock, "1011");
	assert.deepEqual(persisted().migrations, []);
	await new Promise((resolve) => setTimeout(resolve, 50)); // an incomplete scan page is sent without waiting
	assert.deepEqual([...new Set(subjects())], ["VAULT ADMIN EVENT"]); // the sweeps
});

test("the totals count as zero before the vault existed", async (t) => {
	const { base, pendulum, api, monitor, subjects, persisted } = await setup(t, { prepare: (base) => (base.deployedAt = 1_000n) });
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN, 1_005n);
	await monitor.poll(api);
	assert.deepEqual(subjects(), []);
	assert.equal(persisted().baseFromBlock, "1011");
});

/** An approval of `nonce` for `recipient` from attestor `attestor`. */
function approve(base: Base, nonce: bigint, recipient: string, attestor: string, blockNumber: bigint): void {
	base.logs.push({ address: VAULT, eventName: "Approved", args: { nonce, recipient, palletAmount: 100n * PEN, attestor }, blockNumber });
}

test("the release of a nonce forgets the approvals of it that disagreed", async (t) => {
	const { base, pendulum, api, monitor, persisted } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	approve(base, 0n, TOKEN, ATTESTORS[0], 1_004n);
	await monitor.poll(api);
	assert.equal(persisted().mismatchedApprovals.length, 1);
	release(base, 0n, 100n * PEN, 1_012n);
	base.safe = 1_020n;
	await monitor.poll(api);
	assert.deepEqual(persisted().mismatchedApprovals, []);
});

test("one attestor approving against the burn pages critical; a second one pauses, also polls later", async (t) => {
	const { base, pendulum, api, monitor, restart, pages, subjects } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	approve(base, 0n, TOKEN, ATTESTORS[0], 1_012n);
	base.safe = 1_020n;
	await monitor.poll(api);
	const page = pages.find((page) => page.subject === "CRITICAL: MISMATCHED APPROVAL");
	assert.equal(page?.severity, "CRITICAL", JSON.stringify(pages));
	assert.match(page.detail, new RegExp(`by attestor ${ATTESTORS[0]}: nonce 0 recipient ${TOKEN}`, "i"));
	assert.ok(!subjects().includes("MIGRATION TUPLE VIOLATION"), JSON.stringify(subjects()));
	approve(base, 0n, RECIPIENT, ATTESTORS[1], 1_022n); // agrees with the burn
	approve(base, 0n, "0x5555555555555555555555555555555555555555", ATTESTORS[0], 1_023n); // the same attestor again
	base.safe = 1_030n;
	await monitor.poll(api);
	assert.ok(!subjects().includes("MIGRATION TUPLE VIOLATION"), JSON.stringify(subjects()));
	const restarted = await restart();
	approve(base, 0n, TOKEN, ATTESTORS[2], 1_032n);
	base.safe = 1_040n;
	await restarted.poll(api);
	assert.match(
		pages.find((page) => page.subject === "MIGRATION TUPLE VIOLATION")?.detail ?? "",
		new RegExp(`attestors ${ATTESTORS[0]}, ${ATTESTORS[2]} approved tuples for nonce 0`, "i"),
	);
});

test("at a threshold of two, one attestor approving against the burn pauses", async (t) => {
	const { base, pendulum, api, monitor, pages, subjects } = await setup(t, { prepare: (base) => (base.threshold = 2n) });
	burn(pendulum, 0n, 100n * PEN);
	approve(base, 0n, TOKEN, ATTESTORS[0], 1_005n);
	await monitor.poll(api);
	assert.match(
		pages.find((page) => page.subject === "MIGRATION TUPLE VIOLATION")?.detail ?? "",
		new RegExp(`; attestor ${ATTESTORS[0]} approved tuples for nonce 0`, "i"),
	);
	assert.ok(!subjects().includes("CRITICAL: MISMATCHED APPROVAL"), JSON.stringify(subjects()));
});

test("auto-pause readiness is checked at startup and each problem pages", async (t) => {
	const { startup } = await setup(t, {
		env: { GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY },
		prepare: (base) => {
			base.chainId = 84532;
			base.guardian = "0x5555555555555555555555555555555555555555";
			base.balances.clear();
		},
	});
	const detail = startup.find((page) => page.subject === "AUTO-PAUSE NOT READY")?.detail ?? "";
	assert.match(detail, /serves chain 84532, not BASE_CHAIN_ID 8453/);
	assert.match(detail, new RegExp(`${GUARDIAN} is not the vault guardian 0x5{40}`));
	assert.match(detail, /holds 0 wei/);
});

test("the guardian balance floor rejects a negative value and keeps its default when empty", async (t) => {
	await assert.rejects(setup(t, { env: { GUARDIAN_MIN_BALANCE_WEI: "-1" } }), /invalid GUARDIAN_MIN_BALANCE_WEI: -1/);
	const { startup } = await setup(t, {
		env: { GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY, GUARDIAN_MIN_BALANCE_WEI: "" },
		prepare: (base) => base.balances.clear(),
	});
	const detail = startup.find((page) => page.subject === "AUTO-PAUSE NOT READY")?.detail ?? "";
	assert.match(detail, /holds 0 wei, less than GUARDIAN_MIN_BALANCE_WEI 1000000000000000/);
});

test("auto-pause readiness is re-checked every poll", async (t) => {
	const { base, api, monitor, startup, subjects } = await setup(t, { env: { GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY } });
	assert.ok(!startup.some((page) => page.subject.startsWith("AUTO-PAUSE")), JSON.stringify(startup));
	base.guardian = "0x5555555555555555555555555555555555555555";
	await monitor.poll(api);
	assert.deepEqual(subjects(), ["AUTO-PAUSE NOT READY"]);
});

test("a failed readiness read pages and the vault balance check still runs", async (t) => {
	const { base, api, monitor, pages, subjects } = await setup(t, { env: { GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY } });
	base.failing = (method) => method === "eth_getBalance";
	base.vaultBalance -= 1n;
	base.paused = true; // so the auto-pause only confirms
	await monitor.poll(api);
	assert.deepEqual(sorted(subjects()), ["AUTO-PAUSE NOT READY", "VAULT BALANCE DEFICIT", "vault pause confirmed"]);
	assert.match(pages.find((page) => page.subject === "AUTO-PAUSE NOT READY")?.detail ?? "", /readiness reads failed.*rate limited/s);
});

test("vault admin events page once each from the Base scan", async (t) => {
	const { base, api, monitor, pages } = await setup(t);
	const guardian = "0x5555555555555555555555555555555555555555";
	base.logs.push({ address: VAULT, eventName: "GuardianUpdated", args: { guardian }, blockNumber: 1_004n });
	base.logs.push({ address: VAULT, eventName: "CapsUpdated", args: { perReleaseCap: 5n, dailyCap: 7n }, blockNumber: 1_012n });
	await monitor.poll(api);
	base.safe = 1_020n;
	await monitor.poll(api);
	const adminPages = pages.filter((page) => page.subject === "VAULT ADMIN EVENT").map((page) => page.detail);
	assert.equal(adminPages.length, 2, JSON.stringify(pages));
	assert.match(adminPages[0], /^GuardianUpdated\(guardian=0x5{40}\) at Base block 1004/i);
	assert.match(adminPages[1], /^CapsUpdated\(perReleaseCap=5, dailyCap=7\) at Base block 1012/);
});

test("a failed admin scan pages, every other check still runs and the scan resumes where it stopped", async (t) => {
	const { base, pendulum, api, monitor, restart, subjects, persisted } = await setup(t, {
		env: { GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY },
	});
	burn(pendulum, 0n, 100n * PEN);
	release(base, 0n, 100n * PEN);
	base.logs.push({ address: VAULT, eventName: "GuardianUpdated", args: { guardian: GUARDIAN }, blockNumber: 1_004n });
	const guardianUpdated = toEventSelector("GuardianUpdated(address)"); // only the admin scan asks for it
	base.failing = (method, params) => method === "eth_getLogs" && JSON.stringify(params).includes(guardianUpdated);
	base.vaultBalance -= 1n;
	base.paused = true; // so the auto-pause only confirms and the poll persists its state
	await monitor.poll(api);
	assert.deepEqual(sorted(subjects()), ["BASE ADMIN SCAN FAILED", "VAULT BALANCE DEFICIT", "vault pause confirmed"]);
	assert.equal(persisted().baseFromBlock, "1011"); // reconciled although the admin scan failed
	assert.equal(persisted().baseAdminFromBlock, "1000");
	base.failing = undefined;
	base.vaultBalance += 1n;
	await (await restart()).poll(api);
	assert.ok(subjects().includes("VAULT ADMIN EVENT"), JSON.stringify(subjects()));
	assert.equal(persisted().baseAdminFromBlock, "1011");
});

test("an admin scan that keeps failing past the grace pages critical", async (t) => {
	const { base, api, monitor, pages } = await setup(t, { env: { UNMATCHED_EVENT_GRACE_SECONDS: "0" } });
	const guardianUpdated = toEventSelector("GuardianUpdated(address)"); // only the admin scan asks for it
	const failing = (method: string, params: unknown[]) => method === "eth_getLogs" && JSON.stringify(params).includes(guardianUpdated);
	base.failing = failing;
	await monitor.poll(api);
	await new Promise((resolve) => setTimeout(resolve, 5));
	await monitor.poll(api);
	const failed = () => pages.filter((page) => page.subject.endsWith("BASE ADMIN SCAN FAILED"));
	assert.deepEqual(failed().map((page) => [page.subject, page.severity]), [
		["BASE ADMIN SCAN FAILED", "WARNING"],
		["CRITICAL: BASE ADMIN SCAN FAILED", "CRITICAL"],
	]);
	assert.match(failed()[1].detail, /\(failing for \d+s\)/);
	base.failing = undefined;
	await monitor.poll(api); // the scan reads again
	base.failing = failing;
	base.safe = 1_020n; // a new range for the admin scan
	await monitor.poll(api);
	assert.equal(failed()[2]?.subject, "BASE ADMIN SCAN FAILED"); // a new failure starts as a warning
});

test("a monitor without a guardian key says so at startup", async (t) => {
	const { startup } = await setup(t);
	assert.ok(startup.some((page) => page.subject === "AUTO-PAUSE DISABLED"), JSON.stringify(startup));
});

test("a Base view that stops advancing pages", async (t) => {
	const { base, api, monitor, pages } = await setup(t);
	base.safeAgeSeconds = 601;
	await monitor.poll(api);
	assert.match(pages.find((page) => page.subject === "BASE VIEW STALLED")?.detail ?? "", /^safe block 1010 is 60\ds old/);
});

const UNREADABLE = "CRITICAL: Pendulum governance item cannot be read";
const RISKY = "CRITICAL: Pendulum governance item mints, moves or unlocks balances, or changes the runtime";
const pageFor = (pages: Page[], subject: string, label: string) =>
	pages.find((page) => page.subject === subject && page.detail.startsWith(label));

test("an undecodable governance item pages as unreadable and hides nothing after it", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	pendulum.publicProps.push([5, inline("garbage")]);
	pendulum.referenda.set(9, inline("mint"));
	await monitor.poll(api);
	assert.ok(pageFor(pages, UNREADABLE, "public proposal #5"), JSON.stringify(pages));
	assert.ok(pageFor(pages, RISKY, "referendum #9"), JSON.stringify(pages));
});

test("an oversized governance call pages as unreadable without being fetched", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	pendulum.publicProps.push([5, lookup(`0x${"ab".repeat(32)}`, 3_900_000)]);
	pendulum.preimages.set(`0x${"ab".repeat(32)}:3900000`, "mint");
	await monitor.poll(api);
	assert.match(pageFor(pages, UNREADABLE, "public proposal #5")?.detail ?? "", /3900000 bytes/);
	assert.equal(pendulum.preimageReads, 0);
});

test("a governance call that cannot be read yet is re-checked and pages critical once readable", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	const preimage = `0x${"cd".repeat(32)}`;
	pendulum.referenda.set(9, lookup(preimage, 4));
	await monitor.poll(api);
	assert.match(pageFor(pages, UNREADABLE, "referendum #9")?.detail ?? "", /not noted yet/);
	pendulum.preimages.set(`${preimage}:4`, "mint");
	await monitor.poll(api);
	assert.ok(pageFor(pages, RISKY, "referendum #9"), JSON.stringify(pages));
});

test("a governance item whose read fails is retried and hides nothing after it", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	const preimage = `0x${"aa".repeat(32)}`;
	pendulum.publicProps.push([5, lookup(preimage, 4)]);
	pendulum.preimages.set(`${preimage}:4`, "mint");
	pendulum.referenda.set(9, inline("mint"));
	pendulum.preimageFailures = 1;
	await monitor.poll(api);
	assert.match(pageFor(pages, UNREADABLE, "public proposal #5")?.detail ?? "", /reading the call failed/);
	assert.ok(pageFor(pages, RISKY, "referendum #9"), JSON.stringify(pages));
	await monitor.poll(api);
	assert.ok(pageFor(pages, RISKY, "public proposal #5"), JSON.stringify(pages));
});

test("a legacy call reference is read through the noted preimage's length", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	const preimage = `0x${"ef".repeat(32)}`;
	pendulum.referenda.set(9, legacy(preimage));
	pendulum.notedLengths.set(preimage, 4);
	pendulum.preimages.set(`${preimage}:4`, "mint");
	await monitor.poll(api);
	assert.ok(pageFor(pages, RISKY, "referendum #9"), JSON.stringify(pages));
});

test("a legacy call reference to a preimage noted before the status migration is read too", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	const preimage = `0x${"fe".repeat(32)}`;
	pendulum.referenda.set(9, legacy(preimage));
	pendulum.oldNotedLengths.set(preimage, 4);
	pendulum.preimages.set(`${preimage}:4`, "mint");
	await monitor.poll(api);
	assert.ok(pageFor(pages, RISKY, "referendum #9"), JSON.stringify(pages));
});

test("council and technical-committee motions are decoded with the same risk rules", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	const [anchor, remark, huge] = ["0x01", "0x02", "0x03"].map((prefix) => prefix + "00".repeat(31));
	pendulum.motions.council.set(anchor, "setTreasuryDestination");
	pendulum.motions.technicalCommittee.set(remark, "remark");
	pendulum.motions.council.set(huge, "mint");
	pendulum.motionSizes.set(huge, 100_000);
	await monitor.poll(api);
	assert.ok(pageFor(pages, RISKY, `council motion ${anchor}`), JSON.stringify(pages));
	assert.ok(pageFor(pages, "PENDULUM GOVERNANCE ITEM", `technical committee motion ${remark}`), JSON.stringify(pages));
	assert.match(pageFor(pages, UNREADABLE, `council motion ${huge}`)?.detail ?? "", /100000 bytes/);
	assert.ok(!pendulum.motionReads.has(huge));
});

test("a motion or external proposal made again with the same call pages again", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	const motion = `0x04${"00".repeat(31)}`;
	pendulum.motions.council.set(motion, "remark");
	pendulum.motionIndices.set(motion, 3);
	pendulum.external = inline("remark");
	await monitor.poll(api);
	pendulum.motionIndices.set(motion, 4); // closed and proposed again between two polls
	pendulum.referenda.set(9, inline("remark")); // the external proposal tabled, and proposed again between two polls
	await monitor.poll(api);
	pendulum.external = undefined; // tabled or vetoed
	await monitor.poll(api);
	pendulum.external = inline("remark"); // proposed again
	await monitor.poll(api);
	assert.equal(pages.filter((page) => page.detail.startsWith(`council motion ${motion}`)).length, 2, JSON.stringify(pages));
	assert.equal(pages.filter((page) => page.detail.startsWith("the council's external proposal")).length, 3, JSON.stringify(pages));
});

const TREASURY_DESTINATION = "0x5555555555555555555555555555555555555555";

test("a treasury migration to any but the expected address pages and pauses", async (t) => {
	const { base, pendulum, api, monitor, pages, persisted } = await setup(t, {
		env: { EXPECTED_TREASURY_DESTINATION: TREASURY_DESTINATION, GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY },
	});
	base.paused = true; // so the auto-pause only confirms
	burn(pendulum, 0n, 100n * PEN, 101, { who: TREASURY_ACCOUNT, recipient: TREASURY_DESTINATION });
	burn(pendulum, 1n, 100n * PEN, 101, { recipient: TOKEN }); // anyone else may migrate anywhere
	await monitor.poll(api);
	assert.equal(pages.length, 0, JSON.stringify(pages));
	burn(pendulum, 2n, 7n * PEN, 102, { who: TREASURY_ACCOUNT, recipient: TOKEN });
	await monitor.poll(api);
	const page = pages.find((page) => page.subject === "TREASURY DESTINATION VIOLATION");
	assert.equal(page?.severity, "CRITICAL", JSON.stringify(pages));
	assert.match(page.detail, new RegExp(`nonce 2 in finalized Pendulum block 102 pays 7000000000000 pallet units to ${TOKEN}`, "i"));
	assert.ok(pages.some((page) => page.subject === "vault pause confirmed"), JSON.stringify(pages));
	assert.deepEqual(persisted().migrations.map((migration) => migration.nonce), ["0", "1", "2"]);
});

test("while the pause after a treasury violation is not confirmed, the monitor holds at its block", async (t) => {
	const { pendulum, api, monitor, pages, subjects, persisted } = await setup(t, {
		env: { EXPECTED_TREASURY_DESTINATION: TREASURY_DESTINATION },
	});
	burn(pendulum, 0n, 100n * PEN, 101, { who: TREASURY_ACCOUNT, recipient: TOKEN });
	await monitor.poll(api);
	await monitor.poll(api);
	const violations = pages.filter((page) => page.subject === "TREASURY DESTINATION VIOLATION");
	assert.equal(violations.length, 2);
	assert.match(violations[1].detail, /; the monitor holds at that block until the vault is paused$/);
	assert.equal(persisted().lastPendulumBlock, 100);
	assert.ok(!subjects().includes("PENDULUM SOURCE UNREADABLE"), JSON.stringify(subjects())); // the source reads fine
});

test("admin events of the migration pallet page from the ingest", async (t) => {
	const { pendulum, api, monitor, pages } = await setup(t);
	pendulum.finalized = 101;
	pendulum.events.set(101, [
		{ section: "tokenMigration", method: "TreasuryDestinationSet", data: ["0x5555555555555555555555555555555555555555"] },
		{ section: "tokenMigration", method: "MigrationPauseSet", data: [false] },
	]);
	await monitor.poll(api);
	const adminPages = pages.filter((page) => page.subject === "MIGRATION PALLET ADMIN EVENT").map((page) => page.detail);
	assert.equal(adminPages.length, 2, JSON.stringify(pages));
	assert.match(adminPages[0], /TreasuryDestinationSet\(0x5{40}\) in finalized Pendulum block 101/);
	assert.match(adminPages[1], /MigrationPauseSet\(false\)/);
});

test("the governance watch runs after the conservation checks", async (t) => {
	const { base, pendulum, api, monitor, subjects } = await setup(t);
	base.vaultBalance -= 1n;
	pendulum.publicProps.push([5, inline("remark")]);
	await monitor.poll(api);
	assert.deepEqual(subjects().slice(2), ["PENDULUM GOVERNANCE ITEM"]);
	assert.deepEqual(sorted(subjects().slice(0, 2)), ["AUTO-PAUSE UNAVAILABLE", "VAULT BALANCE DEFICIT"]);
});

test("stale migrations page one liveness summary per kind and poll", async (t) => {
	const { base, pendulum, api, monitor, pages } = await setup(t, { env: { GRACE_SECONDS: "30" } });
	pendulum.headAgeMs = 60_000;
	for (let nonce = 0n; nonce < 27n; nonce++) burn(pendulum, nonce, 100n * PEN);
	for (const nonce of [25n, 26n]) {
		const payload = keccak256(encodeAbiParameters(
			[{ type: "uint64" }, { type: "address" }, { type: "uint256" }],
			[nonce, RECIPIENT, 100n * PEN],
		));
		base.approvals.set(payload, 3n);
	}
	await monitor.poll(api);
	await monitor.poll(api);
	const liveness = pages.filter((page) => page.subject.startsWith("LIVENESS"));
	assert.equal(liveness.length, 2, JSON.stringify(liveness));
	assert.match(pageFor(liveness, "LIVENESS: migration below approval quorum", "25 ")!.detail, /nonce 0, 1, .*, 9 and 15 more\)/);
	assert.match(pageFor(liveness, "LIVENESS: quorum reached but not released", "2 ")!.detail, /\(nonce 25, 26\)/);
});

test("first sightings of unmatched events do not delay the pause", async (t) => {
	const events: string[] = [];
	let violationArrived = () => {};
	const arrived = new Promise<void>((resolve) => (violationArrived = resolve));
	const { base, pendulum, api, monitor } = await setup(t, {
		webhook: async ({ subject }) => {
			if (subject === "MIGRATION TUPLE VIOLATION") {
				events.push("violation paged");
				violationArrived();
			}
			if (subject === "UNVERIFIED BASE EVENT") {
				events.push("first sighting paged");
				await Promise.race([arrived, new Promise((resolve) => setTimeout(resolve, 3_000).unref())]);
				events.push("first sighting answered");
			}
			return 200;
		},
	});
	burn(pendulum, 0n, 100n * PEN);
	base.logs.push({
		address: VAULT,
		eventName: "Approved",
		args: { nonce: 5n, recipient: RECIPIENT, palletAmount: 100n * PEN, attestor: ATTESTORS[0] },
		blockNumber: 1_005n,
	});
	base.logs.push({
		address: VAULT,
		eventName: "Released",
		args: { nonce: 0n, recipient: RECIPIENT, palletAmount: 100n * PEN, tokenAmount: 1n },
		blockNumber: 1_006n,
	});
	base.totalReleased += 1n;
	base.vaultBalance -= 1n;
	await monitor.poll(api);
	await until(() => events.includes("first sighting answered"), "the first sighting's answer", 5_000);
	assert.deepEqual(events, ["first sighting paged", "violation paged", "first sighting answered"]);
});

test("after a violation the Base cursor stays on events still awaiting their source", async (t) => {
	const { base, pendulum, api, monitor, subjects, persisted } = await setup(t, {
		env: { UNMATCHED_EVENT_GRACE_SECONDS: "0", GUARDIAN_PRIVATE_KEY: GUARDIAN_KEY },
	});
	base.paused = true; // so the pause is confirmed and the poll persists its state
	release(base, 0n, 100n * PEN); // the burn is not in the lagging source view yet
	await monitor.poll(api);
	await new Promise((resolve) => setTimeout(resolve, 5));
	await monitor.poll(api); // the grace has run out: a cannot-verify violation
	assert.ok(subjects().includes("MIGRATION TUPLE VIOLATION"), JSON.stringify(subjects()));
	assert.equal(persisted().baseFromBlock, "1000");
	burn(pendulum, 0n, 100n * PEN); // the source view catches up
	await monitor.poll(api);
	assert.deepEqual(persisted().migrations, []);
	assert.equal(persisted().baseFromBlock, "1011");
});

test("the conservation check fires only beyond an acknowledged loss", async (t) => {
	const { base, api, monitor, subjects } = await setup(t, { env: { ACKNOWLEDGED_RELEASE_EXCESS: "1000" } });
	base.totalReleased = 1_000n; // released without a burn, quantified and accepted
	base.vaultBalance = SUPPLY - 1_000n;
	await monitor.poll(api);
	assert.deepEqual(subjects(), []);
	base.totalReleased += 1n;
	base.vaultBalance -= 1n;
	await monitor.poll(api);
	assert.ok(subjects().includes("CONSERVATION VIOLATION"), JSON.stringify(subjects()));
});

/** A timelock operation scheduling the vault's `pause()`, ready in 48 h. */
function schedule(base: Base, id: Hex, blockNumber = 1_005n): void {
	const call = { target: VAULT, value: 0n, data: "0x8456cb59", predecessor: toHex(0, { size: 32 }), delay: 172_800n };
	base.logs.push({ address: TIMELOCK, eventName: "CallScheduled", args: { id, index: 0n, ...call }, blockNumber });
	base.readyAt.set(id, BigInt(Math.floor(Date.now() / 1000) + 172_800));
}

test("a scheduled operation with an extreme delay pages and hides nothing after it", async (t) => {
	const { base, api, monitor, pages } = await setup(t, { env: { TIMELOCK_ADDRESS: TIMELOCK } });
	const [extreme, ordinary] = [toHex(1, { size: 32 }), toHex(2, { size: 32 })];
	schedule(base, extreme, 1_004n);
	base.readyAt.set(extreme, 10n ** 20n);
	schedule(base, ordinary, 1_006n);
	await monitor.poll(api);
	const scheduled = pages.filter((page) => page.subject === "GOVERNANCE OPERATION SCHEDULED").map((page) => page.detail);
	assert.equal(scheduled.length, 2, JSON.stringify(pages));
	assert.ok(scheduled.some((detail) => detail.startsWith(`operation ${ordinary}`)), JSON.stringify(scheduled));
	assert.ok(!pages.some((page) => page.subject === "BASE ADMIN SCAN FAILED"), JSON.stringify(pages));
});

test("an operation cancelled and scheduled again with the same id pages again", async (t) => {
	const { base, api, monitor, pages } = await setup(t, { env: { TIMELOCK_ADDRESS: TIMELOCK } });
	const id = toHex(7, { size: 32 });
	schedule(base, id, 1_005n);
	await monitor.poll(api);
	base.readyAt.set(id, 0n); // cancelled
	schedule(base, id, 1_015n);
	base.safe = 1_020n;
	await monitor.poll(api);
	const scheduled = pages.filter((page) => page.subject === "GOVERNANCE OPERATION SCHEDULED");
	assert.equal(scheduled.length, 2, JSON.stringify(pages));
});

test("once-only pages the webhook rejects are sent again on the next poll", async (t) => {
	const failedOnce = new Set<string>();
	const { base, pendulum, api, monitor, pages } = await setup(t, {
		env: { TIMELOCK_ADDRESS: TIMELOCK },
		webhook: ({ subject }) => {
			if (failedOnce.has(subject)) return 200;
			failedOnce.add(subject);
			return 500;
		},
	});
	schedule(base, toHex(1, { size: 32 }));
	pendulum.publicProps.push([7, inline("remark")]);
	await monitor.poll(api);
	await monitor.poll(api);
	const delivered = pages.filter((page) => page.status === 200).map((page) => page.subject);
	assert.ok(delivered.includes("GOVERNANCE OPERATION SCHEDULED"), `delivered: ${delivered}`);
	assert.ok(delivered.includes("PENDULUM GOVERNANCE ITEM"), `delivered: ${delivered}`);
	assert.ok(delivered.includes("AUTO-PAUSE DISABLED"), `delivered: ${delivered}`); // first sent at startup
});
