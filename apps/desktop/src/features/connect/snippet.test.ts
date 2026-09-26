import { describe, expect, it } from "vitest";
import { mcpServerEntry, mcpServerName, mcpServersKey } from "./snippet";

describe("mcpServerEntry", () => {
  it("extracts the one server entry, ready to paste into an existing mcpServers", () => {
    const content = JSON.stringify(
      { mcpServers: { pagelamp: { command: "/Applications/PageLamp.app/x", args: ["mcp"] } } },
      null,
      2,
    );
    const entry = mcpServerEntry(content);
    expect(entry).toBe(
      `"pagelamp": {\n  "command": "/Applications/PageLamp.app/x",\n  "args": [\n    "mcp"\n  ]\n}`,
    );
    // It is valid JSON once wrapped in an object.
    expect(JSON.parse(`{${entry}}`)).toEqual(JSON.parse(content).mcpServers);
  });

  it("gives up on anything else", () => {
    expect(mcpServerEntry("not json")).toBeNull();
    expect(mcpServerEntry("[mcp_servers.pagelamp]")).toBeNull();
    expect(mcpServerEntry(JSON.stringify({ mcpServers: {} }))).toBeNull();
    expect(mcpServerEntry(JSON.stringify({ mcpServers: { a: {}, b: {} } }))).toBeNull();
    expect(mcpServerEntry(JSON.stringify({ other: 1 }))).toBeNull();
  });
});

describe("mcpServersKey", () => {
  it("wraps the entry as a top-level key to add next to other settings", () => {
    const content = JSON.stringify({ mcpServers: { pagelamp: { command: "x", args: ["mcp"] } } });
    const key = mcpServersKey(content);
    expect(key?.startsWith('"mcpServers": {')).toBe(true);
    expect(JSON.parse(`{"preferences": {}, ${key}}`)).toEqual({
      preferences: {},
      mcpServers: { pagelamp: { command: "x", args: ["mcp"] } },
    });
  });

  it("gives up on a bare server definition", () => {
    expect(mcpServersKey(JSON.stringify({ command: "x", args: ["mcp"] }))).toBeNull();
  });
});

describe("mcpServerName", () => {
  it("names the one server, or nothing", () => {
    expect(mcpServerName(JSON.stringify({ mcpServers: { pagelamp: {} } }))).toBe("pagelamp");
    expect(mcpServerName(JSON.stringify({ command: "x" }))).toBeNull();
  });
});
