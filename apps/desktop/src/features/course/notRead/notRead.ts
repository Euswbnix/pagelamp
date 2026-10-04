import type { CoverageView, NotReadable } from "@/api/types";
import { listsNotShown } from "@/lib/canvasLists";

/** One thing the card names: its title as Canvas gives it (plain text), and its address. */
export interface NotReadItem {
  /** Null: Canvas gave no title for it. */
  title: string | null;
  url: string | null;
  /** What kind of thing it is, for a name when it has neither title nor address. */
  kind: "page" | "file" | "item";
  /** The course's Home page: named by a string of ours. */
  home: boolean;
}

/**
 * What the card on the course page says, worked out from `CourseOverview.coverage`. The view
 * gives at most 20 entries (`more` counts the rest, of unknown kinds), so nothing here is a
 * count by reason except the three file groups, which the facade counts from the materials.
 */
export interface NotReadModel {
  /** When the course was last read in full (a full sync, or a download of its files). */
  writtenAt: string | null;
  home: "read" | "locked" | null;
  lists: "both" | "pages" | "files" | null;
  linkedPages: number;
  linkedFiles: number;
  /** Files the student can download and hasn't. */
  toDownload: number;
  /** Pages a module asks the student to view first. */
  mustView: NotReadItem[];
  failed: {
    /** A list that was asked for and not read completely. */
    lists: ("pagesList" | "filesList")[];
    /** No page was read: which ones a module asks to be viewed couldn't be checked. */
    pagesAll: boolean;
    items: NotReadItem[];
  };
  lockedPages: NotReadItem[];
  lockedFiles: number;
  tooLarge: number;
  pastLimit: NotReadItem[];
  /** Read, but with more links in them than PageLamp looks at: the rest were left. */
  cutShort: NotReadItem[];
  gone: NotReadItem[];
  never: {
    assignments: boolean;
    parts: boolean;
    menu: boolean;
    tools: NotReadItem[];
    otherCourses: NotReadItem[];
  };
  other: NotReadItem[];
  more: number;
}

const PARTS: ReadonlySet<NotReadable["area"]> = new Set(["grades", "people", "discussions"]);

/** A title as the facade writes it into an entry: one space between words, 120 characters. */
function asEntryTitle(title: string): string {
  const words = title.trim().split(/\s+/).join(" ");
  const chars = [...words];
  return chars.length > 120 ? `${chars.slice(0, 120).join("")}…` : words;
}

/** An entry that says nothing but its area and reason. */
const bare = (entry: NotReadable) => !entry.title && !entry.url;

function item(entry: NotReadable): NotReadItem {
  return {
    title: entry.title?.trim() ? entry.title : null,
    url: entry.url ?? null,
    kind: entry.area === "files" ? "file" : entry.area === "pages" ? "page" : "item",
    home: entry.area === "home",
  };
}

export function notReadModel(coverage: CoverageView): NotReadModel {
  const model: NotReadModel = {
    writtenAt: coverage.written_at ?? null,
    home:
      coverage.home_state === "locked"
        ? "locked"
        : coverage.home_state === "read" && coverage.home_kind === "page"
          ? "read"
          : null,
    lists: listsNotShown(coverage.pages_list === "hidden", coverage.files_list === "hidden"),
    linkedPages: coverage.counts.linked_pages ?? 0,
    linkedFiles: coverage.counts.linked_files ?? 0,
    toDownload: 0,
    mustView: [],
    failed: {
      lists: [
        ...(coverage.pages_list === "failed" ? (["pagesList"] as const) : []),
        ...(coverage.files_list === "failed" ? (["filesList"] as const) : []),
      ],
      pagesAll: false,
      items: [],
    },
    lockedPages: [],
    lockedFiles: 0,
    tooLarge: 0,
    pastLimit: [],
    cutShort: [],
    gone: [],
    never: { assignments: false, parts: false, menu: false, tools: [], otherCourses: [] },
    other: [],
    more: coverage.not_readable_more,
  };
  // The Home page's title as an entry about it would carry it.
  const homeTitle = coverage.home ? asEntryTitle(coverage.home.title) : null;
  const isHome = (entry: NotReadable) =>
    entry.area === "home" ||
    (entry.area === "pages" && homeTitle !== null && entry.title === homeTitle);
  // The locked Home page comes as a state and as an entry: it is said once, by the state.
  let lockedHomeSaid = model.home !== "locked";
  // The course's menu couldn't be read: then no list was asked for. The facade says that with
  // both lists "failed" and a bare entry for each; the one for the files comes from nothing
  // else, so it tells this case from "no page was read" (a bare entry for the pages alone).
  const menuFailed = coverage.not_readable.some(
    (e) => e.area === "files" && e.reason === "failed_this_sync" && bare(e),
  );

  for (const entry of coverage.not_readable) {
    const { area, reason } = entry;
    if (area === "files" && bare(entry)) {
      // The three groups of files that aren't downloaded, counted from the materials.
      if (reason === "needs_download") model.toDownload += entry.count;
      else if (reason === "too_large") model.tooLarge += entry.count;
      else if (reason === "locked") model.lockedFiles += entry.count;
      // A list that isn't shown or wasn't read is said by its state.
      else if (reason !== "index_hidden" && reason !== "failed_this_sync") {
        model.other.push(item(entry));
      }
      continue;
    }
    switch (reason) {
      case "index_hidden":
        // Said by `lists`.
        break;
      case "would_mark_viewed":
        model.mustView.push({ ...item(entry), home: isHome(entry) });
        break;
      case "failed_this_sync":
        if (area === "pages" && bare(entry)) {
          if (!menuFailed) model.failed.pagesAll = true;
        } else {
          model.failed.items.push({ ...item(entry), home: isHome(entry) });
        }
        break;
      case "locked":
        if (!lockedHomeSaid && isHome(entry)) lockedHomeSaid = true;
        else if (area === "files") model.lockedFiles += entry.count;
        else model.lockedPages.push(item(entry));
        break;
      case "too_large":
        model.tooLarge += entry.count;
        break;
      case "needs_download":
        model.toDownload += entry.count;
        break;
      case "capped":
        // A link that was left alone has only its address. With a title (or for the syllabus
        // and an announcement) it is a text that was read and held more links than are looked at.
        if (entry.title || area === "syllabus" || area === "announcements") {
          model.cutShort.push(item(entry));
        } else {
          model.pastLimit.push(item(entry));
        }
        break;
      case "no_longer_in_canvas":
        model.gone.push(item(entry));
        break;
      case "by_rule":
        model.never.assignments = true;
        break;
      case "outside_canvas":
        model.never.tools.push(item(entry));
        break;
      case "other_course":
        model.never.otherCourses.push(item(entry));
        break;
      case "not_read":
        if (PARTS.has(area)) model.never.parts = true;
        // An item of the course's menu has its name; a link in a text has only its address.
        else if (entry.title) model.never.menu = true;
        else model.other.push(item(entry));
        break;
      default:
        model.other.push(item(entry));
    }
  }
  // The Home page failed, and no entry says so (the facade gives none in one case).
  if (coverage.home_state === "failed" && !model.failed.items.some((i) => i.home)) {
    model.failed.items.unshift({ title: null, url: null, kind: "item", home: true });
  }
  // "Read" is what an earlier sync did when the page is listed as waiting or failed now.
  if (model.home === "read" && [...model.mustView, ...model.failed.items].some((i) => i.home)) {
    model.home = null;
  }
  return model;
}

/** Whether there is something the student can act on, or something that went wrong. */
export function needsAttention(model: NotReadModel): boolean {
  return (
    model.mustView.length > 0 ||
    model.failed.lists.length > 0 ||
    model.failed.pagesAll ||
    model.failed.items.length > 0
  );
}

/** Whether the details hold anything at all. */
export function hasDetails(model: NotReadModel): boolean {
  return (
    needsAttention(model) ||
    model.lockedPages.length > 0 ||
    model.lockedFiles > 0 ||
    model.tooLarge > 0 ||
    model.pastLimit.length > 0 ||
    model.cutShort.length > 0 ||
    model.gone.length > 0 ||
    hasNever(model) ||
    model.other.length > 0 ||
    model.more > 0
  );
}

export function hasNever(model: NotReadModel): boolean {
  const { never } = model;
  return (
    never.assignments ||
    never.parts ||
    never.menu ||
    never.tools.length > 0 ||
    never.otherCourses.length > 0
  );
}

/** Only what is never read by rule: nothing wrong, nothing to do, nothing left out. */
export function onlyNever(model: NotReadModel): boolean {
  return (
    hasNever(model) &&
    !needsAttention(model) &&
    model.lists === null &&
    model.home !== "locked" &&
    model.toDownload === 0 &&
    model.lockedPages.length === 0 &&
    model.lockedFiles === 0 &&
    model.tooLarge === 0 &&
    model.pastLimit.length === 0 &&
    model.cutShort.length === 0 &&
    model.gone.length === 0 &&
    model.other.length === 0 &&
    model.more === 0
  );
}
