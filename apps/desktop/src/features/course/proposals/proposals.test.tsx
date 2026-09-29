import { screen, waitFor, within } from "@testing-library/react";
import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEMO310, openCourse } from "../testing";

// Courses of the "proposals" mock scenario (src/api/mock/proposals.ts).
const FITTED = "canvas:canvas.demo.test/course/332"; // AI proposal: alternatives + a conflict
const UNLABELLED = "canvas:canvas.demo.test/course/240"; // scan proposal
const LEGACY = "canvas:canvas.demo.test/course/205"; // stale accepted calendar

beforeEach(() => {
  toast.dismiss();
});

async function openProposals(courseId: string) {
  const result = await openCourse(courseId, { query: "tab=timeline", scenario: "proposals" });
  const section = await screen.findByRole("region", { name: "Proposed dates" });
  return { ...result, section };
}

describe("calendar proposal cards", () => {
  it("shows a scan proposal with its quotes and what accepting changes, and accepts it", async () => {
    const { user, api, section } = await openProposals(UNLABELLED);
    const accept = vi.spyOn(api, "acceptCalendarProposal");
    const card = within(section).getByRole("article", {
      name: "Dates found in the course's materials",
    });
    const c = within(card);
    expect(c.getByText(/^Found by PageLamp without AI\./)).toBeInTheDocument();
    expect(c.getByText("First day of classes")).toBeInTheDocument();
    // The material's own words, with where they come from.
    expect(c.getByText(/^“Classes begin Tuesday, /)).toBeInTheDocument();
    expect(c.getAllByText("Course information, p. 1").length).toBeGreaterThan(0);
    expect(c.getByText("Today will be: Week 4")).toBeInTheDocument();
    expect(c.getByText("Today's week: Week unknown → Week 4")).toBeInTheDocument();

    await user.click(c.getByRole("button", { name: "Accept" }));
    expect(accept).toHaveBeenCalledWith(expect.any(Number), null);
    expect(await screen.findByText("Dates accepted")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Proposed dates" })).toBeNull(),
    );
    // The section went with its last proposal; focus continues on the card below.
    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "Where this course is" })).toHaveFocus(),
    );
    expect(
      screen.getByText("Found in the syllabus by PageLamp, confirmed by you"),
    ).toBeInTheDocument();
  });

  it("asks the student to settle conflicts, then accepts with their choices", async () => {
    const { user, api, section } = await openProposals(FITTED);
    const accept = vi.spyOn(api, "acceptCalendarProposal");
    const card = within(section).getByRole("article", {
      name: "Dates read from the syllabus by AI",
    });
    const c = within(card);
    expect(
      c.getByText(/^AI-generated · Demo AI \(API key\) · demo-model-1 · /),
    ).toBeInTheDocument();
    expect(
      c.getByText("3 dates were left out because they couldn't be checked against the materials:"),
    ).toBeInTheDocument();

    // The conflict has no default: Accept waits for a choice.
    const acceptButton = c.getByRole("button", { name: "Accept" });
    expect(acceptButton).toHaveAttribute("aria-disabled", "true");
    expect(c.getByText("Choose a date for each question above first.")).toBeInTheDocument();

    const firstClass = c.getByRole("radiogroup", {
      name: /^First day of classes: The materials give different dates/,
    });
    await user.click(within(firstClass).getAllByRole("radio")[1] as HTMLElement);
    const exams = c.getByRole("radiogroup", {
      name: /^Exam period: These dates don't fit together/,
    });
    await user.click(within(exams).getAllByRole("radio")[1] as HTMLElement);

    await user.click(c.getByRole("button", { name: "Accept with my choices" }));
    const [id, edits] = accept.mock.calls[0] ?? [];
    expect(id).toEqual(expect.any(Number));
    const proposal = (await api.courseCalendar(FITTED)).accepted;
    expect(edits?.first_class).toBe(proposal?.dates[0]?.alternatives[0]?.date);
    expect(edits?.exams_end).not.toBeNull();
    expect(await screen.findByText("Dates accepted")).toBeInTheDocument();
  });

  it("edits a proposal before accepting it", async () => {
    const { user, api, section } = await openProposals(UNLABELLED);
    const accept = vi.spyOn(api, "acceptCalendarProposal");
    await user.click(within(section).getByRole("button", { name: "Edit…" }));
    const form = within(section).getByRole("form", { name: "Edit the proposed dates" });
    const f = within(form);
    expect(f.getByLabelText("First day of classes")).not.toHaveValue("");
    await user.clear(f.getByLabelText("Last day of classes (optional)"));
    await user.click(f.getByRole("button", { name: "Accept these dates" }));
    expect(accept).toHaveBeenCalledWith(
      expect.any(Number),
      expect.objectContaining({ last_class: null }),
    );
    expect(await screen.findByText("Dates accepted")).toBeInTheDocument();
  });

  it("dismisses a proposal", async () => {
    const { user, api, section } = await openProposals(FITTED);
    const dismiss = vi.spyOn(api, "dismissCalendarProposal");
    await user.click(within(section).getByRole("button", { name: "Dismiss" }));
    expect(dismiss).toHaveBeenCalled();
    expect(await screen.findByText("Proposal dismissed")).toBeInTheDocument();
  });

  it("says when the syllabus behind the calendar in force changed", async () => {
    const { section } = await openProposals(LEGACY);
    expect(within(section).getByText(/^The syllabus changed on /)).toBeInTheDocument();
    expect(
      within(section).getByText(
        /^Some quoted words aren't in Course outline \(updated\) any more\./,
      ),
    ).toBeInTheDocument();
  });
});

describe("materials to read dates from", () => {
  async function openSources(courseId: string, scenario: "proposals" | "demo" = "proposals") {
    const result = await openCourse(courseId, { query: "tab=timeline", scenario });
    const section = await screen.findByRole("region", { name: "Read the dates from the syllabus" });
    return { ...result, section };
  }

  it("downloads one outline file only when asked, saying it counts as viewing", async () => {
    const { user, api, section } = await openSources(UNLABELLED);
    const download = vi.spyOn(api, "downloadMaterialFiles");
    const list = within(section).getByRole("list", { name: "Candidate materials" });
    expect(within(list).getByRole("checkbox", { name: "Syllabus.pdf" })).toBeDisabled();
    expect(
      within(list).getByText("Looks like the course outline · No text to read yet"),
    ).toBeInTheDocument();

    await user.click(
      within(list).getByRole("button", {
        name: "Download Syllabus.pdf (counts as viewing in Canvas)",
      }),
    );
    expect(download).toHaveBeenCalledWith(UNLABELLED, [expect.any(String)], expect.any(Function));
    expect(download.mock.calls[0]?.[1]).toHaveLength(1);
    expect(await screen.findByText("Syllabus.pdf downloaded")).toBeInTheDocument();
    await waitFor(() =>
      expect(within(list).getByRole("checkbox", { name: "Syllabus.pdf" })).toBeChecked(),
    );
  });

  it("leaves a candidate out at the student's request", async () => {
    const { user, api, section } = await openSources(UNLABELLED);
    const setSources = vi.spyOn(api, "setCalendarSources");
    const list = within(section).getByRole("list", { name: "Candidate materials" });
    await user.click(within(list).getByRole("checkbox", { name: "Course information" }));
    expect(setSources).toHaveBeenCalledWith(UNLABELLED, [], [expect.any(String)]);
    expect(await screen.findByText("Course information is left out")).toBeInTheDocument();
    expect(
      await within(list).findByText("Course information · Left out by you"),
    ).toBeInTheDocument();
  });

  it("says why AI can't read a No-AI course's syllabus", async () => {
    const { section } = await openSources(DEMO310, "demo");
    expect(
      within(section).getByText(
        /^This course is marked “No AI”, so AI doesn't read its syllabus\./,
      ),
    ).toBeInTheDocument();
    expect(within(section).getByText("No outline or schedule found yet.")).toBeInTheDocument();
  });
});
