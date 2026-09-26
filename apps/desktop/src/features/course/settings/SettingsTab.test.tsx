import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { DEMO101, openCourse } from "../testing";

describe("Settings tab", () => {
  it("hides the course with the switch", async () => {
    const { user, api } = await openCourse(DEMO101, { query: "tab=settings" });
    const setHidden = vi.spyOn(api, "setCourseHidden");
    const toggle = screen.getByRole("switch", { name: "Hide this course" });
    expect(toggle).not.toBeChecked();
    expect(toggle).toHaveAccessibleDescription(
      "Hidden courses are skipped in your course list and never shown to your AI app.",
    );

    await user.click(toggle);

    expect(setHidden).toHaveBeenCalledWith(DEMO101, true);
    expect(
      await screen.findByText("Intro to Demo Studies is hidden. Your AI app won't see it."),
    ).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Hide this course" })).toBeChecked();
    const status = screen.getByRole("list", { name: "Course status" });
    expect(within(status).getByText("Hidden")).toBeInTheDocument();
  });

  it("reports a failed change and leaves the switch as it was", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    vi.spyOn(api, "setCourseHidden").mockRejectedValueOnce(new ApiError("internal", "Boom."));
    const { user } = await openCourse(DEMO101, { api, query: "tab=settings" });

    await user.click(screen.getByRole("switch", { name: "Hide this course" }));

    expect(await screen.findByText("Something unexpected went wrong.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Hide this course" })).not.toBeChecked();
  });
});
