import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { DEMO101, DEMO205, DEMO310, openCourse } from "../testing";

function materialsList() {
  return screen.getByRole("region", { name: "Materials" });
}

describe("This week tab", () => {
  it("lists the current week's modules and materials with their text status", async () => {
    await openCourse(DEMO101);

    expect(
      await screen.findByRole("heading", { level: 2, name: "Week 4 (this week)" }),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("region", { name: "Modules" })).getByText(
        "Week 4: Sampling and Surveys",
      ),
    ).toBeInTheDocument();

    const list = materialsList();
    expect(
      within(list).getByRole("link", { name: "Week 4 slides — Sampling and Surveys" }),
    ).toBeInTheDocument();
    expect(within(list).getByText("32 sections")).toBeInTheDocument();
    expect(within(list).getByText("Not downloaded")).toBeInTheDocument();
    expect(within(list).getByText("Couldn't extract text")).toBeInTheDocument();
    expect(
      within(list).getByText("The PDF contains only images; no text could be extracted."),
    ).toBeInTheDocument();
    expect(screen.getByText("4 of 6 materials readable by your AI app")).toBeInTheDocument();
    // Other weeks' materials are not listed.
    expect(within(list).queryByText(/Week 3 slides/)).not.toBeInTheDocument();
  });

  it("shows recent announcements", async () => {
    await openCourse(DEMO101);
    const announcements = screen.getByRole("region", { name: "Recent announcements" });
    expect(
      within(announcements).getByRole("link", { name: "Office hours move to Thursday this week" }),
    ).toBeInTheDocument();
  });

  it("switches weeks with the previous/next buttons", async () => {
    const { user, router } = await openCourse(DEMO101);
    await screen.findByRole("heading", { level: 2, name: "Week 4 (this week)" });
    const previous = screen.getByRole("button", { name: "Previous week" });
    const next = screen.getByRole("button", { name: "Next week" });
    expect(previous).not.toHaveAttribute("aria-disabled");
    expect(next).toHaveAttribute("aria-disabled", "true"); // week 4 is the last week with materials

    await user.click(previous);
    expect(await screen.findByRole("heading", { level: 2, name: "Week 3" })).toBeInTheDocument();
    expect(await screen.findByText("Week 3 slides — Measuring Nothing Carefully")).toBeVisible();
    expect(within(materialsList()).getByText("Can't be read (e.g. video)")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next week" })).not.toHaveAttribute("aria-disabled");
    expect(router.state.location.search).toContain("week=3");

    await user.click(screen.getByRole("button", { name: "Previous week" }));
    await screen.findByRole("heading", { level: 2, name: "Week 2" });
    await user.click(screen.getByRole("button", { name: "Previous week" }));
    expect(await screen.findByRole("heading", { level: 2, name: "Week 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous week" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );

    await user.click(screen.getByRole("button", { name: "Go to this week" }));
    expect(
      await screen.findByRole("heading", { level: 2, name: "Week 4 (this week)" }),
    ).toBeInTheDocument();
    expect(router.state.location.search).not.toContain("week=");
  });

  it("shows an empty state for a week without materials", async () => {
    await openCourse(DEMO101, { query: "week=9" });
    expect(await screen.findByText("No materials found for week 9")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Sources & sync" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous week" })).not.toHaveAttribute(
      "aria-disabled",
    );
    expect(screen.getByRole("button", { name: "Next week" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("explains the fallback when the current week is unknown", async () => {
    const { user } = await openCourse(DEMO310);
    expect(
      await screen.findByText(
        "We couldn't work out which week this course is in, so these are the materials from the last 14 days.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Recent materials" })).toBeInTheDocument();
    expect(within(materialsList()).getByText("Discussion guide")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Previous week" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Set term dates" }));
    expect(screen.getByRole("tab", { name: "Timeline" })).toHaveAttribute("aria-selected", "true");
    // Focus follows to the tab that opened, instead of being lost with the button.
    await waitFor(() => expect(screen.getByRole("tabpanel", { name: "Timeline" })).toHaveFocus());
  });

  it("offers a way back to recent materials when the current week is unknown", async () => {
    const { user, router } = await openCourse(DEMO310, { query: "week=2" });
    expect(await screen.findByText("No materials found for week 2")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show recent materials" }));

    expect(
      await screen.findByRole("heading", { level: 2, name: "Recent materials" }),
    ).toBeInTheDocument();
    expect(router.state.location.search).not.toContain("week=");
    expect(screen.queryByRole("button", { name: "Show recent materials" })).not.toBeInTheDocument();
  });

  it("links only http(s) materials; file:// ones stay plain text", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    const real = await api.weekMaterials(DEMO101, null);
    const [first, ...rest] = real.materials;
    if (!first) throw new Error("fixture has no week-4 materials");
    vi.spyOn(api, "weekMaterials").mockResolvedValue({
      ...real,
      materials: [{ ...first, url: "file:///Users/demo/Courses/DEMO101/week4.pdf" }, ...rest],
    });
    await openCourse(DEMO101, { api });

    const list = await screen.findByRole("region", { name: "Materials" });
    expect(within(list).getByText(first.title)).toBeInTheDocument();
    expect(within(list).queryByRole("link", { name: first.title })).not.toBeInTheDocument();
    expect(within(list).getAllByRole("link")).toHaveLength(rest.length);
  });

  it("shows an error with retry when the week fails to load", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    vi.spyOn(api, "weekMaterials").mockRejectedValueOnce(new ApiError("internal", "Boom."));
    const { user } = await openCourse(DEMO101, { api });

    expect(await screen.findByText("Couldn't load this week's materials")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(
      await screen.findByRole("heading", { level: 2, name: "Week 4 (this week)" }),
    ).toBeInTheDocument();
  });
});

describe("Downloading Canvas files", () => {
  it("says a download can count as viewing, then downloads on confirm", async () => {
    const { user, api } = await openCourse(DEMO205);
    const download = vi.spyOn(api, "downloadCourseFiles");
    const callout = await screen.findByText(
      "1 file here isn't downloaded yet, so your AI app can't read it.",
    );
    expect(callout).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Download files…" }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "Download this course's files?",
    });
    expect(
      within(dialog).getByText(
        "Downloading files through Canvas can count as viewing them (e.g. module 'must view' requirements).",
      ),
    ).toBeInTheDocument();
    expect(download).not.toHaveBeenCalled();

    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(download).toHaveBeenCalledWith(DEMO205, expect.any(Function));
    expect(await screen.findByText("Downloaded 1 file")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Download files…" })).not.toBeInTheDocument(),
    );
  });

  it("explains files the download skipped instead of saying there was nothing new", async () => {
    const { user, api } = await openCourse(DEMO205);
    const real = api.downloadCourseFiles.bind(api);
    const skipped = Array.from({ length: 4 }, (_, i) => `Demo file ${i + 1} skipped (locked)`);
    vi.spyOn(api, "downloadCourseFiles").mockImplementation(async (courseId, onEvent) => ({
      ...(await real(courseId, onEvent)),
      files_downloaded: 0,
      warnings: skipped,
    }));

    await user.click(await screen.findByRole("button", { name: "Download files…" }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "Download this course's files?",
    });
    await user.click(within(dialog).getByRole("button", { name: "Download" }));

    expect(await screen.findByText("Some files couldn't be downloaded")).toBeInTheDocument();
    expect(screen.getByText("Demo file 1 skipped (locked)")).toBeInTheDocument();
    expect(screen.getByText("Demo file 3 skipped (locked)")).toBeInTheDocument();
    expect(screen.queryByText("Demo file 4 skipped (locked)")).toBeNull();
    expect(screen.getByText("…and 1 more")).toBeInTheDocument();
    expect(screen.queryByText("No new files to download")).toBeNull();
  });

  it("doesn't count or offer files that can't be downloaded, and says why", async () => {
    const { user } = await openCourse(DEMO205);
    // Two files aren't downloaded, but the recording is over the size limit.
    expect(
      await screen.findByText("1 file here isn't downloaded yet, so your AI app can't read it."),
    ).toBeInTheDocument();
    const recording = (await screen.findByText("Unit C lecture recording")).closest("li");
    if (!(recording instanceof HTMLElement)) throw new Error("no row for the recording");
    expect(within(recording).getByText("Too large to download")).toBeInTheDocument();
    expect(within(recording).queryByText("Not downloaded")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Download files…" }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "Download this course's files?",
    });
    await user.click(within(dialog).getByRole("button", { name: "Download" }));
    expect(await screen.findByText("Downloaded 1 file")).toBeInTheDocument();
    expect(
      screen.getByText(
        "DEMO205: Unit C lecture recording skipped (larger than the download limit)",
      ),
    ).toBeInTheDocument();
    // Only the blocked file is left, so there's nothing more to offer.
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Download files…" })).not.toBeInTheDocument(),
    );
  });

  it("never offers a download for folder courses", async () => {
    // DEMO101's week 4 has a "not downloaded" archive, but folder courses are local.
    await openCourse(DEMO101);
    expect(await screen.findByText("Survey dataset (large archive)")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download files…" })).not.toBeInTheDocument();
  });

  it("does nothing until the student confirms", async () => {
    const { user, api } = await openCourse(DEMO205);
    const download = vi.spyOn(api, "downloadCourseFiles");
    await user.click(await screen.findByRole("button", { name: "Download files…" }));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(download).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Download files…" })).toBeInTheDocument();
  });
});
