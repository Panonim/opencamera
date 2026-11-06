/** Gets the one-time pairing code from the end of the QR code link. */
export function readPairingToken(hash: string): string | null {
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  const token = params.get("t");
  return token && /^[A-Za-z0-9_-]{16,}$/.test(token) ? token : null;
}

/** Waits twice as long after each failed reconnect, up to 10 seconds. */
export function reconnectDelayMs(attempt: number): number {
  return Math.min(10_000, 500 * 2 ** Math.max(0, attempt));
}
