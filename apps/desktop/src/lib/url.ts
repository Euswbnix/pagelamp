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
