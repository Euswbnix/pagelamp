import type { McpClient, McpClientConfig } from "@/api/types";

// Lower rank = shown first. Claude Desktop leads because it works on every Claude plan and
// needs no terminal. Clients not listed here (e.g. "generic") keep the backend's order at the end.
const RANK: Partial<Record<McpClient, number>> = {
  claude_desktop: 0,
  claude_code: 1,
  codex: 2,
};
const LAST = 3;

function rank(client: McpClient): number {
  return RANK[client] ?? LAST;
}

/** Stable sort: Claude Desktop, Claude Code, Codex, then everything else in backend order. */
export function sortClientConfigs(configs: readonly McpClientConfig[]): McpClientConfig[] {
  return [...configs].sort((a, b) => rank(a.client) - rank(b.client));
}
