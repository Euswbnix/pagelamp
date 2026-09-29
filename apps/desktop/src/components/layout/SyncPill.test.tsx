import { act, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createMockApi } from "@/api/mock";
import { useSyncStore } from "@/stores/sync";
import { renderWithProviders } from "@/test/render";
import { SyncPill } from "./SyncPill";

// The pill on its own, not a whole route: rendering /settings and searching its accessible
// names took over 5 s on the Windows runner.
describe("SyncPill", () => {
  it("keeps the last state while this window syncs (the accessory bar has the progress)", async () => {
    renderWithProviders(<SyncPill />);
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
    renderWithProviders(<SyncPill />, { api: createMockApi({ latencyMs: 0, scenario: "busy" }) });
    expect(
      await screen.findByRole("link", { name: "Sync status: Syncing…. Open Sources & sync." }),
    ).toBeInTheDocument();
  });
});
