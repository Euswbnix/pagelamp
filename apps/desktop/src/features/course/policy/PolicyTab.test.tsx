import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { brand, localized } from "@/brand";
import { DEMO101, DEMO205, DEMO310, openCourse } from "../testing";

const NOTE = "Syllabus section 4: AI may be used to review material only.";

function headerStatus() {
  return screen.getByRole("list", { name: "Course status" });
}

describe("AI policy tab", () => {
  it("lists all five policies with descriptions and the brand hint", async () => {
    await openCourse(DEMO205, { query: "tab=policy" });

    const group = screen.getByRole("radiogroup", { name: "How can you use AI in this course?" });
    expect(within(group).getAllByRole("radio")).toHaveLength(5);
    expect(within(group).getByRole("radio", { name: "Not set" })).toBeChecked();
    expect(within(group).getByRole("radio", { name: "No AI" })).toHaveAccessibleDescription(
      "No generative AI for this course's assessed work.",
    );
    expect(screen.getByText(localized(brand.aiPolicyHint, "en"))).toBeInTheDocument();
    expect(screen.getByText(/it won't read this course's materials/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save AI policy" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("saves a new policy with a note and updates the header badge", async () => {
    const { user, api } = await openCourse(DEMO205, { query: "tab=policy" });
    const setPolicy = vi.spyOn(api, "setCoursePolicy");
    expect(within(headerStatus()).getByText("Not set")).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "Learning aid only" }));
    await user.type(screen.getByLabelText("Paste the rule from your syllabus (optional)"), NOTE);
    expect(screen.getByText("Unsaved changes")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save AI policy" }));

    expect(setPolicy).toHaveBeenCalledWith(DEMO205, "learning_aid", NOTE);
    expect(await screen.findByText("AI policy saved")).toBeInTheDocument();
    expect(await within(headerStatus()).findByText("Learning aid only")).toBeInTheDocument();
    expect(screen.queryByText("Unsaved changes")).not.toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Learning aid only" })).toBeChecked();
    expect(screen.getByLabelText("Paste the rule from your syllabus (optional)")).toHaveValue(NOTE);
    const save = screen.getByRole("button", { name: "Save AI policy" });
    expect(save).toHaveAttribute("aria-disabled", "true");
    expect(save).toHaveFocus(); // focus isn't dropped to <body> after saving
  });

  it("can be changed with the keyboard and discarded", async () => {
    const { user } = await openCourse(DEMO101, { query: "tab=policy" });
    const current = screen.getByRole("radio", { name: "Learning aid only" });
    expect(current).toBeChecked();

    current.focus();
    // Radix moves focus on a timer and selects while the arrow key is held, like a real press.
    await user.keyboard("{ArrowDown>}");
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "Allowed with citation" })).toBeChecked(),
    );
    await user.keyboard("{/ArrowDown}");
    expect(screen.getByText("Unsaved changes")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByRole("radio", { name: "Learning aid only" })).toBeChecked();
    expect(screen.queryByText("Unsaved changes")).not.toBeInTheDocument();
  });

  it("keeps unsaved changes when switching tabs", async () => {
    const { user } = await openCourse(DEMO205, { query: "tab=policy" });
    await user.click(screen.getByRole("radio", { name: "No AI" }));

    await user.click(screen.getByRole("tab", { name: "Deadlines" }));
    await user.click(screen.getByRole("tab", { name: "AI policy" }));

    expect(screen.getByRole("radio", { name: "No AI" })).toBeChecked();
    expect(screen.getByText("Unsaved changes")).toBeInTheDocument();
  });

  it("explains a failed save by error kind", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    vi.spyOn(api, "setCoursePolicy").mockRejectedValueOnce(
      new ApiError("busy", "Another StudentOS process is already syncing."),
    );
    const { user } = await openCourse(DEMO205, { api, query: "tab=policy" });

    await user.click(screen.getByRole("radio", { name: "No restrictions" }));
    await user.click(screen.getByRole("button", { name: "Save AI policy" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "A sync is already running. Try again when it finishes.",
    );
    expect(screen.getByText("Unsaved changes")).toBeInTheDocument();

    // Editing again clears the old message; a retry then saves.
    await user.click(screen.getByRole("radio", { name: "Allowed with citation" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save AI policy" }));
    expect(await screen.findByText("AI policy saved")).toBeInTheDocument();
  });

  describe("AI access to materials", () => {
    const LABEL = "Let my AI app read this course's materials";

    it("turns reading the materials off and on again", async () => {
      const { user, api } = await openCourse(DEMO101, { query: "tab=policy" });
      const setAccess = vi.spyOn(api, "setCourseAiAccess");
      const toggle = screen.getByRole("switch", { name: LABEL });
      expect(toggle).toBeChecked();
      expect(toggle).toHaveAccessibleDescription(
        "When you ask about this course, your AI app can read its materials.",
      );

      await user.click(toggle);
      expect(setAccess).toHaveBeenCalledWith(DEMO101, false);
      expect(
        await screen.findByText("Your AI app will no longer read this course's materials"),
      ).toBeInTheDocument();
      await waitFor(() =>
        expect(toggle).toHaveAccessibleDescription(
          "Your AI app can still see deadlines and course structure, but not the materials.",
        ),
      );
      expect(toggle).not.toBeChecked();
      expect(toggle).toHaveFocus();

      await user.click(toggle);
      expect(setAccess).toHaveBeenLastCalledWith(DEMO101, true);
      await waitFor(() => expect(toggle).toBeChecked());
    });

    it("withholds the materials of a 'No AI' course whatever the switch says", async () => {
      const { user, api } = await openCourse(DEMO310, { query: "tab=policy" });
      const setAccess = vi.spyOn(api, "setCourseAiAccess");
      const toggle = screen.getByRole("switch", { name: LABEL });
      expect(toggle).not.toBeChecked();
      expect(toggle).toHaveAttribute("aria-disabled", "true");
      expect(toggle).toHaveAccessibleDescription(
        "You marked this course “No AI”, so its materials aren't shared with your AI app. Change the AI policy to share them.",
      );
      await user.click(toggle);
      expect(setAccess).not.toHaveBeenCalled();
    });

    it("keeps the stored switch when the policy becomes 'No AI'", async () => {
      const { user, api } = await openCourse(DEMO101, { query: "tab=policy" });
      await user.click(screen.getByRole("radio", { name: "No AI" }));
      await user.click(screen.getByRole("button", { name: "Save AI policy" }));
      await screen.findByText("AI policy saved");

      const toggle = screen.getByRole("switch", { name: LABEL });
      await waitFor(() => expect(toggle).toHaveAttribute("aria-disabled", "true"));
      expect(toggle).not.toBeChecked();
      expect((await api.courseOverview(DEMO101)).course.ai_access).toBe(true);
    });
  });
});
