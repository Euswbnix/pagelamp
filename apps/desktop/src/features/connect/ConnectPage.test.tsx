import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { MOCK_BINARY_PATH, mcpClientConfigs } from "@/api/mock/fixtures";
import type { McpClientConfig, McpNoteCode, TemporaryLocation } from "@/api/types";
import i18n from "@/i18n";
import { useUiStore } from "@/stores/ui";
import { renderRoute } from "@/test/render";
import { showsOnCard } from "./ClientNotes";

const CONFIGS = mcpClientConfigs(MOCK_BINARY_PATH);

function config(client: McpClientConfig["client"]): McpClientConfig {
  const found = CONFIGS.find((c) => c.client === client);
  if (!found) throw new Error(`fixture has no ${client} config`);
  return found;
}

function mockApi(overrides: Partial<ReturnType<typeof createMockApi>> = {}) {
  return { ...createMockApi({ latencyMs: 0, syncStepMs: 0 }), ...overrides };
}

/** Configs as the backend sends them when the binary runs from a temporary location. */
function atTemporaryLocation(command: string, where: TemporaryLocation): McpClientConfig[] {
  return mcpClientConfigs(command).map((c) => ({
    ...c,
    launch: { ...c.launch, temporary_location: where },
    notes: ["PageLamp is running from a temporary location.", ...c.notes],
    note_codes: ["run_from_temporary_location" as const, ...c.note_codes],
  }));
}

/** The setup card whose h2 is `title`. */
async function card(title: string) {
  const heading = await screen.findByRole("heading", { level: 2, name: title });
  const article = heading.closest("article");
  if (!article) throw new Error(`no card for ${title}`);
  return article;
}

/** userEvent.setup() swaps in its own clipboard stub, so spy on it after rendering. */
function spyOnClipboard() {
  return vi.spyOn(navigator.clipboard, "writeText");
}

describe("ConnectPage", () => {
  it("shows the title, description and the AI disclosure", async () => {
    renderRoute("/connect");
    expect(
      await screen.findByRole("heading", { level: 1, name: "Connect your AI app" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.getByText(i18n.t("connect:description"))).toBeInTheDocument();
    expect(screen.getByText(i18n.t("disclosure.full"))).toBeInTheDocument();
    // "How it works": no window to keep open, data stays local until the AI app reads it.
    expect(screen.getByText(/There's nothing to keep open/)).toBeInTheDocument();
    expect(screen.getByText(/Your data stays on this computer/)).toBeInTheDocument();
  });

  it("shows a loading placeholder while the setup steps load", async () => {
    const api = mockApi({ mcpClientConfigs: () => new Promise<McpClientConfig[]>(() => {}) });
    renderRoute("/connect", { api });
    expect(await screen.findByRole("status")).toHaveTextContent("Loading…");
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
  });

  it("renders one card per AI app: Claude Desktop, Claude Code, Codex, then the rest", async () => {
    renderRoute("/connect");
    await card("Claude Desktop");
    const titles = screen
      .getAllByRole("article")
      .map((article) => within(article).getByRole("heading", { level: 2 }).textContent);
    expect(titles).toEqual([
      "Claude Desktop",
      "Claude Code",
      config("codex").title,
      "Other MCP clients",
    ]);
  });

  it("sorts by client whatever order the backend sends, with other apps last", async () => {
    const generic: McpClientConfig = {
      ...config("claude_desktop"),
      client: "generic",
      title: "Other apps",
    };
    const api = mockApi({
      mcpClientConfigs: async () => [
        generic,
        config("codex"),
        config("claude_code"),
        config("claude_desktop"),
      ],
    });
    renderRoute("/connect", { api });
    await card("Claude Desktop");
    const titles = screen
      .getAllByRole("article")
      .map((article) => within(article).getByRole("heading", { level: 2 }).textContent);
    expect(titles).toEqual(["Claude Desktop", "Claude Code", config("codex").title, "Other apps"]);
    // Only Claude Desktop gets the "easiest" mark.
    expect(screen.getAllByText("Easiest to start with")).toHaveLength(1);
    expect(
      within(await card("Claude Desktop")).getByText("Easiest to start with"),
    ).toBeInTheDocument();
  });

  it("shows every note, localised from its code, in a note callout", async () => {
    renderRoute("/connect");
    const desktop = await card("Claude Desktop");
    const note = within(desktop).getByRole("note");
    expect(
      within(note).getByText("Works on every Claude plan, including Free."),
    ).toBeInTheDocument();
    for (const c of CONFIGS) {
      const appCard = await card(c.title);
      for (const code of c.note_codes) {
        // Some are said elsewhere: the page-level warning, or that app's own steps.
        if (!showsOnCard(c.client, code)) continue;
        const known = code as Exclude<
          McpNoteCode,
          "run_from_temporary_location" | "quit_before_editing"
        >;
        expect(within(appCard).getByText(i18n.t(`connect:noteCodes.${known}`))).toBeVisible();
      }
    }
  });

  it("falls back to the backend's English text for a note it has no code for", async () => {
    const [first] = CONFIGS;
    if (!first) throw new Error("no configs");
    const newer = {
      ...first,
      notes: ["A note from a newer backend."],
      note_codes: ["from_the_future" as McpClientConfig["note_codes"][number]],
    };
    useUiStore.getState().setLocale("zh-CN");
    renderRoute("/connect", {
      api: mockApi({ mcpClientConfigs: vi.fn().mockResolvedValue([newer]) }),
    });
    const text = await screen.findByText("A note from a newer backend.");
    expect(text).toHaveAttribute("lang", "en");
  });

  it("copies the exact configuration snippet", async () => {
    const { user } = renderRoute("/connect");
    const desktop = await card("Claude Desktop");
    const writeText = spyOnClipboard();
    await user.click(
      within(desktop).getByRole("button", { name: "Copy Claude Desktop configuration" }),
    );
    expect(writeText).toHaveBeenCalledWith(config("claude_desktop").content);
    expect(within(desktop).getAllByText("Copied").length).toBeGreaterThan(0);
  });

  it("shows the config file path and copies it", async () => {
    const { user } = renderRoute("/connect");
    const desktop = await card("Claude Desktop");
    const path = config("claude_desktop").config_path_hint ?? "";
    expect(within(desktop).getByText(path)).toBeInTheDocument();
    expect(within(desktop).getByText("Add to a config file")).toBeInTheDocument();
    const writeText = spyOnClipboard();
    await user.click(
      within(desktop).getByRole("button", { name: "Copy Claude Desktop file path" }),
    );
    expect(writeText).toHaveBeenCalledWith(path);
  });

  it("tells students to quit Claude Desktop before editing its config", async () => {
    const { user } = renderRoute("/connect");
    const desktop = await card("Claude Desktop");
    const steps = within(desktop).getByRole("list", { name: "Steps for Claude Desktop" });
    const items = within(steps).getAllByRole("listitem");
    expect(items).toHaveLength(4);
    expect(items[0]).toHaveTextContent(/Quit Claude Desktop completely first/);
    expect(items[3]).toHaveTextContent("Save the file, then open Claude Desktop again.");

    // A file that already has mcpServers gets only the pagelamp entry.
    const writeText = spyOnClipboard();
    await user.click(
      within(desktop).getByRole("button", { name: "Copy Claude Desktop entry only" }),
    );
    const entry = writeText.mock.calls[0]?.[0] ?? "";
    expect(entry.startsWith('"pagelamp": {')).toBe(true);
    expect(JSON.parse(`{${entry}}`)).toEqual(
      JSON.parse(config("claude_desktop").content).mcpServers,
    );
  });

  it("says when each app picks up the change in its own steps, not a generic note", async () => {
    renderRoute("/connect");
    // The backend sends quit_before_editing here (older ones: restart_client_after_change);
    // the numbered steps already say it, in the right order.
    const desktop = await card("Claude Desktop");
    expect(config("claude_desktop").note_codes).toContain("quit_before_editing");
    expect(within(desktop).queryByText(/Quit Claude Desktop before editing its config/)).toBeNull();
    // Claude Code has no config file to change: new sessions pick it up.
    const code = await card("Claude Code");
    expect(config("claude_code").note_codes).toContain("restart_client_after_change");
    expect(
      within(code).getByText("New Claude Code sessions have PageLamp after you run this once."),
    ).toBeVisible();
    // Codex: restart, or just a new session.
    const codex = await card(config("codex").title);
    expect(
      within(codex).getByText("Save the file, then restart the app, or start a new Codex session."),
    ).toBeVisible();
    // Nowhere the generic "quit and reopen after changing its config".
    expect(screen.queryByText(/Quit and reopen the app after changing/)).toBeNull();
  });

  it("says to replace an existing pagelamp entry instead of adding a second one", async () => {
    renderRoute("/connect");
    const desktop = await card("Claude Desktop");
    expect(
      within(desktop).getByText(/If there's already a "pagelamp" entry, replace it:/),
    ).toBeVisible();
  });

  it("adds an mcpServers key to a Claude Desktop file that has other settings", async () => {
    const { user } = renderRoute("/connect");
    const desktop = await card("Claude Desktop");
    expect(
      within(desktop).getByText(/put it right after the opening \{, and add a comma after it/),
    ).toBeVisible();
    const writeText = spyOnClipboard();
    await user.click(
      within(desktop).getByRole("button", { name: "Copy Claude Desktop mcpServers section" }),
    );
    const key = writeText.mock.calls[0]?.[0] ?? "";
    // Claude Desktop writes "preferences" itself. Following the step literally: right after
    // the opening {, then a comma before the next setting.
    const merged = JSON.parse(`{${key}, "preferences": {"sidebarMode": "chat"}}`);
    expect(merged.mcpServers).toEqual(JSON.parse(config("claude_desktop").content).mcpServers);
  });

  it("keeps the generic card neutral: no Claude Desktop steps, just the server", async () => {
    const { user } = renderRoute("/connect");
    const other = await card("Other MCP clients");
    const steps = within(other).getByRole("list", { name: "Steps for Other MCP clients" });
    const items = within(steps).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[1]).toHaveTextContent(/The exact format depends on the app\./);
    expect(within(other).queryByText(/Quit|⌘Q|new or empty|mcpServers/)).toBeNull();
    const writeText = spyOnClipboard();
    await user.click(
      within(other).getByRole("button", { name: "Copy Other MCP clients configuration" }),
    );
    expect(writeText).toHaveBeenCalledWith(config("generic").content);
  });

  it("warns at the top when PageLamp runs from the disk image, not in every card", async () => {
    const configs = atTemporaryLocation(
      "/Volumes/PageLamp/PageLamp.app/Contents/MacOS/pagelamp",
      "disk_image",
    );
    renderRoute("/connect", { api: mockApi({ mcpClientConfigs: async () => configs }) });

    const title = await screen.findByText("Move PageLamp to Applications before connecting");
    expect(title.closest("[data-slot=alert]")).toHaveTextContent(
      /running from the disk image.*drag it into your Applications folder, open it from there/,
    );
    // Above everything else on the page, including the AI disclosure.
    const disclosure = screen.getByText(i18n.t("disclosure.full"));
    expect(
      title.compareDocumentPosition(disclosure) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // Said once, prominently, not again in each app's "Good to know".
    expect(screen.queryByText("PageLamp is running from a temporary location.")).toBeNull();
    expect(screen.getAllByText(/temporary location|Move PageLamp to Applications/)).toHaveLength(1);
  });

  it("tells AppImage users to use an installed build instead", async () => {
    const configs = atTemporaryLocation("/tmp/.mount_PageLaXyZ/usr/bin/pagelamp", "appimage");
    renderRoute("/connect", { api: mockApi({ mcpClientConfigs: async () => configs }) });
    expect(
      await screen.findByText("Use the installed version for your AI app"),
    ).toBeInTheDocument();
  });

  it("explains a translocated copy (the app was never moved out of Downloads)", async () => {
    const configs = atTemporaryLocation(
      "/private/var/folders/x/T/AppTranslocation/1/d/PageLamp.app/Contents/MacOS/pagelamp",
      "translocated",
    );
    renderRoute("/connect", { api: mockApi({ mcpClientConfigs: async () => configs }) });
    const title = await screen.findByText("Move PageLamp to Applications before connecting");
    expect(title.closest("[data-slot=alert]")).toHaveTextContent(
      /opened from the disk image or from the folder you downloaded it to/,
    );
  });

  it("shows no location warning for an installed app", async () => {
    renderRoute("/connect");
    await card("Claude Desktop");
    expect(screen.queryByText(/before connecting|installed version for your AI app/)).toBeNull();
  });

  it("gives the TOML card its own file path and three steps", async () => {
    const { user } = renderRoute("/connect");
    const codex = await card(config("codex").title);
    const steps = within(codex).getByRole("list", { name: `Steps for ${config("codex").title}` });
    expect(within(steps).getAllByRole("listitem")).toHaveLength(3);
    expect(within(codex).getByText("~/.codex/config.toml")).toBeInTheDocument();
    const writeText = spyOnClipboard();
    await user.click(
      within(codex).getByRole("button", { name: `Copy ${config("codex").title} configuration` }),
    );
    expect(writeText).toHaveBeenCalledWith(config("codex").content);
  });

  it("shows terminal steps for a shell command and copies the command", async () => {
    const { user } = renderRoute("/connect");
    const code = await card("Claude Code");
    expect(within(code).getByText("Run in a terminal")).toBeInTheDocument();
    expect(within(code).getByText("Open a terminal.")).toBeInTheDocument();
    const steps = within(code).getByRole("list", { name: "Steps for Claude Code" });
    expect(within(steps).getAllByRole("listitem")).toHaveLength(3);
    // No config file for a shell command.
    expect(within(code).queryByRole("button", { name: /file path/ })).not.toBeInTheDocument();
    const writeText = spyOnClipboard();
    await user.click(within(code).getByRole("button", { name: "Copy Claude Code command" }));
    expect(writeText).toHaveBeenCalledWith(config("claude_code").content);
  });

  it("suggests questions to try", async () => {
    renderRoute("/connect");
    expect(await screen.findByRole("heading", { level: 2, name: /Try it/ })).toBeInTheDocument();
    expect(screen.getByText("What's happening in my courses this week?")).toBeInTheDocument();
    expect(screen.getByText("Make me a study plan for the next two weeks.")).toBeInTheDocument();
  });

  it("translates the page and example questions into Chinese", async () => {
    useUiStore.getState().setLocale("zh-CN");
    renderRoute("/connect");
    expect(
      await screen.findByRole("heading", { level: 1, name: "连接 AI 应用" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("我这周各门课都有什么安排？")).toBeInTheDocument();
    // Notes are localised from their codes.
    expect(screen.getByText("所有 Claude 套餐都能用，包括免费版。")).toBeInTheDocument();
  });

  it("shows an empty state with a retry when there are no configs", async () => {
    const mcp = vi.fn().mockResolvedValue([]);
    const { user } = renderRoute("/connect", { api: mockApi({ mcpClientConfigs: mcp }) });
    expect(await screen.findByText("No setup steps available")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(mcp).toHaveBeenCalledTimes(2);
  });

  it("shows an error with a retry that recovers", async () => {
    const base = createMockApi({ latencyMs: 0 });
    const mcp = vi
      .fn()
      .mockRejectedValueOnce(new ApiError("internal", "database is locked"))
      .mockImplementation(base.mcpClientConfigs);
    const { user } = renderRoute("/connect", { api: { ...base, mcpClientConfigs: mcp } });
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Couldn't load the setup steps")).toBeInTheDocument();
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(await card("Claude Desktop")).toBeInTheDocument();
  });
});
