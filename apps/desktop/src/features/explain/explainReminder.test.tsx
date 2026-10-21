import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import { paths } from "@/lib/routes";
import { renderRoute } from "@/test/render";

// A build with the Explain tab and without the calendar screens (alpha.2): no syllabus reading
// on the Timeline tab closes the course's reminder for it.
vi.mock("@/features/course/proposals/availability", () => ({
  CALENDAR_UI: false,
  calendarUiEnabled: () => false,
}));

const READABLE = "canvas:canvas.demo.test/course/205"; // "proposals": week 4 has one material
const REMINDER = /^Check whether your instructor/;

describe("Course → Explain: the question (b) reminder on its own", () => {
  it("closes once question (b) is answered elsewhere, and stays closed", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "proposals" });
    const { user, queryClient } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    const button = await screen.findByRole("button", { name: "Explain week 4" });
    await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
    await user.click(button);
    const result = await screen.findByRole("region", { name: "Explanation of week 4" });
    expect(within(result).getByRole("region", { name: REMINDER })).toBeInTheDocument();
    // Answered in the course's Policy tab.
    await api.setCourseMaterialSharing(READABLE, "allowed");
    await act(() => queryClient.invalidateQueries());
    await waitFor(() => expect(screen.queryByRole("region", { name: REMINDER })).toBeNull());
    // Done with, not just hidden: a later "not sure" doesn't bring it back.
    await api.setCourseMaterialSharing(READABLE, "not_sure");
    await act(() => queryClient.invalidateQueries());
    await screen.findByRole("tablist", { name: "Course sections" });
    expect(screen.queryByRole("region", { name: REMINDER })).toBeNull();
  });
});
