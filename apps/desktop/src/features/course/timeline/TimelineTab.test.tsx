import { act, screen, waitFor, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import type { ApplySkipReason } from "@/api/types";
import { todayIso } from "@/lib/format";
import { paths } from "@/lib/routes";
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

/**
 * "Not used in <courses>: <why>": each course by the start of its code (the rest of the code
 * names the session, which depends on today's date).
 */
function notUsed(courses: string[], why: string): RegExp {
  const escaped = why.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^Not used in ${courses.map((c) => `${c}[^,:]*`).join(", ")}: ${escaped}$`);
}

/** The innermost element whose whole text matches `re` (a sentence may hold a code element). */
function sentence(re: RegExp) {
  return (_: string, element: Element | null) =>
    !!element &&
    re.test(element.textContent ?? "") &&
    ![...element.children].some((child) => re.test(child.textContent ?? ""));
}

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

    // The listing read again takes a while, as over IPC: the ticks wait for it.
    const listing = api.sessionCourses;
    vi.spyOn(api, "sessionCourses").mockImplementation(async (courseId) => {
      await new Promise((resolve) => setTimeout(resolve, 50));
      return listing(courseId);
    });
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
    // The ticks come back by the same rule: ticked again with no dates of its own, the other one
    // unticked with its dates back.
    const group = await othersGroup();
    await waitFor(() =>
      expect(checkbox(group, /DEM240H5/)).toHaveAttribute("aria-checked", "true"),
    );
    expect(checkbox(group, /DEM205H5/)).toHaveAttribute("aria-checked", "false");
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
    expect((await api.courseOverview(LEGACY)).timeline.term).toMatchObject({
      anchor_origin: "user",
      student_dates_confirmed: true,
    });
  });

  it("confirms a last day of classes kept alone from version 0.1 before giving it on", async () => {
    // No anchor of its own says "legacy" for an end alone: the term says it isn't confirmed.
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await api.restoreCourseDates([
      { course_id: FITTED, start: null, end: "2026-12-04", legacy: true, copied_from: null },
    ]);
    const { user } = await openCourse(FITTED, { query: "tab=timeline", api });
    expect(screen.getByText("Check this course's dates")).toBeInTheDocument();
    expect(checkbox(await othersGroup(), /DEM240H5/)).toHaveAttribute("aria-checked", "true");
    const setTerm = vi.spyOn(api, "setCourseTerm");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText("Saved, and used in 1 other course")).toBeInTheDocument();
    expect(setTerm).toHaveBeenCalledWith(FITTED, null, "2026-12-04");
    expect((await api.courseOverview(FITTED)).timeline.term.student_dates_confirmed).toBe(true);
    expect((await api.courseOverview(UNLABELLED)).timeline.term.student_end).toBe("2026-12-04");
    await waitFor(() => expect(screen.queryByText("Check this course's dates")).toBeNull());
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

  it("keeps the list as it is while a Save runs: the courses ticked then get the dates", async () => {
    const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
    const group = await othersGroup();
    const setTerm = api.setCourseTerm;
    let finish: () => void = () => {};
    vi.spyOn(api, "setCourseTerm").mockImplementation(
      (...args) =>
        new Promise((resolve, reject) => {
          finish = () => void setTerm(...args).then(resolve, reject);
        }),
    );
    // The copy that follows takes a while too.
    const applyTo = api.applyCourseDatesTo;
    let finishCopy: () => void = () => {};
    const apply = vi.spyOn(api, "applyCourseDatesTo").mockImplementation(
      (...args) =>
        new Promise((resolve, reject) => {
          finishCopy = () => void applyTo(...args).then(resolve, reject);
        }),
    );
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await screen.findByRole("button", { name: "Saving…" });
    const box = checkbox(group, /DEM240H5/);
    expect(box).toBeDisabled();
    await user.click(box);
    expect(box).toHaveAttribute("aria-checked", "true");
    finish();
    // Saved here; the copy runs: still no change to the list.
    await waitFor(() => expect(apply).toHaveBeenCalledWith(FITTED, [UNLABELLED]));
    expect(checkbox(group, /DEM240H5/)).toBeDisabled();
    await user.click(checkbox(group, /DEM240H5/));
    expect(checkbox(group, /DEM240H5/)).toHaveAttribute("aria-checked", "true");
    finishCopy();
    expect(await screen.findByText("Saved, and used in 1 other course")).toBeInTheDocument();
    expect(apply).toHaveBeenCalledTimes(1);
    expect(checkbox(group, /DEM240H5/)).toBeEnabled();
  });

  it("gives the dates to no course when the listing fails while this course's are saved", async () => {
    const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    // The save reads the listing again, and that fails.
    const setTerm = api.setCourseTerm;
    vi.spyOn(api, "setCourseTerm").mockImplementation(async (...args) => {
      await setTerm(...args);
      vi.spyOn(api, "sessionCourses").mockRejectedValue(new Error("Synthetic failure"));
    });
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText("Course dates saved")).toBeInTheDocument();
    expect(
      screen.getByText(
        "The other courses of this term couldn't be listed, so these dates stay in this course.",
      ),
    ).toBeInTheDocument();
    expect(apply).not.toHaveBeenCalled();
    expect((await api.courseOverview(UNLABELLED)).timeline.term.student_start ?? null).toBeNull();
  });

  it("ticks a course by how it is now when the form is shown again, not as it was listed before", async () => {
    const { user, router } = await openTimeline(FITTED, "uoft-fall");
    expect(checkbox(await othersGroup(), /DEM240H5/)).toHaveAttribute("aria-checked", "true");
    // In DEM240H5, the student sets its own dates (and gives them to no other course).
    await act(() => router.navigate(`${paths.course(UNLABELLED)}?tab=timeline`));
    await untickOthers(user);
    await user.type(await screen.findByLabelText("Last day of classes (optional)"), "2026-12-04");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await screen.findByText("Course dates saved");
    // Back in this course: DEM240H5 has dates of its own now, so it comes unticked.
    await act(() => router.navigate(`${paths.course(FITTED)}?tab=timeline`));
    await waitFor(() =>
      expect(
        checkbox(screen.getByRole("group", { name: "Also use these dates in" }), /DEM240H5/),
      ).toHaveAccessibleDescription("Has its own dates: they would be replaced."),
    );
    expect(checkbox(await othersGroup(), /DEM240H5/)).toHaveAttribute("aria-checked", "false");
  });

  it("ticks again a course Undo emptied when the form is shown again after the Undo", async () => {
    const { user, router, start } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await screen.findByText("Saved, and used in 1 other course");
    // The student goes elsewhere, and undoes it from there.
    await act(() => router.navigate(paths.courses));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await screen.findByText("1 other course is back as it was");
    await act(() => router.navigate(`${paths.course(FITTED)}?tab=timeline`));
    await waitFor(() =>
      expect(
        checkbox(screen.getByRole("group", { name: "Also use these dates in" }), /DEM240H5/),
      ).toHaveAttribute("aria-checked", "true"),
    );
  });

  it("ticks again a course Undo emptied once the listing can be read again", async () => {
    const { user, api, start, queryClient } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await screen.findByText("Saved, and used in 1 other course");
    // Undo, and the listing read again fails...
    const listing = vi.spyOn(api, "sessionCourses").mockRejectedValue(new Error("Synthetic"));
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await screen.findByText("1 other course is back as it was");
    await screen.findByText(/couldn't be listed/);
    // ...then works again.
    listing.mockRestore();
    await act(() => queryClient.invalidateQueries());
    await waitFor(() =>
      expect(
        checkbox(screen.getByRole("group", { name: "Also use these dates in" }), /DEM240H5/),
      ).toHaveAttribute("aria-checked", "true"),
    );
  });

  it("unticks the courses the dates went to, not one they couldn't be used in", async () => {
    const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
    const group = await othersGroup();
    await user.click(checkbox(group, /DEM205H5/));
    // The facade gives the dates to DEM205H5 and skips DEM240H5.
    const apply = api.applyCourseDatesTo;
    vi.spyOn(api, "applyCourseDatesTo").mockImplementation(async (from, to) => {
      const report = await apply(
        from,
        to.filter((id) => id !== UNLABELLED),
      );
      return {
        ...report,
        skipped: [...report.skipped, { course_id: UNLABELLED, reason: "not_same_session" }],
      };
    });
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    await screen.findByText("Saved, and used in 1 other course");
    await waitFor(() =>
      expect(checkbox(group, /DEM205H5/)).toHaveAttribute("aria-checked", "false"),
    );
    expect(checkbox(group, /DEM240H5/)).toHaveAttribute("aria-checked", "true");
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

  it("gives the dates to no course when the listing fails after a good one", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await api.setCourseTerm(FITTED, "2026-09-08", null);
    const { user, queryClient } = await openCourse(FITTED, { query: "tab=timeline", api });
    expect(checkbox(await othersGroup(), /DEM240H5/)).toHaveAttribute("aria-checked", "true");
    vi.spyOn(api, "sessionCourses").mockRejectedValue(new Error("Synthetic failure"));
    await act(() => queryClient.invalidateQueries());
    await screen.findByText(
      "The other courses of this term couldn't be listed, so these dates stay in this course.",
    );
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    // Nothing changed here and nothing to give: Save waits.
    const save = screen.getByRole("button", { name: "Save dates" });
    expect(save).toHaveAttribute("aria-disabled", "true");
    // With a change, it is saved here only.
    await user.type(screen.getByLabelText("Last day of classes (optional)"), "2026-12-04");
    await user.click(save);
    expect(await screen.findByText("Course dates saved")).toBeInTheDocument();
    expect(apply).not.toHaveBeenCalled();
    expect((await api.courseOverview(UNLABELLED)).timeline.term.student_start ?? null).toBeNull();
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

  // Every reason the facade can give, with what it says for one course: a reason added in Rust
  // fails to compile until it is here.
  const SKIP_REASONS: Record<ApplySkipReason, { one: string; other: string }> = {
    not_same_session: {
      one: "it isn't a course of this term any more.",
      other: "they aren't courses of this term any more.",
    },
    no_dates: {
      one: "there were no dates of yours here to give.",
      other: "there were no dates of yours here to give.",
    },
    unknown_course: {
      one: "it isn't among your courses any more.",
      other: "they aren't among your courses any more.",
    },
    from_course: {
      one: "it's the course these dates are from.",
      other: "it's the course these dates are from.",
    },
    dates_not_confirmed: {
      one: "the dates here aren't confirmed yet. Save them, then try again.",
      other: "the dates here aren't confirmed yet. Save them, then try again.",
    },
    own_calendar: {
      one: "it counts its weeks from a calendar of its own. Change that calendar with the pagelamp course calendar command.",
      other:
        "they count their weeks from calendars of their own. Change those calendars with the pagelamp course calendar command.",
    },
  };

  it.each(
    (Object.keys(SKIP_REASONS) as ApplySkipReason[]).flatMap((reason) =>
      (["one", "other"] as const).map((form) => [reason, form] as const),
    ),
  )("names the courses the dates couldn't be used in, and why (%s, %s)", async (reason, form) => {
    const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
    const courses = form === "one" ? ["DEM240H5"] : ["DEM240H5", "DEM205H5"];
    if (form === "other") await user.click(checkbox(await othersGroup(), /DEM205H5/));
    else await othersGroup();
    vi.spyOn(api, "applyCourseDatesTo").mockResolvedValue({
      applied: [],
      skipped: [UNLABELLED, LEGACY]
        .slice(0, courses.length)
        .map((course_id) => ({ course_id, reason })),
      replaced: [],
    });
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(
      await screen.findByText(sentence(notUsed(courses, SKIP_REASONS[reason][form]))),
    ).toBeInTheDocument();
    // This course's dates were saved all the same.
    expect(screen.getByText("Course dates saved")).toBeInTheDocument();
  });

  it("says why for each reason in one message, the courses of one reason together", async () => {
    const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
    await user.click(checkbox(await othersGroup(), /DEM205H5/));
    const skip = (reason: ApplySkipReason) => [
      { course_id: UNLABELLED, reason },
      { course_id: LEGACY, reason },
    ];
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    apply.mockResolvedValueOnce({ applied: [], skipped: skip("not_same_session"), replaced: [] });
    await user.clear(start);
    await user.type(start, "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(
      await screen.findByText(
        notUsed(["DEM240H5", "DEM205H5"], "they aren't courses of this term any more."),
      ),
    ).toBeInTheDocument();

    apply.mockResolvedValueOnce({
      applied: [],
      skipped: [
        { course_id: LEGACY, reason: "unknown_course" },
        { course_id: UNLABELLED, reason: "dates_not_confirmed" },
      ],
      replaced: [],
    });
    await user.clear(start);
    await user.type(start, "2026-09-09");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    const first = await screen.findByText(
      notUsed(["DEM205H5"], "it isn't among your courses any more."),
    );
    const second = screen.getByText(notUsed(["DEM240H5"], SKIP_REASONS.dates_not_confirmed.one));
    // One message, a sentence per reason, in the order the facade gave them.
    expect(first.parentElement).toBe(second.parentElement);
    expect(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  /** A calendar of its own for `courseId`, saved with the dates form v2 (exams after classes). */
  const ownCalendar = (api: ReturnType<typeof createMockApi>, courseId: string) =>
    api.setCourseDates(courseId, {
      first_class: "2026-09-08",
      last_class: "2026-12-04",
      exams_end: "2026-12-22",
      breaks: [],
    });

  it("offers a course with a calendar of its own unticked and fixed, and gives it nothing", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await ownCalendar(api, UNLABELLED);
    const { user } = await openCourse(FITTED, { query: "tab=timeline", api });
    const group = await othersGroup();
    const box = checkbox(group, /DEM240H5/);
    expect(box).toHaveAttribute("aria-checked", "false");
    expect(box).toBeDisabled();
    expect(box).toHaveAccessibleDescription(
      "Counts its weeks from a calendar of its own: a copy leaves it alone.",
    );
    // The usual hint: another course can still get the dates.
    expect(
      within(group).getByText(
        "Other courses of this term. Untick any you want to leave as they are.",
      ),
    ).toBeInTheDocument();
    await user.click(checkbox(group, /DEM205H5/));
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    await user.clear(screen.getByLabelText("First day of classes"));
    await user.type(screen.getByLabelText("First day of classes"), "2026-09-09");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText("Saved, and used in 1 other course")).toBeInTheDocument();
    expect(apply).toHaveBeenCalledWith(FITTED, [LEGACY]);
    expect((await api.courseOverview(UNLABELLED)).timeline.term.student_start).toBe("2026-09-08");
  });

  it("unticks a course that gets a calendar of its own while the form is open", async () => {
    const { user, api, start, queryClient } = await openTimeline(FITTED, "uoft-fall");
    const group = await othersGroup();
    expect(checkbox(group, /DEM240H5/)).toHaveAttribute("aria-checked", "true");
    // Its calendar is accepted elsewhere (the CLI), and the listing is read again.
    await ownCalendar(api, UNLABELLED);
    await act(() => queryClient.invalidateQueries());
    await waitFor(() => expect(checkbox(group, /DEM240H5/)).toBeDisabled());
    expect(checkbox(group, /DEM240H5/)).toHaveAttribute("aria-checked", "false");
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    await user.clear(start);
    await user.type(start, "2026-09-09");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText("Course dates saved")).toBeInTheDocument();
    expect(apply).not.toHaveBeenCalled();
  });

  it("shows a course's own calendar read-only, with no Save, and says where it's changed", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await ownCalendar(api, FITTED);
    const { user } = await openCourse(FITTED, { query: "tab=timeline", api });
    const code = await screen.findByText("pagelamp course calendar", { selector: "code" });
    expect(code.parentElement?.textContent).toBe(
      "These dates come from this course's own calendar, which counts its weeks: they can't be changed here. Change that calendar with the pagelamp course calendar command.",
    );
    // The calendar's days of classes, not its mirrored span (which ends with the exams).
    const start = screen.getByLabelText("First day of classes");
    const end = screen.getByLabelText("Last day of classes (optional)");
    expect(start).toHaveValue("2026-09-08");
    expect(end).toHaveValue("2026-12-04");
    expect(start).toHaveAttribute("readonly");
    expect(end).toHaveAttribute("readonly");
    expect(start).toHaveAccessibleDescription(expect.stringContaining("own calendar"));
    expect(screen.queryByRole("button", { name: "Save dates" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Clear my dates" })).toBeNull();
    // Nothing here says otherwise: no invitation to set dates, no three weeks of exams (the
    // calendar has its own end of exams), also not read out with the field.
    expect(screen.queryByText(/^If the week shown is wrong/)).toBeNull();
    expect(screen.queryByText("The three weeks after it count as the exam period.")).toBeNull();
    expect(end).not.toHaveAccessibleDescription(expect.stringContaining("three weeks"));
    // Typing changes nothing.
    const setTerm = vi.spyOn(api, "setCourseTerm");
    await user.type(start, "2026-09-20");
    expect(start).toHaveValue("2026-09-08");
    expect(setTerm).not.toHaveBeenCalled();
  });

  it("drops what was typed and its message when the course turns to a calendar of its own", async () => {
    const { user, api, start, end, queryClient } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    await untickOthers(user);
    await user.clear(start);
    await user.type(start, "2026-09-20");
    await user.type(end, "2026-09-10");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    const invalid = "These dates don't work. Check that classes end after they start.";
    expect(await screen.findByText(invalid)).toBeInTheDocument();
    // Its calendar is saved elsewhere (the CLI), and the course is read again.
    await ownCalendar(api, FITTED);
    await act(() => queryClient.invalidateQueries());
    await screen.findByText("pagelamp course calendar", { selector: "code" });
    expect(screen.queryByText(invalid)).toBeNull();
    expect(start).toHaveValue("2026-09-08");
    expect(end).toHaveValue("2026-12-04");
    expect(start).not.toHaveAttribute("aria-invalid");
  });

  it("starts clean when the course turns back from a calendar of its own", async () => {
    const { user, api, start, end, queryClient } = await openTimeline(FITTED, "uoft-fall");
    await othersGroup();
    await untickOthers(user);
    const invalid = "These dates don't work. Check that classes end after they start.";
    /** Its calendar saved elsewhere (the CLI), then cleared there, and the course read again. */
    async function thereAndBack() {
      await ownCalendar(api, FITTED);
      await act(() => queryClient.invalidateQueries());
      await screen.findByText("pagelamp course calendar", { selector: "code" });
      await api.setCourseDates(FITTED, null);
      await act(() => queryClient.invalidateQueries());
      await screen.findByRole("button", { name: "Save dates" });
    }
    // The form's own check of the dates.
    await user.clear(start);
    await user.type(start, "2026-09-20");
    await user.type(end, "2026-09-10");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText(invalid)).toBeInTheDocument();
    await thereAndBack();
    expect(screen.queryByText(invalid)).toBeNull();
    expect(start).not.toHaveAttribute("aria-invalid");
    // A save the facade refused.
    vi.spyOn(api, "setCourseTerm").mockRejectedValueOnce(
      new ApiError("invalid", "These dates are out of order."),
    );
    await user.clear(start);
    await user.type(start, "2026-09-15");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText(invalid)).toBeInTheDocument();
    await thereAndBack();
    expect(screen.queryByText(invalid)).toBeNull();
    expect(start).not.toHaveAttribute("aria-invalid");
    // A copy to the others that failed (the ticks chosen stay through the flips).
    const failed = "Your dates are saved for this course. The other courses weren't changed.";
    await user.click(within(await othersGroup()).getByRole("checkbox", { name: /DEM240H5/ }));
    vi.spyOn(api, "applyCourseDatesTo").mockRejectedValueOnce(new Error("Synthetic failure"));
    await user.clear(start);
    await user.type(start, "2026-09-16");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    expect(await screen.findByText(failed)).toBeInTheDocument();
    await thereAndBack();
    expect(screen.queryByText(failed)).toBeNull();
  });

  it.each([
    ["while the session listing is still read", () => new Promise<never>(() => {})],
    ["when the session listing fails", () => Promise.reject(new Error("Synthetic failure"))],
  ])("shows a course's own calendar read-only %s", async (_, listing) => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await ownCalendar(api, FITTED);
    vi.spyOn(api, "sessionCourses").mockImplementation(listing);
    await openCourse(FITTED, { query: "tab=timeline", api });
    await screen.findByText("pagelamp course calendar", { selector: "code" });
    expect(screen.getByLabelText("First day of classes")).toHaveAttribute("readonly");
    expect(screen.getByLabelText("Last day of classes (optional)")).toHaveValue("2026-12-04");
    expect(screen.queryByRole("button", { name: "Save dates" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Clear my dates" })).toBeNull();
    // Nothing about the other courses: nothing goes to them from here, listed or not.
    expect(screen.queryByText(/couldn't be listed/)).toBeNull();
    expect(screen.queryByText("pagelamp course copy-dates", { selector: "code" })).toBeNull();
  });

  it("doesn't ask to check 0.1 dates on a course with a calendar of its own", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await ownCalendar(api, FITTED);
    // Not a state the facade makes (a calendar accepted or saved confirms its dates, and Undo
    // leaves such a course alone); should unconfirmed dates sit under one, its weeks still come
    // from the calendar, and the form there is read-only.
    const overview = api.courseOverview;
    vi.spyOn(api, "courseOverview").mockImplementation(async (id) => {
      const real = await overview(id);
      if (id !== FITTED) return real;
      const term = { ...real.timeline.term, student_dates_confirmed: false };
      return { ...real, timeline: { ...real.timeline, term } };
    });
    await openCourse(FITTED, { query: "tab=timeline", api });
    await screen.findByText("pagelamp course calendar", { selector: "code" });
    expect(screen.queryByText("Check this course's dates")).toBeNull();
  });

  it("shows the own calendar read-only in a course of no session too", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    await ownCalendar(api, DEMO101);
    await openCourse(DEMO101, { query: "tab=timeline", api });
    await screen.findByText("pagelamp course calendar", { selector: "code" });
    expect(screen.getByLabelText("First day of classes")).toHaveAttribute("readonly");
    expect(screen.queryByRole("button", { name: "Save dates" })).toBeNull();
  });

  it("offers no list from a course with a calendar of its own, and says how to copy its dates", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await ownCalendar(api, FITTED);
    const listed = vi.spyOn(api, "sessionCourses");
    await openCourse(FITTED, { query: "tab=timeline", api });
    const code = await screen.findByText("pagelamp course copy-dates", { selector: "code" });
    expect(code.parentElement?.textContent).toBe(
      "This course counts its weeks from a calendar of its own, so its dates aren't given to other courses from here. Copy them with the pagelamp course copy-dates command.",
    );
    expect(listed).toHaveBeenCalled();
    expect(screen.queryByRole("group", { name: "Also use these dates in" })).toBeNull();
  });

  it.each([
    [
      "two parts",
      {
        second_segment: {
          first_class: "2027-01-06",
          last_class: "2027-04-02",
          restart_numbering: false,
        },
        exams_end: "2027-04-30",
        breaks: [],
      },
    ],
    [
      "a break its weeks skip",
      {
        exams_end: "2026-12-22",
        breaks: [
          {
            kind: "reading_week" as const,
            label: "Reading week",
            start: "2026-11-09",
            end: "2026-11-13",
            numbered: false,
          },
        ],
      },
    ],
  ])("offers no command for a calendar of its own that can't be copied (%s)", async (_, rest) => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    await api.setCourseDates(FITTED, {
      first_class: "2026-09-08",
      last_class: "2026-12-04",
      ...rest,
    });
    await openCourse(FITTED, { query: "tab=timeline", api });
    expect(
      await screen.findByText(
        "This course counts its weeks from a calendar of its own, which two dates can't carry to other courses (it has two parts, a first week other than week 1, no last day of classes, or a break that skips a week). Set the other courses' dates on their own.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("pagelamp course copy-dates", { selector: "code" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Also use these dates in" })).toBeNull();
  });

  it("says so when every other course has a calendar of its own, and copies to none", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    // Calendars of their own and no term dates of their own: only own_calendar keeps them out.
    for (const id of [UNLABELLED, LEGACY]) {
      await ownCalendar(api, id);
      await api.setCourseTerm(id, null, null);
    }
    // Dates of the student's here, which the courses ticked would get.
    await api.setCourseTerm(FITTED, "2026-09-08", null);
    const { user } = await openCourse(FITTED, { query: "tab=timeline", api });
    const group = await othersGroup();
    expect(
      within(group).getByText(
        "The other courses of this term count their weeks from calendars of their own, so these dates stay in this course.",
      ),
    ).toBeInTheDocument();
    for (const box of within(group).getAllByRole("checkbox")) expect(box).toBeDisabled();
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    // Nothing to give and nothing changed: Save waits; with a change, it saves here only.
    const save = screen.getByRole("button", { name: "Save dates" });
    expect(save).toHaveAttribute("aria-disabled", "true");
    await user.type(screen.getByLabelText("Last day of classes (optional)"), "2026-12-04");
    await user.click(save);
    expect(await screen.findByText("Course dates saved")).toBeInTheDocument();
    expect(apply).not.toHaveBeenCalled();
  });

  it("shows the command a course's own calendar is changed with in code style", async () => {
    const { user, api, start } = await openTimeline(FITTED, "uoft-fall");
    await user.click(checkbox(await othersGroup(), /DEM205H5/));
    const apply = vi.spyOn(api, "applyCourseDatesTo");
    for (const skipped of [[UNLABELLED], [UNLABELLED, LEGACY]]) {
      apply.mockResolvedValueOnce({
        applied: [],
        skipped: skipped.map((course_id) => ({ course_id, reason: "own_calendar" })),
        replaced: [],
      });
      await user.clear(start);
      await user.type(start, skipped.length === 1 ? "2026-09-08" : "2026-09-09");
      await user.click(screen.getByRole("button", { name: "Save dates" }));
    }
    // Both messages, the second one too.
    await waitFor(() =>
      expect(screen.getAllByText("pagelamp course calendar", { selector: "code" })).toHaveLength(2),
    );
    const commands = screen.getAllByText("pagelamp course calendar", { selector: "code" });
    // Newest first: each sentence is looked for on its own.
    const texts = commands.map((code) => code.parentElement?.textContent ?? "");
    const one = notUsed(
      ["DEM240H5"],
      "it counts its weeks from a calendar of its own. Change that calendar with the pagelamp course calendar command.",
    );
    const other = notUsed(
      ["DEM240H5", "DEM205H5"],
      "they count their weeks from calendars of their own. Change those calendars with the pagelamp course calendar command.",
    );
    expect(texts.filter((text) => one.test(text))).toHaveLength(1);
    expect(texts.filter((text) => other.test(text))).toHaveLength(1);
  });

  it("keeps a course's name as text in that sentence, whatever it holds", async () => {
    // A code from the LMS with placeholders, nesting, markup and the sentence's own marker.
    const hostile =
      'Lab {{command}} {{courses}} $t(common:errors.internal) <p>x</p> <code style="color:red">X</code> \u0000 end';
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "uoft-fall" });
    const listing = api.sessionCourses;
    vi.spyOn(api, "sessionCourses").mockImplementation(async (courseId) => {
      const listed = await listing(courseId);
      return {
        ...listed,
        courses: listed.courses.map((c) =>
          c.course_id === UNLABELLED ? { ...c, code: hostile } : c,
        ),
      };
    });
    vi.spyOn(api, "applyCourseDatesTo").mockResolvedValue({
      applied: [],
      skipped: [{ course_id: UNLABELLED, reason: "own_calendar" }],
      replaced: [],
    });
    const { user } = await openCourse(FITTED, { query: "tab=timeline", api });
    await othersGroup();
    await user.clear(screen.getByLabelText("First day of classes"));
    await user.type(screen.getByLabelText("First day of classes"), "2026-09-08");
    await user.click(screen.getByRole("button", { name: "Save dates" }));
    const code = await screen.findByText("pagelamp course calendar", { selector: "code" });
    const sentence = code.parentElement?.textContent ?? "";
    expect(sentence).toBe(
      `Not used in ${hostile.replace("\u0000", "\uFFFD")}: it counts its weeks from a calendar of its own. Change that calendar with the pagelamp course calendar command.`,
    );
    // Its markup stayed text: one code element (the command's), no paragraph made from it.
    expect(code.parentElement?.querySelectorAll("code")).toHaveLength(1);
    expect(code.parentElement?.querySelector("p")).toBeNull();
  });

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
      copy_refused: false,
    });
    await screen.findByLabelText("First day of classes");
    expect(screen.queryByRole("group", { name: "Also use these dates in" })).toBeNull();
    expect(screen.queryByText(/couldn't be listed/)).toBeNull();
  });
});
