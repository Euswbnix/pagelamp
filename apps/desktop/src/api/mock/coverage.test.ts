import { describe, expect, it } from "vitest";
import { createMockApi } from "./index";

const CANVAS = "canvas:canvas.demo.test";
const DEMO312 = `${CANVAS}/course/312`;
const DEMO205 = `${CANVAS}/course/205`;
const mock = (scenario: "demo" | "canvas-hidden-lists" | "auto-sync-due") =>
  createMockApi({ latencyMs: 0, syncStepMs: 0, scenario });

describe("the mock's Canvas coverage (as the facade gives it)", () => {
  it("has no record until a full sync by this version has written one", async () => {
    const api = mock("demo");
    expect((await api.courseOverview(DEMO205)).coverage).toBeNull();
    // The first full sync after the update writes it, for every Canvas course.
    const result = await api.syncSource(CANVAS, {}, () => {});
    expect(result.course_summaries.find((c) => c.course === "DEMO205")).toMatchObject({
      linked_pages: 0,
      linked_files: 0,
      pages_hidden: false,
      files_hidden: false,
    });
    expect((await api.courseOverview(DEMO205)).coverage).toMatchObject({
      pages_list: "read",
      files_list: "read",
    });
  });

  it("gives every Canvas course its record, and the lists a course doesn't show", async () => {
    const api = mock("canvas-hidden-lists");
    const plain = (await api.courseOverview(DEMO205)).coverage;
    expect(plain).toMatchObject({
      pages_list: "read",
      files_list: "read",
      home_state: "not_a_page",
    });
    const hidden = (await api.courseOverview(DEMO312)).coverage;
    expect(hidden).toMatchObject({
      pages_list: "hidden",
      files_list: "hidden",
      home_kind: "page",
      home_state: "read",
      home: { title: "Welcome to DEMO312" },
      counts: { linked_pages: 2, linked_files: 3 },
      not_readable_more: 0,
    });
    // The files that aren't downloaded come first, one entry for each reason, counted now.
    expect(hidden?.not_readable.slice(0, 3)).toEqual([
      { area: "files", reason: "needs_download", title: null, url: null, count: 2 },
      { area: "files", reason: "too_large", title: null, url: null, count: 1 },
      { area: "pages", reason: "index_hidden", title: null, url: null, count: 1 },
    ]);
    // A folder course has none.
    expect((await api.courseOverview("folder:demo-courses/course/DEMO101")).coverage).toBeNull();
  });

  it("says in a full sync's summary what the record says, and sends the facade's warning", async () => {
    const api = mock("canvas-hidden-lists");
    const warnings: string[] = [];
    const result = await api.syncSource(CANVAS, {}, (event) => {
      if (event.type === "warning") warnings.push(event.message);
    });
    const byCourse = Object.fromEntries(result.course_summaries.map((c) => [c.course, c]));
    expect(byCourse.DEMO312).toMatchObject({
      linked_pages: 2,
      linked_files: 3,
      pages_hidden: true,
      files_hidden: true,
      warnings: 1,
    });
    // A course that shows its lists, with nothing found through links.
    expect(byCourse.DEMO205).toMatchObject({
      linked_pages: 0,
      linked_files: 0,
      pages_hidden: false,
      files_hidden: false,
    });
    expect(warnings).toEqual(["DEMO312: Files tab hidden, used module items only"]);
    expect(result.warnings).toEqual(warnings);
  });

  it("names every course in a light sync's summary and counts nothing of them", async () => {
    const api = mock("auto-sync-due");
    const summary = await api.syncAll({ automatic: "unattended" }, () => {});
    const canvas = summary.results.find((r) => r.source_id === CANVAS);
    const courses = (await api.listCourses()).filter((c) => c.course.source_id === CANVAS);
    expect(canvas?.course_summaries.map((line) => line.course)).toEqual(
      courses.map((c) => c.course.code),
    );
    for (const line of canvas?.course_summaries ?? []) {
      expect(line).toMatchObject({
        modules: 0,
        pages: 0,
        files: 0,
        events: 0,
        linked_pages: 0,
        linked_files: 0,
        not_read: 0,
        pages_hidden: false,
        files_hidden: false,
      });
    }
  });

  it("reads one course for a download: one line, and the file count follows at once", async () => {
    const api = mock("canvas-hidden-lists");
    const result = await api.downloadCourseFiles(DEMO312, () => {});
    expect(result.files_downloaded).toBe(2);
    expect(result.course_summaries.map((c) => c.course)).toEqual(["DEMO312"]);
    expect(result.courses).toBe(1);
    const after = (await api.courseOverview(DEMO312)).coverage;
    expect(after?.not_readable[0]).toEqual({
      area: "files",
      reason: "too_large",
      title: null,
      url: null,
      count: 1,
    });
  });

  it("says nothing of the source's other courses in one course's download", async () => {
    const api = mock("canvas-hidden-lists");
    const before = (await api.courseOverview(DEMO312)).coverage?.written_at;
    const warnings: string[] = [];
    const result = await api.downloadCourseFiles(DEMO205, (event) => {
      if (event.type === "warning") warnings.push(event.message);
    });
    expect(result.course_summaries.map((c) => c.course)).toEqual(["DEMO205"]);
    // DEMO312's Files list isn't shown, but this run didn't read DEMO312.
    expect(warnings).toEqual([]);
    expect(result.warnings.join(" ")).not.toContain("DEMO312");
    expect((await api.courseOverview(DEMO312)).coverage?.written_at).toBe(before);
  });
});
