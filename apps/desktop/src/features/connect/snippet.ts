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

/** The table a TOML snippet defines, e.g. "mcp_servers.pagelamp" from its first `[…]` line. */
export function tomlTable(content: string): string | null {
  return /^\[([^\]\s]+)\]\s*$/m.exec(content)?.[1] ?? null;
}

/**
 * The command that removes what a `claude mcp add … <name> -- <command>` snippet added, with
 * the same scope: re-running `add` fails while a server with that name exists.
 */
export function claudeMcpRemove(content: string): string | null {
  const [head] = content.split(" -- ");
  if (!head?.startsWith("claude mcp add ") || head === content) return null;
  const name = head.trim().split(/\s+/).pop();
  if (!name || name.startsWith("-")) return null;
  const scope = /--scope\s+(\S+)/.exec(head)?.[1];
  return `claude mcp remove${scope ? ` --scope ${scope}` : ""} ${name}`;
}
