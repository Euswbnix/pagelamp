import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEMO101, DEMO205, openCourse } from "../testing";

const QUESTION = "Can this course's materials (slides, notes) be shared with an AI service?";

describe("question (b): sharing materials with AI services", () => {
  it("starts unanswered, says what happens meanwhile, and saves an answer", async () => {
    const { user, api } = await openCourse(DEMO101, { query: "tab=policy" });
    const save = vi.spyOn(api, "setCourseMaterialSharing");
    const group = await screen.findByRole("radiogroup", { name: QUESTION });
    const radios = within(group).getAllByRole("radio");
    expect(radios).toHaveLength(3);
    for (const radio of radios) expect(radio).not.toBeChecked();
    expect(group).toHaveAccessibleDescription(/Not answered yet\. The first time PageLamp sends/);

    await user.click(within(group).getByRole("radio", { name: "Not sure" }));
    expect(save).toHaveBeenCalledWith(DEMO101, "not_sure");
    expect(await screen.findByText("Answer saved")).toBeInTheDocument();
    expect(await within(group).findByRole("radio", { name: "Not sure" })).toBeChecked();
    expect(screen.queryByText(/Not answered yet/)).toBeNull();
  });

  it("shows a saved answer with what it means, never styled as an error", async () => {
    await openCourse(DEMO205, { query: "tab=policy" });
    const group = await screen.findByRole("radiogroup", { name: QUESTION });
    const notAllowed = within(group).getByRole("radio", { name: "No, it's not allowed" });
    expect(notAllowed).toBeChecked();
    expect(notAllowed).toHaveAccessibleDescription(
      /never sends this course's materials to a cloud service/,
    );
    const section = group.closest("section");
    expect(section?.querySelector(".text-destructive")).toBeNull();
  });
});
