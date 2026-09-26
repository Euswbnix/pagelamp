import { describe, expect, it } from "vitest";
import {
  claudeMcpRemove,
  mcpServerEntry,
  mcpServerName,
  mcpServersKey,
  tomlTable,
} from "./snippet";

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

describe("tomlTable", () => {
  it("names the table the snippet defines", () => {
    const content =
      '[mcp_servers.pagelamp]\ncommand = "/x/pagelamp"\nargs = ["mcp"]\n\n[mcp_servers.pagelamp.env]\nPAGELAMP_HOME = "/d"\n';
    expect(tomlTable(content)).toBe("mcp_servers.pagelamp");
    expect(tomlTable("no table here")).toBeNull();
  });
});

describe("claudeMcpRemove", () => {
  it("removes the server the add command adds, with the same scope", () => {
    // The backend's shape, with a data-folder override and an explicit transport.
    const add =
      "claude mcp add --scope user --env PAGELAMP_HOME='/Users/demo/My Data' --transport stdio pagelamp -- '/Applications/PageLamp.app/Contents/MacOS/pagelamp' mcp";
    expect(claudeMcpRemove(add)).toBe("claude mcp remove --scope user pagelamp");
    expect(claudeMcpRemove("claude mcp add pagelamp -- /x/pagelamp mcp")).toBe(
      "claude mcp remove pagelamp",
    );
  });

  it("gives up on anything else", () => {
    expect(claudeMcpRemove("npx something")).toBeNull();
    expect(claudeMcpRemove("claude mcp add --scope user pagelamp")).toBeNull();
  });
});
