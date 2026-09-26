/**
 * The single server entry of a `{ "mcpServers": { "<key>": { … } } }` snippet, as
 * `"<key>": { … }` — what to add when the student's config file already has an `mcpServers`
 * section. Null when the snippet doesn't have exactly that shape.
 */
export function mcpServerEntry(content: string): string | null {
  try {
    const parsed: unknown = JSON.parse(content);
    if (typeof parsed !== "object" || parsed === null) return null;
    const servers: unknown = (parsed as Record<string, unknown>).mcpServers;
    if (typeof servers !== "object" || servers === null || Array.isArray(servers)) return null;
    const entries = Object.entries(servers);
    if (entries.length !== 1) return null;
    const [[key, value]] = entries as [[string, unknown]];
    return `${JSON.stringify(key)}: ${JSON.stringify(value, null, 2)}`;
  } catch {
    return null;
  }
}
