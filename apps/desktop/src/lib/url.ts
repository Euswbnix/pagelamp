/**
 * True only for http(s) URLs. Materials can carry `file://` URLs (local course folders) and
 * calendar feeds other schemes; those are shown as plain text, never opened.
 */
export function isHttpUrl(url: string | null | undefined): url is string {
  if (!url) return false;
  try {
    const { protocol } = new URL(url);
    return protocol === "http:" || protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Whether `url` addresses this computer, as the facade's `is_loopback` decides it: localhost,
 * 127.0.0.0/8 or [::1]. False for anything that doesn't parse.
 */
export function isLoopbackUrl(url: string): boolean {
  let host: string;
  try {
    host = new URL(url).hostname.toLowerCase();
  } catch {
    return false;
  }
  return host === "localhost" || host === "[::1]" || /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host);
}
