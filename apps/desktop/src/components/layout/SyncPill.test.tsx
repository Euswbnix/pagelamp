import { act, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createMockApi } from "@/api/mock";
import { useSyncStore } from "@/stores/sync";
import { renderRoute } from "@/test/render";

describe("SyncPill", () => {
  it("keeps the last state while this window syncs (the accessory bar has the progress)", async () => {
    renderRoute("/settings");
    const pill = await screen.findByRole("link", { name: /^Sync status: Synced .+ ago\./ });
    act(() => {
      useSyncStore.getState().begin(3);
      useSyncStore
        .getState()
        .apply({ type: "source_started", source_id: "canvas", label: "Demo Canvas" });
    });
    expect(pill).toHaveAccessibleName(/^Sync status: Synced .+ ago\./);
    expect(pill).not.toHaveTextContent("Syncing");
  });

  it("says another process is syncing", async () => {
    renderRoute("/settings", { api: createMockApi({ latencyMs: 0, scenario: "busy" }) });
    expect(
      await screen.findByRole("link", { name: "Sync status: Syncing…. Open Sources & sync." }),
    ).toBeInTheDocument();
  });
});
