import { describe, expect, it } from "vitest";
import type { CoverageView, NotReadable } from "@/api/types";
import { hasDetails, needsAttention, notReadModel, onlyNever } from "./notRead";

const BASE = "https://canvas.demo.test/courses/312";

function entry(
  area: NotReadable["area"],
  reason: NotReadable["reason"],
  title: string | null = null,
  path: string | null = null,
  count = 1,
): NotReadable {
  return { area, reason, title, url: path ? `${BASE}/${path}` : null, count };
}

function view(fields: Partial<CoverageView> = {}): CoverageView {
  return {
    home_kind: "modules",
    home_state: "not_a_page",
    home: null,
    syllabus: null,
    pages_list: "read",
    files_list: "read",
    not_readable: [],
    not_readable_more: 0,
    announcements_synced: 0,
    counts: {},
    written_at: "2026-10-05T13:00:00Z",
    ...fields,
  };
}

describe("what the card says, from the coverage view", () => {
  it("sorts every entry by what the student can do about it", () => {
    const model = notReadModel(
      view({
        home_kind: "page",
        home_state: "read",
        pages_list: "hidden",
        files_list: "hidden",
        counts: { linked_pages: 2, linked_files: 3 },
        not_readable_more: 4,
        not_readable: [
          entry("files", "needs_download", null, null, 2),
          entry("files", "too_large", null, null, 1),
          entry("files", "locked", null, null, 3),
          entry("pages", "index_hidden"),
          entry("files", "index_hidden"),
          entry("pages", "would_mark_viewed", "Lab safety briefing", "pages/lab-safety"),
          entry("home", "would_mark_viewed", null, "pages/welcome"),
          entry("files", "failed_this_sync", "Week 3 notes", "files/31"),
          entry("files", "no_longer_in_canvas", "Old reading list", "files/9"),
          entry("pages", "locked", "Midterm review", "pages/midterm-review"),
          entry("pages", "capped", null, "pages/older-notes"),
          // A page that was read and holds more links than are looked at.
          entry("pages", "capped", "Course schedule", "pages/schedule"),
          entry("syllabus", "capped", null, "assignments/syllabus"),
          entry("assignments", "by_rule", "Assignments", "assignments"),
          entry("quizzes", "by_rule", "Quiz 2", "quizzes/2"),
          entry("grades", "not_read", "Grades", "grades"),
          entry("other", "not_read", "Course evaluations", "evaluations"),
          // A link in a text to somewhere else in Canvas: not an item of the course's menu.
          entry("other", "not_read", null, "../../calendar"),
          entry("external_tool", "outside_canvas", "Demo Reader", "external_tools/7"),
          entry("other", "other_course", null, "../313/pages/intro"),
          entry("other", "other", "Something new", "something"),
        ],
      }),
    );
    expect(model).toMatchObject({
      // It was read once; now it waits for the student (below), so "read" isn't said.
      home: null,
      lists: "both",
      linkedPages: 2,
      linkedFiles: 3,
      toDownload: 2,
      tooLarge: 1,
      lockedFiles: 3,
      more: 4,
      never: { assignments: true, parts: true, menu: true },
    });
    expect(model.mustView.map((i) => [i.title, i.home])).toEqual([
      ["Lab safety briefing", false],
      // The Home page, when a module asks for it to be viewed: named by a string of ours.
      [null, true],
    ]);
    expect(model.failed).toMatchObject({ lists: [], pagesAll: false });
    expect(model.failed.items.map((i) => i.title)).toEqual(["Week 3 notes"]);
    expect(model.gone.map((i) => i.title)).toEqual(["Old reading list"]);
    expect(model.lockedPages.map((i) => i.title)).toEqual(["Midterm review"]);
    expect(model.pastLimit).toEqual([
      { title: null, url: `${BASE}/pages/older-notes`, kind: "page", home: false },
    ]);
    expect(model.cutShort.map((i) => i.title ?? i.url)).toEqual([
      "Course schedule",
      `${BASE}/assignments/syllabus`,
    ]);
    expect(model.writtenAt).toBe("2026-10-05T13:00:00Z");
    expect(model.never.tools.map((i) => i.title)).toEqual(["Demo Reader"]);
    expect(model.never.otherCourses).toHaveLength(1);
    expect(model.other.map((i) => i.title ?? i.url)).toEqual([
      `${BASE}/../../calendar`,
      "Something new",
    ]);
    expect(needsAttention(model)).toBe(true);
    expect(hasDetails(model)).toBe(true);
    expect(onlyNever(model)).toBe(false);
  });

  it("says a list that isn't shown, and a locked Home page, once", () => {
    const model = notReadModel(
      view({
        home_kind: "page",
        home_state: "locked",
        home: { id: "m1", title: "Welcome" },
        pages_list: "hidden",
        not_readable: [
          entry("pages", "index_hidden"),
          entry("pages", "locked", "Welcome", "pages/welcome"),
          entry("pages", "locked", "Midterm review", "pages/midterm-review"),
        ],
      }),
    );
    expect(model.home).toBe("locked");
    expect(model.lists).toBe("pages");
    // The state says both; only the other locked page is listed.
    expect(model.lockedPages.map((i) => i.title)).toEqual(["Midterm review"]);
    expect(model.other).toEqual([]);

    // An entry carries the title as the facade writes it: one space between words.
    const spaced = notReadModel(
      view({
        home_state: "locked",
        home: { id: "m1", title: "  Welcome   to DEMO312 " },
        not_readable: [entry("pages", "locked", "Welcome to DEMO312", "pages/welcome")],
      }),
    );
    expect(spaced.lockedPages).toEqual([]);
  });

  it("doesn't say it read the Home page when that page waits for the student or failed", () => {
    // A module asks for the Home page to be viewed: it was read once, and is left since.
    const waiting = notReadModel(
      view({
        home_kind: "page",
        home_state: "read",
        home: { id: "m1", title: "Read me first" },
        not_readable: [entry("pages", "would_mark_viewed", "Read me first", "modules/items/4")],
      }),
    );
    expect(waiting.home).toBeNull();
    expect(waiting.mustView).toEqual([
      { title: "Read me first", url: `${BASE}/modules/items/4`, kind: "page", home: true },
    ]);

    // Another page that waits says nothing about the Home page.
    const other = notReadModel(
      view({
        home_kind: "page",
        home_state: "read",
        home: { id: "m1", title: "Welcome" },
        not_readable: [entry("pages", "would_mark_viewed", "Lab safety", "pages/lab-safety")],
      }),
    );
    expect(other.home).toBe("read");
    expect(other.mustView[0]?.home).toBe(false);
  });

  it("says 'the lists' once when the course's menu couldn't be read, and no more than that", () => {
    // What the facade writes then: both lists failed, and a bare entry for each.
    const model = notReadModel(
      view({
        pages_list: "failed",
        files_list: "failed",
        not_readable: [entry("pages", "failed_this_sync"), entry("files", "failed_this_sync")],
      }),
    );
    expect(model.failed).toEqual({ lists: ["pagesList", "filesList"], pagesAll: false, items: [] });
    expect(model.other).toEqual([]);
  });

  it("names what failed: a list, the Home page, every page at once", () => {
    const lists = notReadModel(view({ pages_list: "failed", files_list: "failed" }));
    expect(lists.failed.lists).toEqual(["pagesList", "filesList"]);
    expect(needsAttention(lists)).toBe(true);

    // The module list couldn't be read, so no page was: one entry without a title or address.
    const all = notReadModel(view({ not_readable: [entry("pages", "failed_this_sync")] }));
    expect(all.failed).toMatchObject({ pagesAll: true, items: [] });

    // The Home page's own request failed: an entry of the area "home", with the course's
    // address.
    const home = notReadModel(
      view({ home_state: "failed", not_readable: [entry("home", "failed_this_sync", null, "..")] }),
    );
    expect(home.failed.items).toEqual([
      { title: null, url: `${BASE}/..`, kind: "item", home: true },
    ]);

    // It failed and no entry says so: it is named all the same, once.
    const silent = notReadModel(view({ home_state: "failed" }));
    expect(silent.failed.items).toEqual([{ title: null, url: null, kind: "item", home: true }]);
    const said = notReadModel(
      view({
        home_state: "failed",
        home: { id: "m1", title: "Welcome" },
        not_readable: [entry("pages", "failed_this_sync", "Welcome", "pages/welcome")],
      }),
    );
    expect(said.failed.items.map((i) => [i.title, i.home])).toEqual([["Welcome", true]]);
    // Another page that failed doesn't stand for the Home page.
    const both = notReadModel(
      view({
        home_state: "failed",
        home: { id: "m1", title: "Welcome" },
        not_readable: [entry("pages", "failed_this_sync", "Week 2 notes", "pages/week-2")],
      }),
    );
    expect(both.failed.items.map((i) => [i.title, i.home])).toEqual([
      [null, true],
      ["Week 2 notes", false],
    ]);
  });

  it("has no time when the record has none", () => {
    expect(notReadModel(view({ written_at: null })).writtenAt).toBeNull();
  });

  it("knows when only what is never read is left, and when nothing is", () => {
    const never = notReadModel(
      view({
        not_readable: [
          entry("assignments", "by_rule", "Assignments", "assignments"),
          entry("people", "not_read", "People", "users"),
        ],
      }),
    );
    expect(onlyNever(never)).toBe(true);
    expect(needsAttention(never)).toBe(false);
    expect(hasDetails(never)).toBe(true);

    // With something more there are other things to say first.
    expect(onlyNever(notReadModel(view({ not_readable_more: 3 })))).toBe(false);

    const nothing = notReadModel(view());
    expect(hasDetails(nothing)).toBe(false);
    expect(onlyNever(nothing)).toBe(false);
  });

  it("takes a reason or an area it doesn't know for 'other', never for nothing", () => {
    const model = notReadModel(
      view({
        not_readable: [
          { area: "other", reason: "other", title: "From a newer version", url: null, count: 1 },
          // A file entry with a title that isn't one of the three groups.
          entry("files", "other", "Loose file link", "files/77"),
        ],
      }),
    );
    expect(model.other.map((i) => [i.title, i.kind])).toEqual([
      ["From a newer version", "item"],
      ["Loose file link", "file"],
    ]);
  });
});
