import { screen, waitFor, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import type { ApplySkipReason } from "@/api/types";
import { todayIso } from "@/lib/format";
import { DEMO101, DEMO310, openCourse } from "../testing";

// These tests cover the dates form a real alpha.1 build shows (v1: first and last day of
// classes). Form v2 is behind DATES_V2_UI (CourseDatesForm.test.tsx).
vi.mock("./availability", () => ({ DATES_V2_UI: false, datesV2UiEnabled: () => false }));

/** "YYYY-MM-DD" shifted by whole days (UTC arithmetic, so no DST surprises). */
function shiftIso(date: string, days: number): string {
  const [y, m, d] = date.split("-").map(Number) as [number, number, number];
  return new Date(Date.UTC(y, m - 1, d + days)).toISOString().slice(0, 10);
}

// Courses of the uoft-fall and phases mock scenarios (src/api/mock/courseScenarios.ts).
const FITTED = "canvas:canvas.demo.test/course/332"; // week from the professor's posts
const UNLABELLED = "canvas:canvas.demo.test/course/240"; // wide Canvas term, no week numbers
const LEGACY = "canvas:canvas.demo.test/course/205"; // a v0.1 start date to check
const ENDED = "canvas:canvas.demo.test/course/PHS150";

async function openTimeline(courseId: string, scenario?: "uoft-fall" | "phases") {
  const result = await openCourse(courseId, { query: "tab=timeline", scenario });
  const overview = await result.api.courseOverview(courseId);
  return {
    ...result,
    overview,
    panel: screen.getByRole("tabpanel", { name: "Timeline" }),
    start: screen.getByLabelText("First day of classes"),
    end: screen.getByLabelText("Last day of classes (optional)"),
  };
}

/** "Also use these dates in", once the other courses of the session are listed. */
const othersGroup = () => screen.findByRole("group", { name: "Also use these dates in" });

/** Only this course: every other course of the session unticked. */
async function untickOthers(user: UserEvent) {
  for (const box of within(await othersGroup()).getAllByRole("checkbox")) {
    if (box.getAttribute("aria-checked") === "true") await user.click(box);
  }
}

describe("Timeline tab — where this course is", () => {
  it("explains the week with translated evidence and the dates used", async () => {
    const { panel } = await openTimeline(DEMO101);
    const card = within(panel).getByRole("region", { name: "Where this course is" });
    expect(within(card).getByText("Week 4")).toBeInTheDocument();
    expect(
      within(card).getByText("This is very likely right. The clues are listed below."),
    ).toBeInTheDocument();
    const evidence = within(card).getAllByRole("list", { name: "How we worked this out" })[0];
    expect(
      within(evidence as HTMLElement).getByText(
        /^Module “Week 4: Sampling and Surveys” unlocked .+ → week 4$/,
      ),
    ).toBeInTheDocument();
    expect(within(card).getByText("Source")).toBeInTheDocument();
    expect(within(card).getByText("Course folder settings")).toBeInTheDocument();
    expect(within(card).getByRole("region", { name: "Course status" })).toHaveTextContent(
      "Current",
    );
  });

  it("sets a wide Canvas term aside and never prefills the form with it (CAL-2)", async () => {
    const { panel, start, end, overview } = await openTimeline(UNLABELLED, "uoft-fall");
    expect(within(panel).getAllByText("Week unknown").length).toBeGreaterThan(0);
    const notUsed = within(panel).getByRole("list", { name: "Dates not used" });
    expect(within(notUsed).getByText(/^Canvas term dates: /)).toBeInTheDocument();
    expect(
      within(notUsed).getByText(
        "Longer than a teaching term, so it looks like an enrollment window rather than class dates.",
      ),
    ).toBeInTheDocument();
    // The Canvas window is what v0.1 prefilled; now the fields stay empty.
    expect(overview.course.term_start).toBeTruthy();
    expect(start).toHaveValue("");
    expect(end).toHaveValue("");
    expect(screen.queryByText("Filled in with the dates PageLamp is using now.")).toBeNull();
  });

  it("prefills the dates the resolver uses, not the Canvas window", async () => {
    const { start, overview } = await openTimeline(FITTED, "uoft-fall");
    expect(start).toHaveValue(overview.timeline.term.week_one_monday);
    expect(start).not.toHaveValue(overview.course.term_start);
    expect(screen.getByText("Filled in with the dates PageLamp is using now.")).toBeInTheDocument();
  });
});

describe("Timeline tab — course dates form", () => {
  it("saves only the field the student changed", async () => {
    const { user, api, end, start, overview } = await openTimeline(FITTED, "uoft-fall");
    await untickOthers(user);
    const setTerm = vi.spyOn(api, "setCourseTerm");
    const monday = overview.timeline.term.week_one_monday ?? todayIso();
    const lastDay = shiftIso(monday, 12 * 7 + 4);
    await user.type(end, lastDay);
    await user.click(screen.getByRole("button", { name: "Save dates" }));

    // The prefilled start was not touched, so it is not saved as the student's own date.
    expect(setTerm).toHaveBeenCalledWith(FITTED, null, lastDay);
    expect(await screen.findByText("Course dates saved")).toBeInTheDocument();
    expect(end).toHaveValue(lastDay);
    expect(start).toHaveValue(monday);
  });

  it("keeps the student's own untouched date when saving the other one", async () => {
    const { user, api, end, overview } = await openTimeline(LEGACY, "uoft-fall");
    await untickOthers(user);
    const setTerm = vi.spyOn(api, "setCourseTerm");
    const own = overview.timeline.term.student_start;
    expect(own).toBeTruthy();
    const lastDay = shiftIso(own ?? todayIso(), 12 * 7);
    await user.type(end, lastDay);
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(setTerm).toHaveBeenCalledWith(LEGACY, own, lastDay);
  });

  it("saves with Enter and keeps focus in the date field", async () => {
    const { user, start } = await openTimeline(DEMO101);
    const newStart = shiftIso(todayIso(), -14);
    await user.clear(start);
    await user.type(start, `${newStart}{Enter}`);

    expect(await screen.findByText("Course dates saved")).toBeInTheDocument();
    expect(start).toHaveValue(newStart);
    expect(start).toHaveFocus();
    expect(screen.getByRole("button", { name: "Save dates" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("shows an inline error when classes end before they start", async () => {
    const { user, start, end } = await openTimeline(DEMO101);
    await user.clear(end);
    await user.type(end, "2000-01-01");
    expect(start).not.toHaveValue("");
    await user.click(screen.getByRole("button", { name: "Save dates" }));

    expect(
      await screen.findByText("These dates don't work. Check that classes end after they start."),
    ).toBeInTheDocument();
    expect(end).toHaveAttribute("aria-invalid", "true");
    expect(start).toHaveAttribute("aria-invalid", "true");
  });

  it("offers to clear only the student's own dates", async () => {
    const { user, api, start } = await openTimeline(DEMO310);
    const setTerm = vi.spyOn(api, "setCourseTerm");
    expect(screen.queryByRole("button", { name: "Clear my dates" })).not.toBeInTheDocument();

    await user.type(start, shiftIso(todayIso(), -21));
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await user.click(await screen.findByRole("button", { name: "Clear my dates" }));

    expect(setTerm).toHaveBeenLastCalledWith(DEMO310, null, null);
    expect(await screen.findByText("Your dates were cleared")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Clear my dates" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save dates" })).toHaveFocus();
  });

  it("shows a course that hasn't started as upcoming", async () => {
    const { user, start, panel } = await openTimeline(DEMO310);
    await user.type(start, shiftIso(todayIso(), 30));
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    const status = await within(panel).findByRole("region", { name: "Course status" });
    await waitFor(() => expect(status).toHaveTextContent("Upcoming"));
    expect(within(panel).getAllByText(/^Starts /).length).toBeGreaterThan(0);
  });
});

describe("Timeline tab — checking dates kept from version 0.1", () => {
  it("confirms the dates and moves focus to the card", async () => {
    const { user, api, panel } = await openTimeline(LEGACY, "uoft-fall");
    const confirm = vi.spyOn(api, "confirmCourseDates");
    expect(within(panel).getByText("Check this course's dates")).toBeInTheDocument();
    expect(
      within(panel).getByText("Set by you in an earlier version of PageLamp"),
    ).toBeInTheDocument();

    await user.click(within(panel).getByRole("button", { name: "These dates are right" }));
    expect(confirm).toHaveBeenCalledWith(LEGACY);
    expect(await screen.findByText("Dates confirmed")).toBeInTheDocument();
    expect(within(panel).queryByText("Check this course's dates")).toBeNull();
    expect(within(panel).getByText("Set by you")).toBeInTheDocument();
    await waitFor(() =>
      expect(within(panel).getByRole("heading", { name: "Where this course is" })).toHaveFocus(),
    );
  });

  it("goes to the form with Edit dates", async () => {
    const { user, start } = await openTimeline(LEGACY, "uoft-fall");
    await user.click(screen.getByRole("button", { name: "Edit dates" }));
    expect(start).toHaveFocus();
  });
});

describe("Timeline tab — I'm still taking this", () => {
  it("keeps a past course current, then undoes it", async () => {
    const { user, api, panel } = await openTimeline(ENDED, "phases");
    const keep = vi.spyOn(api, "keepCourseCurrent");
    const clear = vi.spyOn(api, "clearKeepCourseCurrent");
    const status = within(panel).getByRole("region", { name: "Course status" });
    expect(status).toHaveTextContent("Ended");

    await user.click(within(status).getByRole("button", { name: "I'm still taking this" }));
    expect(keep).toHaveBeenCalledWith(ENDED, null);
    expect(await screen.findByText("Moved to your current courses")).toBeInTheDocument();
    expect(
      within(status).getByText(/^You marked this course as current until /),
    ).toBeInTheDocument();
    expect(status).toHaveTextContent("Current");

    await user.click(within(status).getByRole("button", { name: "Undo" }));
    expect(clear).toHaveBeenCalledWith(ENDED);
    expect(
      await within(status).findByRole("button", { name: "I'm still taking this" }),
    ).toBeInTheDocument();
  });

  it("refuses a date in the past inline", async () => {
    const { user, panel } = await openTimeline(ENDED, "phases");
    const status = within(panel).getByRole("region", { name: "Course status" });
    await user.click(within(status).getByRole("button", { name: "I'm still taking this" }));
    await user.click(await within(status).findByRole("button", { name: "Change date" }));
    const until = within(status).getByLabelText("Current until");
    expect(until).toHaveFocus();
    await user.clear(until);
    await user.type(until, "2000-01-01");
    await user.click(within(status).getByRole("button", { name: "Save date" }));
    expect(await within(status).findByText("Pick a date from today on.")).toBeInTheDocument();
    expect(until).toHaveAttribute("aria-invalid", "true");
  });
});

describe("Timeline tab — also use these dates in", () => {
  const checkbox = (group: HTMLElement, name: RegExp) =>
    within(group).getByRole("checkbox", { name });

  it("lists the other courses of the session, ticked unless they have dates of their own", async () => {
    await openTimeline(FITTED, "uoft-fall");
    const group = await othersGroup();
    // The two other courses of this Fall session, by code and name; not this one, and not the
    // summer, last Fall or Winter courses.
    expect(within(group).getAllByRole("checkbox")).toHaveLength(2);
    expect(checkbox(group, /DEM240H5/)).toHaveAttribute("aria-checked", "true");
    const withOwn = checkbox(group, /DEM205H5/);
    expect(withOwn).toHaveAttribute("aria-checked", "false");
    expect(withOwn).toHaveAccessibleDescription("Has its own dates: they would be replaced.");
    expect(
      within(group).queryByRole("checkbox", { name: /DEM332H5|DEM101H5|DEM236H5|DEM150H5/ }),
    ).toBeNull();
  });

  it("says that a course is hidden, and offers it like the others", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await api.setCourseHidden(UNLABELLED, true);
    await openCourse(FITTED, { query: "tab=timeline", api });
    const box = checkbox(await othersGroup(), /DEM240H5/);
    expect(box).toHaveAccessibleName(/hidden$/);
    expect(box).toHaveAttribute("aria-checked", "true");
  });

  it("gives the dates saved here to the courses ticked, and says so once", async () => {
    const { user, api, start, end } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.type(end, "2026-12-04");
    await user.click(screen.getByRole("button", { name: "Save dates" }));

    expect(await screen.findByText("Saved, and used in 1 other course")).toBeInTheDocument();
    expect(screen.queryByText("Course dates saved")).toBeNull();
    expect(apply).toHaveBeenCalledWith(FITTED, [UNLABELLED]);
    // There they are the student's own now; the course with dates of its own kept them.
    const there = await api.courseOverview(UNLABELLED);
    expect(there.timeline.term).toMatchObject({
      anchor: "student_confirmed",
      student_start: "2026-09-08",
      student_end: "2026-12-04",
    });
    expect((await api.courseOverview(LEGACY)).timeline.term.student_start).not.toBe("2026-09-08");
  });

  it("gives them to no course the student unticked", async () => {
    const { user, api, end, overview } = await openTimeline(FITTED, "uoft-fall");
    await untickOthers(user);
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    const monday = overview.timeline.term.week_one_monday ?? todayIso();
    await user.type(end, shiftIso(monday, 12 * 7 + 4));
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText("Course dates saved")).toBeInTheDocument();
    expect(apply).not.toHaveBeenCalled();
  });

  it("replaces a course's own dates only when ticked, and Undo puts every course back", async () => {
    const { user, api, start, end } = await openTimeline(FITTED, "uoft-fall");
    const before = (await api.courseOverview(LEGACY)).timeline.term;
    await user.click(checkbox(await othersGroup(), /DEM205H5/));
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.type(end, "2026-12-04");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await screen.findByText("Saved, and used in 2 other courses");
    expect((await api.courseOverview(LEGACY)).timeline.term.student_start).toBe("2026-09-08");

    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(
      await screen.findByText("The 2 other courses are back as they were"),
    ).toBeInTheDocument();
    // The one with dates of its own has them again; the one that had none has none.
    const legacy = (await api.courseOverview(LEGACY)).timeline.term;
    expect(legacy.student_start).toBe(before.student_start);
    expect(legacy.student_end ?? null).toBe(before.student_end ?? null);
    // Still dates from version 0.1 for the student to check, not confirmed by the Undo.
    expect(legacy.anchor_origin).toBe("legacy");
    const unlabelled = (await api.courseOverview(UNLABELLED)).timeline.term;
    expect(unlabelled.student_start ?? null).toBeNull();
    expect(unlabelled.anchor).not.toBe("student_confirmed");
    // This course keeps what was saved here.
    expect((await api.courseOverview(FITTED)).timeline.term.student_start).toBe("2026-09-08");
  });

  it("gives the student's own dates to the courses ticked without a change here", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await api.setCourseTerm(FITTED, "2026-09-08", "2026-12-04");
    const { user } = await openCourse(FITTED, { query: "tab=timeline", api });
    expect(checkbox(await othersGroup(), /DEM240H5/)).toHaveAttribute("aria-checked", "true");
    const setTerm = vi.spyOn(api, "setCourseTerm");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText("Saved, and used in 1 other course")).toBeInTheDocument();
    // Nothing to save here: the dates are already the student's.
    expect(setTerm).not.toHaveBeenCalled();
    expect((await api.courseOverview(UNLABELLED)).timeline.term.student_start).toBe("2026-09-08");
  });

  it("confirms dates kept from version 0.1 here before giving them to the others", async () => {
    // Unconfirmed here, they would arrive confirmed there: Save confirms them here first.
    const { user, api, overview } = await openTimeline(LEGACY, "uoft-fall");
    await othersGroup();
    const own = overview.timeline.term.student_start;
    const setTerm = vi.spyOn(api, "setCourseTerm");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText("Saved, and used in 2 other courses")).toBeInTheDocument();
    expect(setTerm).toHaveBeenCalledWith(LEGACY, own, null);
    expect((await api.courseOverview(LEGACY)).timeline.term.anchor_origin).toBe("user");
  });

  it("takes one Save at a time, while the dates go to the other courses too", async () => {
    const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    let finish: () => void = () => {};
    const apply = vi.spyOn(api, "applyCourseDatesTo").mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = () => resolve({ applied: [], skipped: [], replaced: [] });
        }),
    );
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    const saving = await screen.findByRole("button", { name: "Saving…" });
    expect(saving).toHaveAttribute("aria-disabled", "true");
    await user.click(saving);
    await user.keyboard("{Enter}");
    expect(apply).toHaveBeenCalledTimes(1);
    finish();
    await screen.findByRole("button", { name: "Save dates" });
  });

  it("unticks the courses the dates went to, and keeps what the student chose for the others", async () => {
    const { user, start } = await openTimeline(FITTED, "uoft-fall");
    const group = await othersGroup();
    await user.click(checkbox(group, /DEM240H5/));
    await user.click(checkbox(group, /DEM205H5/));
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await screen.findByText("Saved, and used in 1 other course");
    // The one that got them is unticked now; the one the student unticked stays so.
    await waitFor(() =>
      expect(checkbox(group, /DEM205H5/)).toHaveAttribute("aria-checked", "false"),
    );
    expect(checkbox(group, /DEM240H5/)).toHaveAttribute("aria-checked", "false");
    // Nothing is left to give, and nothing was changed since: Save waits.
    expect(screen.getByRole("button", { name: "Save dates" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("says when the other courses couldn't be listed", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    vi.spyOn(api, "sessionCourses").mockRejectedValue(new Error("Synthetic failure"));
    await openCourse(FITTED, { query: "tab=timeline", api });
    expect(
      await screen.findByText(
        "The other courses of this term couldn't be listed, so these dates stay in this course.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Also use these dates in" })).toBeNull();
  });

  it("gives nothing when this course has no dates of the student's: a prefill isn't theirs", async () => {
    const { user, api } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    const save = screen.getByRole("button", { name: "Save dates" });
    expect(save).toHaveAttribute("aria-disabled", "true");
    await user.click(save);
    expect(apply).not.toHaveBeenCalled();
  });

  it("clears only this course's dates", async () => {
    const { user, api } = await openTimeline(LEGACY, "uoft-fall");
    await othersGroup();
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    await user.click(screen.getByRole("button", { name: "Clear my dates" }));
    await screen.findByText("Your dates were cleared");
    expect(apply).not.toHaveBeenCalled();
  });

  it("says when the other courses weren't changed, with this course's dates saved", async () => {
    const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    vi.spyOn(api, "applyCourseDatesTo").mockRejectedValue(new Error("Synthetic failure"));
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    const failed = "Your dates are saved for this course. The other courses weren't changed.";
    expect(await screen.findByText(failed)).toBeInTheDocument();
    expect((await api.courseOverview(FITTED)).timeline.term.student_start).toBe("2026-09-08");
    // Clearing the dates here ends that message: it isn't true any more.
    await user.click(screen.getByRole("button", { name: "Clear my dates" }));
    await screen.findByText("Your dates were cleared");
    expect(screen.queryByText(failed)).toBeNull();
  });

  // Every reason the facade can give: a reason added in Rust fails to compile until it is here.
  const SKIP_REASONS: Record<ApplySkipReason, true> = {
    not_same_session: true,
    no_dates: true,
    unknown_course: true,
    from_course: true,
    dates_not_confirmed: true,
  };

  it.each(Object.keys(SKIP_REASONS) as ApplySkipReason[])(
    "names the courses the dates couldn't be used in (%s)",
    async (reason) => {
      const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
      await othersGroup();
      vi.spyOn(api, "applyCourseDatesTo").mockResolvedValue({
        applied: [],
        skipped: [{ course_id: UNLABELLED, reason }],
        replaced: [],
      });
      await user.clear(start);
      await user.type(start, "2026-09-08");
      await user.click(screen.getByRole("button", { name: "Save dates" }));
      expect(await screen.findByText(/^Not used in DEM240H5/)).toBeInTheDocument();
      // This course's dates were saved all the same.
      expect(screen.getByText("Course dates saved")).toBeInTheDocument();
    },
  );

  it("says in the other course where its dates came from", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await api.setCourseTerm(FITTED, "2026-09-08", "2026-12-04");
    await api.applyCourseDatesTo(FITTED, [UNLABELLED]);
    await openCourse(UNLABELLED, { query: "tab=timeline", api });
    const card = screen.getByRole("region", { name: "Where this course is" });
    expect(within(card).getByText(/^Your dates, copied from DEM332H5/)).toBeInTheDocument();
  });

  it("lists nothing for a course with no session", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    const listed = vi.spyOn(api, "sessionCourses");
    await openCourse(DEMO101, { query: "tab=timeline", api });
    // Once the listing is in (empty for this course), and the form has rendered it.
    await waitFor(() => expect(listed).toHaveBeenCalled());
    expect(await listed.mock.results[0]?.value).toEqual({
      session: null,
      campus: null,
      courses: [],
    });
    await screen.findByLabelText("First day of classes");
    expect(screen.queryByRole("group", { name: "Also use these dates in" })).toBeNull();
    expect(screen.queryByText(/couldn't be listed/)).toBeNull();
  });
});
