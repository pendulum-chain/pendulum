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
