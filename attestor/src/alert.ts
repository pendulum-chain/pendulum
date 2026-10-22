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

/**
 * POST one alert to the webhook and report whether it was delivered. `fetch`
 * resolves on any HTTP answer, so a non-2xx status is checked explicitly: a
 * relay answering 4xx/5xx, a timeout and a connection failure are all logged
 * and count as not delivered.
 */
export async function postAlert(url: string, body: unknown, timeoutMs = 10_000): Promise<boolean> {
	try {
		const response = await fetch(url, {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify(body),
			signal: AbortSignal.timeout(timeoutMs),
		});
		if (response.ok) return true;
		console.error(`alert webhook failed: HTTP ${response.status}`);
	} catch (webhookError) {
		console.error("alert webhook failed", webhookError);
	}
	return false;
}
