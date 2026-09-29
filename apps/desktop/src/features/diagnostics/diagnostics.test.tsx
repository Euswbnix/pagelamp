import { QueryClient } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import type { CrashReport } from "@/api/types";
import { Providers } from "@/app/Providers";
import { RouteError } from "@/app/RouteError";
import { brand } from "@/brand";
import { renderRoute } from "@/test/render";
import { describeUiError } from "./useLogUiError";

function mockApi(options: Parameters<typeof createMockApi>[0] = {}) {
  return createMockApi({ latencyMs: 0, syncStepMs: 0, ...options });
}

async function helpSection() {
  const heading = await screen.findByRole("heading", { level: 2, name: "Help & feedback" });
  const region = heading.closest("section");
  if (!region) throw new Error("no Help & feedback section");
  return region;
}

afterEach(() => vi.restoreAllMocks());

describe("diagnostic report", () => {
  it("shows the whole report before copying, then copies exactly what was shown", async () => {
    const api = mockApi();
    const report = vi.spyOn(api, "diagnosticReport");
    const { user } = renderRoute("/settings", { api });
    const writeText = vi.spyOn(navigator.clipboard, "writeText");

    const help = await helpSection();
    await user.click(within(help).getByRole("button", { name: "Copy diagnostic report…" }));

    const dialog = await screen.findByRole("dialog", { name: "Diagnostic report" });
    expect(dialog).toHaveAccessibleDescription(
      "Check that nothing private is in it before sharing.",
    );
    const text = await within(dialog).findByRole("textbox", { name: "Diagnostic report" });
    expect(text).toHaveAttribute("readonly");
    expect((text as HTMLTextAreaElement).value).toContain("# PageLamp diagnostic report");
    // Opening the preview fetched the report but copied nothing.
    expect(report).toHaveBeenCalledTimes(1);
    expect(writeText).not.toHaveBeenCalled();

    await user.click(within(dialog).getByRole("button", { name: "Copy diagnostic report" }));
    expect(writeText).toHaveBeenCalledWith((text as HTMLTextAreaElement).value);
    expect(within(dialog).getByRole("status")).toHaveTextContent("Copied");
  });

  it("fetches a fresh report every time the preview opens", async () => {
    const api = mockApi();
    const report = vi.spyOn(api, "diagnosticReport");
    const { user } = renderRoute("/settings", { api });
    const help = await helpSection();
    const open = within(help).getByRole("button", { name: "Copy diagnostic report…" });

    await user.click(open);
    let dialog = await screen.findByRole("dialog", { name: "Diagnostic report" });
    await within(dialog).findByRole("textbox");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(open).toHaveFocus();

    await user.click(open);
    dialog = await screen.findByRole("dialog", { name: "Diagnostic report" });
    await within(dialog).findByRole("textbox");
    expect(report).toHaveBeenCalledTimes(2);
  });

  it("explains a failed report and offers nothing to copy", async () => {
    const api = mockApi();
    vi.spyOn(api, "diagnosticReport").mockRejectedValue(new ApiError("internal", "Disk full."));
    const { user } = renderRoute("/settings", { api });
    const help = await helpSection();
    await user.click(within(help).getByRole("button", { name: "Copy diagnostic report…" }));

    const dialog = await screen.findByRole("dialog", { name: "Diagnostic report" });
    expect(await within(dialog).findByText("Disk full.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(
      within(dialog).queryByRole("button", { name: "Copy diagnostic report" }),
    ).not.toBeInTheDocument();
  });
});

describe("Help & feedback", () => {
  it("opens the logs folder through the desktop helper", async () => {
    const api = mockApi();
    const reveal = vi.spyOn(api, "revealLogsDir");
    const { user } = renderRoute("/settings", { api });
    const help = await helpSection();
    await user.click(within(help).getByRole("button", { name: "Open logs folder" }));
    expect(reveal).toHaveBeenCalledTimes(1);
  });

  it("opens the brand's new-issue page with nothing added to the URL", async () => {
    const api = mockApi();
    const openExternal = vi.spyOn(api, "openExternal");
    const { user } = renderRoute("/settings", { api });
    const help = await helpSection();
    const link = within(help).getByRole("link", { name: "Report a problem on GitHub" });
    expect(link).toHaveAttribute("href", "https://github.com/Euswbnix/pagelamp/issues/new/choose");

    await user.click(link);
    expect(openExternal).toHaveBeenCalledWith(brand.links.issues);
    const url = new URL(openExternal.mock.calls[0]?.[0] ?? "");
    expect(url.search).toBe("");
    expect(url.hash).toBe("");
  });
});

describe("crash notice", () => {
  const notice = /closed unexpectedly last time/;

  it("shows the last crash above the screen until it is dismissed", async () => {
    const api = mockApi({ scenario: "crashed" });
    const clear = vi.spyOn(api, "clearLastCrash");
    const { user } = renderRoute("/courses", { api });

    const region = await screen.findByRole("region", {
      name: "PageLamp closed unexpectedly last time",
    });
    expect(within(region).getByText(/^It happened /)).toBeInTheDocument();
    expect(within(region).getByRole("link", { name: "Report a problem on GitHub" })).toBeVisible();

    await user.click(within(region).getByRole("button", { name: "Dismiss" }));
    expect(clear).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("region", { name: notice })).toBeNull());
    expect(screen.getByRole("heading", { level: 1 })).toHaveFocus();
    // Cleared for good: the facade no longer reports it.
    expect(await api.lastCrash()).toBeNull();
  });

  it("opens the report preview from the notice", async () => {
    const { user } = renderRoute("/courses", { scenario: "crashed" });
    const region = await screen.findByRole("region", { name: notice });
    await user.click(within(region).getByRole("button", { name: "Copy diagnostic report…" }));
    const dialog = await screen.findByRole("dialog", { name: "Diagnostic report" });
    const text = (await within(dialog).findByRole("textbox")) as HTMLTextAreaElement;
    expect(text.value).toContain("Option::unwrap()");
  });

  it("names the MCP server when that is what crashed", async () => {
    const api = mockApi();
    const crash: CrashReport = {
      time: new Date(Date.now() - 3_600_000).toISOString(),
      version: "0.3.0-alpha.1",
      process: "mcp",
      message: "boom",
      location: null,
    };
    vi.spyOn(api, "lastCrash").mockResolvedValue(crash);
    renderRoute("/courses", { api });
    expect(
      await screen.findByRole("region", {
        name: "PageLamp stopped unexpectedly while your AI app was using it",
      }),
    ).toBeInTheDocument();
  });

  it("keeps the notice and says so when dismissing fails", async () => {
    const api = mockApi({ scenario: "crashed" });
    vi.spyOn(api, "clearLastCrash").mockRejectedValue(new ApiError("internal", "Read-only disk."));
    const { user } = renderRoute("/courses", { api });
    const region = await screen.findByRole("region", { name: notice });
    await user.click(within(region).getByRole("button", { name: "Dismiss" }));
    expect(await screen.findByText("Couldn't dismiss this notice")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: notice })).toBeInTheDocument();
  });

  it("stays away when nothing crashed", async () => {
    const api = mockApi();
    const lastCrash = vi.spyOn(api, "lastCrash");
    renderRoute("/courses", { api });
    expect(await screen.findByRole("heading", { level: 1, name: "Courses" })).toBeInTheDocument();
    await waitFor(() => expect(lastCrash).toHaveBeenCalled());
    expect(screen.queryByRole("region", { name: notice })).not.toBeInTheDocument();
  });
});

describe("error boundary", () => {
  function Boom(): never {
    throw new Error("Render exploded");
  }

  it("writes the crash (message and stack only) to the log once and offers the report", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const api = mockApi();
    const log = vi.spyOn(api, "logUiError");
    const router = createMemoryRouter([
      { path: "/", element: <Boom />, errorElement: <RouteError /> },
    ]);
    render(
      <Providers api={api} queryClient={new QueryClient()}>
        <RouterProvider router={router} />
      </Providers>,
    );

    expect(
      await screen.findByRole("button", { name: "Copy diagnostic report…" }),
    ).toBeInTheDocument();
    await waitFor(() => expect(log).toHaveBeenCalledTimes(1));
    const [message, stack] = log.mock.calls[0] ?? [];
    expect(message).toBe("Error: Render exploded");
    expect(stack).toContain("Render exploded");
  });

  it("describes what a route can throw without its data", () => {
    expect(describeUiError(new ApiError("not_found", "No course."))).toMatchObject({
      message: "ApiError(not_found): No course.",
    });
    expect(describeUiError("plain string")).toEqual({ message: "plain string", stack: null });
  });

  it("caps what it writes", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const api = mockApi();
    const log = vi.spyOn(api, "logUiError");
    function Huge(): never {
      throw new Error("x".repeat(50_000));
    }
    const router = createMemoryRouter([
      { path: "/", element: <Huge />, errorElement: <RouteError /> },
    ]);
    render(
      <Providers api={api} queryClient={new QueryClient()}>
        <RouterProvider router={router} />
      </Providers>,
    );
    await waitFor(() => expect(log).toHaveBeenCalledTimes(1));
    const [message, stack] = log.mock.calls[0] ?? [];
    expect(message?.length).toBeLessThanOrEqual(2_001);
    expect(stack?.length ?? 0).toBeLessThanOrEqual(8_001);
  });
});

describe("start screen", () => {
  it("offers the diagnostic report when the data can't be loaded", async () => {
    const api = mockApi();
    vi.spyOn(api, "status").mockRejectedValue(new ApiError("internal", "Database is locked."));
    renderRoute("/", { api });
    expect(await screen.findByText("Database is locked.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy diagnostic report…" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Report a problem on GitHub" })).toBeInTheDocument();
  });
});
