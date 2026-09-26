import { describe, expect, it } from "vitest";
import { MOCK_BINARY_PATH, mcpClientConfigs } from "@/api/mock/fixtures";
import type { McpClientConfig } from "@/api/types";
import { sortClientConfigs } from "./order";

describe("sortClientConfigs", () => {
  const [desktop, code, codex] = mcpClientConfigs(MOCK_BINARY_PATH) as [
    McpClientConfig,
    McpClientConfig,
    McpClientConfig,
  ];
  const genericA: McpClientConfig = { ...desktop, client: "generic", title: "A" };
  const genericB: McpClientConfig = { ...desktop, client: "generic", title: "B" };

  it("puts known clients first and keeps the backend order for the rest", () => {
    const sorted = sortClientConfigs([genericA, codex, genericB, code, desktop]);
    expect(sorted.map((c) => c.title)).toEqual([desktop.title, code.title, codex.title, "A", "B"]);
  });

  it("does not mutate its input", () => {
    const input = [codex, desktop];
    sortClientConfigs(input);
    expect(input).toEqual([codex, desktop]);
  });
});
