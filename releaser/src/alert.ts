export type Severity = "critical" | "warning";

/** Longer details are cut, well inside what Slack shows of one message. */
const MAX_TEXT = 3000;

/**
 * An alert as the `text` of a Slack incoming-webhook message, which Slack
 * requires: severity, source and subject on the first line, the detail below.
 * `&`, `<` and `>` are escaped as Slack asks, so a detail cannot form a link or
 * mention a channel.
 */
export function alertText(severity: Severity, source: string, subject: string, detail: string): string {
	const text = `[${severity.toUpperCase()}] ${source}: ${subject}\n${detail}`;
	const cut = text.length > MAX_TEXT ? `${text.slice(0, MAX_TEXT)}…` : text;
	return cut.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** A dead-man's switch needs a steady signal, not one per loop, and some
 *  refuse more than a few requests a minute. */
const HEARTBEAT_INTERVAL_MS = 60_000;

/** HEARTBEAT_URL, checked once at startup: unset, or an http(s) URL. The value
 *  is never echoed, as it usually holds a token. */
export function parseHeartbeatUrl(value: string | undefined): string | undefined {
	if (!value) return undefined;
	let protocol: string | undefined;
	try {
		protocol = new URL(value).protocol;
	} catch {
		// Not a URL at all: refused below.
	}
	if (protocol !== "http:" && protocol !== "https:") throw new Error("invalid HEARTBEAT_URL: expected an http(s) URL");
	return value;
}

/**
 * Request the URL of an external dead-man's switch, which pages once these
 * requests stop. A failed request is logged, never thrown: the missing
 * heartbeat is the signal. Only the error's name and code are logged: the URL
 * usually holds a token, and error messages can quote it.
 */
export async function pingHeartbeat(url: string, timeoutMs = 10_000): Promise<void> {
	try {
		const response = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
		await response.body?.cancel();
		if (!response.ok) console.error(`heartbeat failed: HTTP ${response.status}`);
	} catch (error) {
		const { name, cause } = error as { name?: unknown; cause?: { code?: unknown } };
		console.error(`heartbeat failed: ${name}${cause?.code === undefined ? "" : ` ${cause.code}`}`);
	}
}

/** The heartbeat for `url` (nothing when unset): requested at most once per
 *  `intervalMs`, however often it is called. Returns the request when one goes
 *  out. */
export function throttledHeartbeat(
	url: string | undefined,
	intervalMs = HEARTBEAT_INTERVAL_MS,
	now = Date.now,
): () => Promise<void> | undefined {
	let last = -Infinity;
	return () => {
		if (url === undefined || now() - last < intervalMs) return undefined;
		last = now();
		return pingHeartbeat(url);
	};
}

/** A failed alert delivery is tried again after each of these waits: a relay
 *  that is briefly down or rate-limiting must not swallow a page. */
const ALERT_RETRY_DELAYS_MS = [2_000, 10_000];

/**
 * POST one alert to the webhook and report whether it was delivered. `fetch`
 * resolves on any HTTP answer, so a non-2xx status is checked explicitly: a
 * relay answering 4xx/5xx, a timeout and a connection failure are all logged
 * and count as not delivered. Each is tried again after the next of
 * `retryDelaysMs`, except a 4xx other than 408 and 429: the relay refused the
 * request itself (a wrong URL, a rejected payload), which repeating it cannot
 * change.
 */
export async function postAlert(
	url: string,
	body: unknown,
	timeoutMs = 10_000,
	retryDelaysMs = ALERT_RETRY_DELAYS_MS,
): Promise<boolean> {
	for (let attempt = 0; ; attempt++) {
		let status: number | undefined;
		try {
			const response = await fetch(url, {
				method: "POST",
				headers: { "content-type": "application/json" },
				body: JSON.stringify(body),
				signal: AbortSignal.timeout(timeoutMs),
			});
			if (response.ok) return true;
			status = response.status;
			console.error(`alert webhook failed: HTTP ${response.status}`);
		} catch (webhookError) {
			console.error("alert webhook failed", webhookError);
		}
		const refused = status !== undefined && status < 500 && status !== 408 && status !== 429;
		if (refused || attempt === retryDelaysMs.length) return false;
		await new Promise((resolve) => setTimeout(resolve, retryDelaysMs[attempt]));
	}
}
