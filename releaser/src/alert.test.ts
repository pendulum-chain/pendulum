import assert from "node:assert/strict";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { test } from "node:test";
import { inspect } from "node:util";
import { alertText, pingHeartbeat, postAlert } from "./alert.js";

/** A local webhook answering every POST with `status`, or never when null. */
async function webhook(status: number | null): Promise<{ url: string; server: Server }> {
	const server = createServer((request, response) => {
		request.resume();
		if (status !== null) request.on("end", () => response.writeHead(status).end());
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	return { url: `http://127.0.0.1:${(server.address() as AddressInfo).port}`, server };
}

test("an alert reads as a Slack message: severity, source and subject, then the detail", () => {
	assert.equal(
		alertText("critical", "pen-releaser 0xAb", "release blocked and needs operator action", "nonce=1 — ExceedsPerReleaseCap"),
		"[CRITICAL] pen-releaser 0xAb: release blocked and needs operator action\nnonce=1 — ExceedsPerReleaseCap",
	);
	assert.equal(alertText("warning", "s", "gas balance low", "1 wei"), "[WARNING] s: gas balance low\n1 wei");
	// Slack's control characters cannot mention a channel or form a link.
	assert.equal(alertText("warning", "s", "x", "<!channel> a & b <url>"), "[WARNING] s: x\n&lt;!channel&gt; a &amp; b &lt;url&gt;");
	const long = alertText("warning", "s", "x", "y".repeat(10_000));
	assert.equal(long.length, 3001);
	assert.ok(long.endsWith("y…"));
});

test("an accepted page is delivered", async () => {
	const { url, server } = await webhook(200);
	try {
		assert.equal(await postAlert(url, { subject: "s" }), true);
	} finally {
		server.close();
	}
});

test("a webhook error status, a timeout or a dead relay is a failed delivery, and logged", async (t) => {
	const logged = t.mock.method(console, "error", () => {});
	for (const status of [500, 429, 404]) {
		const { url, server } = await webhook(status);
		try {
			assert.equal(await postAlert(url, { subject: "s" }), false, `HTTP ${status}`);
		} finally {
			server.close();
		}
	}
	const hanging = await webhook(null);
	try {
		assert.equal(await postAlert(hanging.url, { subject: "s" }, 200), false, "timeout");
	} finally {
		hanging.server.closeAllConnections();
		hanging.server.close();
	}
	const closed = await webhook(200);
	closed.server.close();
	assert.equal(await postAlert(closed.url, { subject: "s" }), false, "connection refused");
	assert.equal(logged.mock.callCount(), 5);
});

test("a heartbeat requests its URL, and a failing endpoint is logged without the URL, never thrown", async (t) => {
	const logged = t.mock.method(console, "error", () => {});
	const seen: string[] = [];
	const ok = createServer((request, response) => {
		seen.push(`${request.method} ${request.url}`);
		response.writeHead(200).end("OK");
	});
	await new Promise<void>((resolve) => ok.listen(0, "127.0.0.1", resolve));
	try {
		await pingHeartbeat(`http://127.0.0.1:${(ok.address() as AddressInfo).port}/ping/secret-token?rid=1`);
	} finally {
		ok.close();
	}
	assert.deepEqual(seen, ["GET /ping/secret-token?rid=1"]);
	assert.equal(logged.mock.callCount(), 0);

	const failing = await webhook(500);
	await pingHeartbeat(`${failing.url}/secret-token`);
	failing.server.close();
	const hanging = await webhook(null);
	const settled = await Promise.race([
		pingHeartbeat(`${hanging.url}/secret-token`, 200).then(() => "gave up"),
		new Promise((resolve) => setTimeout(resolve, 2000, "still waiting").unref()),
	]);
	hanging.server.closeAllConnections();
	hanging.server.close();
	assert.equal(settled, "gave up", "a silent endpoint must not hold the request open");
	const closed = await webhook(200);
	closed.server.close();
	await pingHeartbeat(`${closed.url}/secret-token`);
	assert.equal(logged.mock.callCount(), 3);
	assert.ok(logged.mock.calls.every((call) => !inspect(call.arguments).includes("secret-token")));
});
