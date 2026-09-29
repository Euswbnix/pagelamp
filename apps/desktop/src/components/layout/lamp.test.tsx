import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { renderRoute } from "@/test/render";

const pool = () => document.querySelector(".pl-lamp");

describe("lamp band", () => {
  it('lights beside "This week" once it has loaded, and goes out elsewhere', async () => {
    const { router } = renderRoute("/courses");
    const heading = await screen.findByRole("heading", { level: 2, name: "This week" });
    await waitFor(() => expect(pool()).toHaveAttribute("data-lit"));
    expect(pool()).toHaveAttribute("aria-hidden", "true");
    expect(heading.closest(".pl-lamp-text")).not.toBeNull();
    expect(screen.getByRole("heading", { level: 1 }).closest(".pl-lamp-text")).toBeNull();

    await act(() => router.navigate("/settings"));
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    expect(pool()).not.toHaveAttribute("data-lit");
    expect(document.querySelector(".pl-lamp-text")).toBeNull();
  });

  it("stays dark for an empty home screen and for errors", async () => {
    renderRoute("/courses", { scenario: "empty" });
    expect(await screen.findByText("No courses yet")).toBeInTheDocument();
    expect(pool()).not.toHaveAttribute("data-lit");
  });

  it("stays dark when this week's deadlines fail to load", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    api.listDeadlines = vi.fn().mockRejectedValue(new ApiError("internal", "Synthetic failure"));
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 2, name: "This week" });
    await waitFor(() => expect(api.listDeadlines).toHaveBeenCalled());
    expect(pool()).not.toHaveAttribute("data-lit");
  });
});
