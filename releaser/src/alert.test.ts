import assert from "node:assert/strict";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { test } from "node:test";
import { postAlert } from "./alert.js";

/** A local webhook answering every POST with `status`, or never when null. */
async function webhook(status: number | null): Promise<{ url: string; server: Server }> {
	const server = createServer((request, response) => {
		request.resume();
		if (status !== null) request.on("end", () => response.writeHead(status).end());
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	return { url: `http://127.0.0.1:${(server.address() as AddressInfo).port}`, server };
}

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
