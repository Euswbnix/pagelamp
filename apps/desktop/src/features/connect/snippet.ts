function servers(content: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(content);
    if (typeof parsed !== "object" || parsed === null) return null;
    const value: unknown = (parsed as Record<string, unknown>).mcpServers;
    if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
    const record = value as Record<string, unknown>;
    return Object.keys(record).length === 1 ? record : null;
  } catch {
    return null;
  }
}

/**
 * The single server entry of a `{ "mcpServers": { "<key>": { … } } }` snippet, as
 * `"<key>": { … }` — what to add when the student's config file already has an `mcpServers`
 * section. Null when the snippet doesn't have exactly that shape.
 */
export function mcpServerEntry(content: string): string | null {
  const record = servers(content);
  if (!record) return null;
  const [[key, value]] = Object.entries(record) as [[string, unknown]];
  return `${JSON.stringify(key)}: ${JSON.stringify(value, null, 2)}`;
}

/** The server's key in a `{ "mcpServers": { "<key>": … } }` snippet ("pagelamp"). */
export function mcpServerName(content: string): string | null {
  const record = servers(content);
  return record ? (Object.keys(record)[0] ?? null) : null;
}

/**
 * `"mcpServers": { "<key>": { … } }` — what to add as a top-level key when the config file has
 * other settings (Claude Desktop writes "preferences" itself) but no `mcpServers` yet.
 */
export function mcpServersKey(content: string): string | null {
  const record = servers(content);
  return record ? `"mcpServers": ${JSON.stringify(record, null, 2)}` : null;
}
