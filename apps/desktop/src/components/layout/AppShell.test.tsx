import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { brand } from "@/brand";
import { renderRoute } from "@/test/render";

describe("AppShell", () => {
  it("names the window after the page's heading", async () => {
    renderRoute("/courses");
    await screen.findByRole("heading", { level: 1, name: "Courses" });
    await waitFor(() => expect(document.title).toBe(`Courses – ${brand.productName}`));
  });

  it("moves focus to the page when a navigation leaves it nowhere", async () => {
    const { router } = renderRoute("/courses");
    await screen.findByRole("heading", { level: 1, name: "Courses" });
    (document.activeElement as HTMLElement | null)?.blur();

    await act(() => router.navigate("/settings"));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    expect(screen.getByRole("main")).toHaveFocus();
    await waitFor(() => expect(document.title).toBe(`Settings – ${brand.productName}`));
  });

  it("names the sync status link after its status and destination", async () => {
    renderRoute("/courses");
    expect(
      await screen.findByRole("link", {
        name: /^Sync status: Synced .+ ago\. Open Sources & sync\.$/,
      }),
    ).toBeInTheDocument();
  });
});
