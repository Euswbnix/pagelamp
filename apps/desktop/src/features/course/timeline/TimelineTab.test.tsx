import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { todayIso } from "@/lib/format";
import { DEMO101, DEMO310, openCourse } from "../testing";

/** "YYYY-MM-DD" shifted by whole days (UTC arithmetic, so no DST surprises). */
function shiftIso(date: string, days: number): string {
  const [y, m, d] = date.split("-").map(Number) as [number, number, number];
  return new Date(Date.UTC(y, m - 1, d + days)).toISOString().slice(0, 10);
}

async function openTimeline(courseId: string) {
  const result = await openCourse(courseId, { query: "tab=timeline" });
  const overview = await result.api.courseOverview(courseId);
  return {
    ...result,
    course: overview.course,
    start: screen.getByLabelText("First day of term"),
    end: screen.getByLabelText("Last day of term (optional)"),
  };
}

describe("Timeline tab", () => {
  it("explains the current week with the backend's evidence", async () => {
    await openCourse(DEMO101, { query: "tab=timeline" });
    const panel = screen.getByRole("tabpanel", { name: "Timeline" });

    expect(
      within(panel).getByText("This is very likely right. The clues are listed below."),
    ).toBeInTheDocument();
    const evidence = within(panel).getByRole("list", { name: "How we worked this out" });
    expect(
      within(evidence).getByText(/^Module 'Week 4: Sampling and Surveys' unlocked \d{4}-/),
    ).toBeInTheDocument();
    expect(within(evidence).getByText("Reading week is not modelled in v0.1")).toBeInTheDocument();
    expect(
      within(panel).getByText(/Reading weeks and breaks aren't taken into account yet\./),
    ).toBeInTheDocument();
  });

  it("prefills the term dates and saves a new start date", async () => {
    const { user, api, course, start, end } = await openTimeline(DEMO101);
    const setTerm = vi.spyOn(api, "setCourseTerm");
    expect(start).toHaveValue(course.term_start);
    expect(end).toHaveValue(course.term_end);
    const save = screen.getByRole("button", { name: "Save dates" });
    expect(save).toBeDisabled(); // nothing changed yet

    const newStart = shiftIso(course.term_start ?? todayIso(), -7);
    await user.clear(start);
    await user.type(start, newStart);
    await user.click(save);

    expect(setTerm).toHaveBeenCalledWith(DEMO101, newStart, course.term_end);
    expect(await screen.findByText("Term dates saved")).toBeInTheDocument();
    expect(
      await screen.findByText(new RegExp(`^Term start set to ${newStart} by you`)),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("First day of term")).toHaveValue(newStart);
  });

  it("saves with Enter and keeps focus in the date field", async () => {
    const { user, course, start } = await openTimeline(DEMO101);
    const newStart = shiftIso(course.term_start ?? todayIso(), 7);
    await user.clear(start);
    await user.type(start, `${newStart}{Enter}`);

    expect(await screen.findByText("Term dates saved")).toBeInTheDocument();
    expect(start).toHaveValue(newStart);
    expect(start).toHaveFocus();
    expect(screen.getByRole("button", { name: "Save dates" })).toBeDisabled();
  });

  it("shows an inline error when the term ends before it starts", async () => {
    const { user, start, end } = await openTimeline(DEMO101);
    await user.clear(end);
    await user.type(end, "2000-01-01");
    expect(start).not.toHaveValue("");
    await user.click(screen.getByRole("button", { name: "Save dates" }));

    expect(
      await screen.findByText("These dates don't work. Check that the term ends after it starts."),
    ).toBeInTheDocument();
    expect(end).toHaveAttribute("aria-invalid", "true");
    expect(start).toHaveAttribute("aria-invalid", "true");
  });

  it("offers to go back to the synced dates only after the student overrides them", async () => {
    const { user, api, course, start } = await openTimeline(DEMO101);
    const setTerm = vi.spyOn(api, "setCourseTerm");
    // Synced dates: nothing to undo.
    expect(screen.queryByRole("button", { name: "Use synced dates" })).not.toBeInTheDocument();

    await user.clear(start);
    await user.type(start, shiftIso(course.term_start ?? todayIso(), -7));
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await user.click(await screen.findByRole("button", { name: "Use synced dates" }));

    expect(setTerm).toHaveBeenLastCalledWith(DEMO101, null, null);
    expect(await screen.findByText("Back to the synced term dates")).toBeInTheDocument();
    expect(screen.getByLabelText("First day of term")).toHaveValue(course.term_start);
    expect(screen.queryByRole("button", { name: "Use synced dates" })).not.toBeInTheDocument();
  });

  it("handles an unknown week and a term that hasn't started", async () => {
    const { user, start } = await openTimeline(DEMO310);
    const panel = screen.getByRole("tabpanel", { name: "Timeline" });
    expect(within(panel).getByText("Week unknown")).toBeInTheDocument();
    expect(
      within(panel).getByText(
        "No term dates and no week-numbered folders — set the term start to fix this",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Use synced dates" })).not.toBeInTheDocument();

    // A start date in the future puts today outside the term.
    await user.type(start, shiftIso(todayIso(), 30));
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(
      await within(panel).findByText(
        "Today is outside this course's term dates. If the term is running, check the dates below.",
      ),
    ).toBeInTheDocument();
  });
});
