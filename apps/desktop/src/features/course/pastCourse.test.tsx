import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import { renderRoute } from "@/test/render";
import { DEMO099, openCourse } from "./testing";

// A course that is over, inactive or not started has no current week (the facade clears it).
// The screens say where it stands from its lifecycle, never "Week unknown".

// Courses of the phases mock scenario (src/api/mock/courseScenarios.ts).
const INACTIVE = "canvas:canvas.demo.test/course/PHS190"; // no dates, quiet for months
const UPCOMING = "canvas:canvas.demo.test/course/PHS160"; // starts in 40 days

const statusList = () => screen.getByRole("list", { name: "Course status" });

describe("a course with no current week: the header", () => {
  it("says Ended for a course that ended while its dates still name a week", async () => {
    // DEMO099's fixture is in week 5 of its term, and no longer listed in Canvas.
    await openCourse(DEMO099);
    const status = statusList();
    expect(within(status).getByText("Ended")).toBeInTheDocument();
    expect(within(status).getByText("Past course")).toBeInTheDocument();
    expect(within(status).queryByText(/Week/)).toBeNull();
  });

  it("says Inactive for a site nothing happens in", async () => {
    await openCourse(INACTIVE, { scenario: "phases" });
    const status = statusList();
    expect(within(status).getByText("Inactive")).toBeInTheDocument();
    expect(within(status).queryByText("Week unknown")).toBeNull();
  });

  it("says when an upcoming course starts", async () => {
    await openCourse(UPCOMING, { scenario: "phases" });
    const status = statusList();
    expect(within(status).getByText(/^Starts /)).toBeInTheDocument();
    expect(within(status).queryByText("Week unknown")).toBeNull();
  });
});

describe("a course with no current week: the Timeline card", () => {
  it.each([
    ["ended", DEMO099, undefined, "Ended"],
    ["inactive", INACTIVE, "phases" as const, "Inactive"],
  ])(
    "names the %s course and says nothing about how sure a week is",
    async (_state, id, scenario, label) => {
      await openCourse(id, { query: "tab=timeline", scenario });
      const panel = screen.getByRole("tabpanel", { name: "Timeline" });
      const card = within(panel).getByRole("heading", { name: "Where this course is" })
        .parentElement as HTMLElement;
      expect(within(card).getByText(label)).toBeInTheDocument();
      expect(within(card).queryByText("Week unknown")).toBeNull();
      // No confidence sentence and no "materials have reached week N" for a course that is over.
      expect(within(panel).queryByText(/This is very likely right/)).toBeNull();
      expect(within(panel).queryByText(/This is a good estimate/)).toBeNull();
      expect(within(panel).queryByText(/We couldn't work this out reliably/)).toBeNull();
      expect(within(panel).queryByText(/materials have reached week/)).toBeNull();
    },
  );
});

describe("a course with no current week: the week tab", () => {
  it("gives the real reason and doesn't offer to set term dates", async () => {
    await openCourse(DEMO099);
    expect(
      await screen.findByText(
        "This course is over, so it has no current week. Pick a week above to see its materials.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/outside this course's term dates/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Set term dates" })).toBeNull();
  });

  it("says an inactive site has no current week", async () => {
    await openCourse(INACTIVE, { scenario: "phases" });
    expect(
      await screen.findByText(/Nothing has happened in this course for a long time/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Set term dates" })).toBeNull();
  });
});

describe("a course with no current week: the course card", () => {
  it("shows when an upcoming course starts, and no hint to set its dates, when its phase is unknown", async () => {
    // A re-used site named for a later term: no dates of its own, so the phase is unknown.
    const api = createMockApi({ latencyMs: 0, scenario: "phases" });
    const courses = await api.listCourses();
    const unknown = { phase: "unknown" as const, starts_on: null, current_week: null };
    vi.spyOn(api, "listCourses").mockResolvedValue(
      courses.map((c) =>
        c.course.id === UPCOMING || c.course.code === "PHS110"
          ? { ...c, timeline: { ...c.timeline, ...unknown, default_week: null } }
          : c,
      ),
    );
    renderRoute("/courses", { api });
    const list = await screen.findByRole("region", { name: "Your courses" });
    const card = within(list)
      .getByRole("link", { name: /^PHS160\b/ })
      .closest("article") as HTMLElement;
    expect(within(card).getByText(/^Starts /)).toBeInTheDocument();
    expect(within(card).queryByText("Week unknown")).toBeNull();
    expect(within(card).queryByText("Set the first day of classes to fix this")).toBeNull();
    // A current course whose week is unknown still gets the hint.
    const current = within(list)
      .getByRole("link", { name: /^PHS110\b/ })
      .closest("article") as HTMLElement;
    expect(within(current).getByText("Week unknown")).toBeInTheDocument();
    expect(
      within(current).getByText("Set the first day of classes to fix this"),
    ).toBeInTheDocument();
  });
});
