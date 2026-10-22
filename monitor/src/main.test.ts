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
		/** Requests to answer with an RPC error. */
		failing: undefined as ((method: string, params: unknown[]) => boolean) | undefined,
	};
}
type Base = ReturnType<typeof baseChain>;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function answer(base: Base, method: string, params: any[]): unknown {
	if (base.failing?.(method, params)) throw new Error("rate limited");
	switch (method) {
		case "eth_chainId":
			return toHex(base.chainId);
		case "eth_getCode":
			return "0x";
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
			const result = {
				totalReleased: base.totalReleased,
				totalSwept: base.totalSwept,
				conversionFactor: CF,
				token: TOKEN,
				paused: base.paused,
				threshold: base.threshold,
				guardian: base.guardian,
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
 *  pallet's totals and the governance queues. Block numbers double as hashes. */
function pendulumChain() {
	const chain = {
		finalized: 100,
		/** Age of the finalized head; earlier blocks are 12 s apart. */
		headAgeMs: 0,
		failure: undefined as Error | undefined,
		/** Holds every finalized-head read until it settles, as a slow node would. */
		gate: undefined as Promise<void> | undefined,
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
					await chain.gate;
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

/** A started monitor against fresh stand-ins, which `prepare` can change
 *  before startup. `webhook` answers each page with an HTTP status; `pages`
 *  lists them in arrival order from after startup, `startup` the ones before. */
async function setup(
	t: { after(fn: () => void): void },
	{
		env = {},
		webhook = () => 200,
		prepare = () => {},
	}: {
		env?: Record<string, string>;
		webhook?: (page: Page) => number | Promise<number>;
		prepare?: (base: Base, pendulum: Pendulum) => void;
	} = {},
) {
	const base = baseChain();
	const { chain: pendulum, api } = pendulumChain();
	prepare(base, pendulum);
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
	const launch = async () => {
		const monitor: typeof import("./main.js") = await import(`./main.js?instance=${++instances}`);
		await monitor.start(api);
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
		monitor,
		/** A new monitor process on the same state file and stand-ins. */
		restart: launch,
		subjects: () => pages.map((page) => page.subject),
		persisted: () => JSON.parse(readFileSync(stateFile, "utf8")) as {
			baseFromBlock: string;
			baseAdminFromBlock: string;
			migrations: Array<{ nonce: string }>;
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

test("while Pendulum cannot be read, a mismatch on a known nonce pauses at once", async (t) => {
	const { base, pendulum, api, monitor, pages } = await setup(t);
	burn(pendulum, 0n, 100n * PEN);
	await monitor.poll(api);
	pendulum.failure = new Error("node unreachable");
	const approval = { nonce: 0n, recipient: TOKEN, palletAmount: 100n * PEN, attestor: ATTESTORS[0] };
	base.logs.push({ address: VAULT, eventName: "Approved", args: approval, blockNumber: 1_012n });
	base.safe = 1_020n;
	await monitor.poll(api);
	assert.match(pages.find((page) => page.subject === "MIGRATION TUPLE VIOLATION")?.detail ?? "", /recipient/);
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
	await monitor.poll(api);
	while (!events.includes("first sighting answered")) await new Promise((resolve) => setTimeout(resolve, 10));
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
