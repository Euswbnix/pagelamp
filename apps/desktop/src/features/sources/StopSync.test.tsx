import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import { renderRoute } from "@/test/render";

describe("stopping a sync (cancel_sync)", () => {
  it("stops at the next step and says so, without calling it a failure", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 40 });
    const cancel = vi.spyOn(api, "cancelSync");
    const { user } = renderRoute("/sources", { api });
    await user.click(await screen.findByRole("button", { name: "Sync all" }));
    const panel = (await screen.findByRole("heading", { level: 2, name: "Syncing…" })).closest(
      "section",
    );
    if (!panel) throw new Error("no progress panel");
    const stop = within(panel).getByRole("button", { name: "Stop" });
    expect(stop).toHaveAccessibleDescription(/Stops after the file it's on/);
    await user.click(stop);
    expect(cancel).toHaveBeenCalledTimes(1);

    expect(
      await screen.findByRole("heading", { level: 2, name: "Sync stopped" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Sync failed")).toBeNull();
    // The source it stopped in reads as stopped, not failed.
    expect(within(panel).getAllByText("Stopped").length).toBeGreaterThan(0);
    expect(within(panel).queryByRole("button", { name: "Stop" })).toBeNull();
  });
});
