import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import type { CoverageView } from "@/api/types";
import i18n from "@/i18n";
import { paths } from "@/lib/routes";
import { useUiStore } from "@/stores/ui";
import { renderRoute } from "@/test/render";
import { DEMO101, DEMO205, DEMO312, openCourse } from "../testing";

const HIDDEN_LISTS = { scenario: "canvas-hidden-lists" } as const;
const TITLE = "What PageLamp didn't read";
const card = () => screen.getByRole("region", { name: TITLE });
const trigger = () => screen.getByRole("button", { name: TITLE });

/** The mock in the scenario, with this course's coverage changed by `change`. */
function apiWith(courseId: string, change: (coverage: CoverageView) => CoverageView | null) {
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, ...HIDDEN_LISTS });
  const overview = api.courseOverview.bind(api);
  api.courseOverview = async (id) => {
    const real = await overview(id);
    return id === courseId && real.coverage ? { ...real, coverage: change(real.coverage) } : real;
  };
  return api;
}

describe("What PageLamp didn't read, on the course page", () => {
  it("says what it read and found, and opens with what waits for the student", async () => {
    const { api } = await openCourse(DEMO312, HIDDEN_LISTS);
    const section = card();
    expect(within(section).getByText(/^PageLamp last read this course in full/)).toBeVisible();
    expect(within(section).getByText("PageLamp read this course's Home page.")).toBeVisible();
    expect(
      within(section).getByText(
        "This course doesn't show its Pages and Files lists in Canvas. PageLamp found the pages and files that its modules and links point to; it can't see any others.",
      ),
    ).toBeVisible();
    expect(within(section).getByText("Found through links: 2 pages and 3 files")).toBeVisible();
    expect(
      within(section).getByText("2 files in this course aren't downloaded yet."),
    ).toBeVisible();
    expect(
      within(section).getByText(
        "“Download files…” at the top of the page downloads all of this course's files.",
      ),
    ).toBeVisible();
    // The time is the record's own, not the source's last sync.
    const overview = await api.courseOverview(DEMO312);
    expect(
      within(section)
        .getByText(/ago|now/)
        .closest("time"),
    ).toHaveAttribute("datetime", overview.coverage?.written_at);

    // A page a module asks the student to view: the details are open.
    expect(trigger()).toHaveAttribute("aria-expanded", "true");
    const waiting = within(section).getByRole("region", { name: "Waiting for you" });
    expect(within(waiting).getByRole("link", { name: "Lab safety briefing" })).toBeVisible();
    const cant = within(section).getByRole("region", { name: "PageLamp can't read these" });
    expect(within(cant).getByText("1 file is too large to download.")).toBeVisible();
    // A link past the limits has no title: its address is what there is.
    expect(
      within(cant).getByText(
        "PageLamp opens a limited number of links in each course and doesn't follow links from a linked page. These are beyond that. Some are reached by a later sync; most are not.",
      ),
    ).toBeVisible();
    expect(
      within(cant).getByRole("link", {
        name: "https://canvas.demo.test/courses/312/pages/older-notes",
      }),
    ).toBeVisible();
    const never = within(section).getByRole("region", { name: "PageLamp never reads these" });
    expect(
      within(never).getByText(
        "Assignments and quizzes: PageLamp keeps only their title, due date and link.",
      ),
    ).toBeVisible();
    expect(
      within(never).getByText("Grades, People and Discussions: PageLamp doesn't read these."),
    ).toBeVisible();
    expect(within(never).getByRole("link", { name: "Demo Reader" })).toBeVisible();
    // What is never read by rule is said, not listed.
    expect(within(never).queryByText("Assignments")).toBeNull();
    // A list that isn't shown is said once, above: its entries are named nowhere else.
    expect(within(section).getAllByText(/doesn't show its Pages and Files lists/)).toHaveLength(1);
    expect(
      within(section).queryByRole("region", { name: "Other things PageLamp didn't read" }),
    ).toBeNull();
    expect(within(section).queryByText("A page")).toBeNull();
    expect(within(section).queryByText("A file")).toBeNull();
  });

  it("stays closed when nothing waits for the student, and opens on request", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, ...HIDDEN_LISTS });
    const started = [
      vi.spyOn(api, "syncAll"),
      vi.spyOn(api, "syncSource"),
      vi.spyOn(api, "downloadCourseFiles"),
      vi.spyOn(api, "downloadMaterialFiles"),
    ];
    const { user } = await openCourse(DEMO205, { api });
    const section = card();
    expect(within(section).getByText("1 file in this course isn't downloaded yet.")).toBeVisible();
    expect(trigger()).toHaveAttribute("aria-expanded", "false");
    expect(
      within(section).queryByRole("region", { name: "PageLamp never reads these" }),
    ).toBeNull();

    await user.click(trigger());
    expect(trigger()).toHaveAttribute("aria-expanded", "true");
    expect(
      within(section).getByRole("region", { name: "PageLamp never reads these" }),
    ).toBeVisible();
    expect(within(section).getByText("1 file is too large to download.")).toBeVisible();
    // Reading the card starts nothing.
    for (const spy of started) expect(spy).not.toHaveBeenCalled();
  });

  it("opens once when the sync row sends the student here", async () => {
    const { user, router } = renderRoute(paths.courseNotRead(DEMO205), HIDDEN_LISTS);
    await screen.findByRole("tablist", { name: "Course sections" });
    const heading = await screen.findByRole("button", { name: TITLE });
    await waitFor(() => expect(heading).toHaveAttribute("aria-expanded", "true"));
    // The request is spent: the address no longer asks for it, and the heading has the focus.
    await waitFor(() => expect(router.state.location.search).not.toContain("show="));
    expect(heading).toHaveFocus();

    // Closed by the student, it stays closed when they come back to this tab.
    await user.click(heading);
    expect(heading).toHaveAttribute("aria-expanded", "false");
    await user.click(screen.getByRole("tab", { name: "Deadlines" }));
    await user.click(screen.getByRole("tab", { name: "This week" }));
    expect(await screen.findByRole("button", { name: TITLE })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("opens by itself when a later record says something waits for the student", async () => {
    // First what an earlier sync recorded (nothing to do), then what the next one found.
    let waiting = false;
    const api = apiWith(DEMO312, (coverage) => ({
      ...coverage,
      not_readable: coverage.not_readable.filter(
        (n) => waiting || n.reason !== "would_mark_viewed",
      ),
    }));
    const { queryClient } = await openCourse(DEMO312, { api });
    expect(trigger()).toHaveAttribute("aria-expanded", "false");
    waiting = true;
    await act(() => queryClient.invalidateQueries());
    await waitFor(() => expect(trigger()).toHaveAttribute("aria-expanded", "true"));
    expect(within(card()).getByRole("region", { name: "Waiting for you" })).toBeVisible();
  });

  it("opens Canvas for a title that is a web address, and nothing else", async () => {
    const api = apiWith(DEMO312, (coverage) => ({
      ...coverage,
      not_readable: [
        {
          area: "pages",
          reason: "would_mark_viewed",
          title: "Lab safety briefing",
          url: "https://canvas.demo.test/courses/312/pages/lab-safety-briefing",
          count: 1,
        },
        // An address that isn't a web address is never a link.
        {
          area: "pages",
          reason: "would_mark_viewed",
          title: "Odd address",
          url: "javascript:alert(1)",
          count: 1,
        },
        // A link in a text that could open a page a module asks to be viewed: no title.
        {
          area: "pages",
          reason: "would_mark_viewed",
          title: null,
          url: "https://canvas.demo.test/courses/312/pages/week-3-notes",
          count: 1,
        },
        // Canvas no longer has it: its address is gone too.
        {
          area: "files",
          reason: "no_longer_in_canvas",
          title: "Old reading list {{product}}",
          url: "https://canvas.demo.test/courses/312/files/9",
          count: 1,
        },
      ],
    }));
    const open = vi.spyOn(api, "openExternal");
    const { user } = await openCourse(DEMO312, { api });
    const section = card();
    expect(within(section).getByText("Odd address").closest("a")).toBeNull();
    const waiting = within(section).getByRole("region", { name: "Waiting for you" });
    expect(
      within(waiting).getByRole("link", { name: "A link that could open such a page" }),
    ).toBeVisible();
    expect(within(waiting).queryByText(/week-3-notes/)).toBeNull();
    const gone = within(section).getByRole("region", { name: "No longer in Canvas" });
    // Shown as written, and not a link.
    expect(within(gone).getByText("Old reading list {{product}}").closest("a")).toBeNull();
    expect(within(gone).getByText("PageLamp keeps what it already has of them.")).toBeVisible();
    expect(open).not.toHaveBeenCalled();

    await user.click(within(section).getByRole("link", { name: "Lab safety briefing" }));
    expect(open).toHaveBeenCalledTimes(1);
    expect(open).toHaveBeenCalledWith(
      "https://canvas.demo.test/courses/312/pages/lab-safety-briefing",
    );
  });

  it("names what failed and says it is tried again", async () => {
    const api = apiWith(DEMO312, (coverage) => ({
      ...coverage,
      files_list: "read",
      home_state: "failed",
      not_readable: [
        { area: "home", reason: "failed_this_sync", title: null, url: null, count: 1 },
        // The module list couldn't be read, so no page was.
        { area: "modules", reason: "failed_this_sync", title: null, url: null, count: 1 },
      ],
      not_readable_more: 1,
    }));
    await openCourse(DEMO312, { api });
    const failed = within(card()).getByRole("region", { name: "Couldn't be read last time" });
    const items = within(failed)
      .getAllByRole("listitem")
      .map((item) => item.textContent);
    expect(items).toEqual([
      "The module list",
      "This course's pages: PageLamp couldn't check which ones a module asks you to view, so it read none of them this time.",
      "The course's Home page",
    ]);
    expect(within(failed).getByText("They're tried again on the next full sync.")).toBeVisible();
    expect(within(card()).getByText("…and 1 more that isn't listed here.")).toBeVisible();
    // The Home page wasn't read this time.
    expect(within(card()).queryByText("PageLamp read this course's Home page.")).toBeNull();
  });

  it("says only that the lists weren't read when the course's menu couldn't be read", async () => {
    const api = apiWith(DEMO312, (coverage) => ({
      ...coverage,
      pages_list: "failed",
      files_list: "failed",
      not_readable: [
        { area: "pages", reason: "failed_this_sync", title: null, url: null, count: 1 },
        { area: "files", reason: "failed_this_sync", title: null, url: null, count: 1 },
      ],
    }));
    await openCourse(DEMO312, { api });
    const failed = within(card()).getByRole("region", { name: "Couldn't be read last time" });
    expect(
      within(failed)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["The Pages list", "The Files list"]);
    // The module pages were read: nothing says they weren't, and nothing is named twice.
    expect(within(card()).queryByText(/read none of them/)).toBeNull();
    expect(
      within(card()).queryByRole("region", { name: "Other things PageLamp didn't read" }),
    ).toBeNull();
  });

  it("tells a locked Home page, texts cut short, other links and other courses apart", async () => {
    const base = "https://canvas.demo.test/courses/312";
    const api = apiWith(DEMO312, (coverage) => ({
      ...coverage,
      home_state: "locked",
      home: { id: "home", title: "Welcome to DEMO312" },
      written_at: null,
      not_readable: [
        {
          area: "pages",
          reason: "locked",
          title: "Welcome to DEMO312",
          url: `${base}/pages/welcome`,
          count: 1,
        },
        {
          area: "pages",
          reason: "locked",
          title: "Midterm review",
          url: `${base}/pages/midterm`,
          count: 1,
        },
        {
          area: "pages",
          reason: "capped",
          title: "Course schedule",
          url: `${base}/pages/schedule`,
          count: 1,
        },
        {
          area: "other",
          reason: "not_read",
          title: "Course evaluations",
          url: `${base}/evaluations`,
          count: 1,
        },
        {
          area: "other",
          reason: "not_read",
          title: null,
          url: "https://canvas.demo.test/calendar",
          count: 1,
        },
        {
          area: "other",
          reason: "other_course",
          title: null,
          url: "https://canvas.demo.test/courses/313/pages/intro",
          count: 1,
        },
      ],
    }));
    const { user } = await openCourse(DEMO312, { api });
    const section = card();
    await user.click(trigger());
    expect(
      within(section).getByText(
        "This course's Home page is locked in Canvas, so PageLamp has only its title.",
      ),
    ).toBeVisible();
    // No record time, no sentence about it.
    expect(within(section).queryByText(/last read this course in full/)).toBeNull();
    const cant = within(section).getByRole("region", { name: "PageLamp can't read these" });
    expect(
      within(cant).getByText("Locked in Canvas. PageLamp has only their titles."),
    ).toBeVisible();
    // The Home page was said above: only the other locked page is listed.
    expect(within(cant).getByRole("link", { name: "Midterm review" })).toBeVisible();
    expect(within(cant).queryByText("Welcome to DEMO312")).toBeNull();
    // A page that was read, with more links in it than are looked at: not "beyond the limit".
    expect(
      within(cant).getByText(
        "PageLamp read these, but they hold more links than it looks at in one text; it left the rest of them:",
      ),
    ).toBeVisible();
    expect(within(cant).getByRole("link", { name: "Course schedule" })).toBeVisible();
    expect(within(cant).queryByText(/These are beyond that/)).toBeNull();
    const never = within(section).getByRole("region", { name: "PageLamp never reads these" });
    expect(
      within(never).getByText("Other items in the course menu: PageLamp doesn't read them."),
    ).toBeVisible();
    expect(
      within(never).getByRole("link", { name: "https://canvas.demo.test/courses/313/pages/intro" }),
    ).toBeVisible();
    // A link in a text to somewhere else in Canvas is no menu item: it is named by its address.
    const other = within(section).getByRole("region", {
      name: "Other things PageLamp didn't read",
    });
    expect(
      within(other).getByRole("link", { name: "https://canvas.demo.test/calendar" }),
    ).toBeVisible();
  });

  it("says so when only what is never read is left", async () => {
    const api = apiWith(DEMO312, (coverage) => ({
      ...coverage,
      home_kind: "modules",
      home_state: "not_a_page",
      pages_list: "read",
      files_list: "read",
      counts: {},
      not_readable: coverage.not_readable.filter(
        (n) => n.reason === "by_rule" || n.reason === "not_read",
      ),
    }));
    await openCourse(DEMO312, { api });
    expect(
      within(card()).getByText(
        "Only the parts PageLamp never reads, such as assignments, quizzes and grades.",
      ),
    ).toBeVisible();
    expect(trigger()).toHaveAttribute("aria-expanded", "false");
  });

  it("isn't there without a record, for a folder course, or with nothing to say", async () => {
    // The demo: no full sync by this version has recorded anything.
    const demo = await openCourse(DEMO205);
    expect(screen.queryByRole("region", { name: TITLE })).toBeNull();
    demo.unmount();

    const folder = await openCourse(DEMO101, HIDDEN_LISTS);
    expect(screen.queryByRole("region", { name: TITLE })).toBeNull();
    folder.unmount();

    // A record with nothing in it is no claim that everything was read.
    const api = apiWith(DEMO312, (coverage) => ({
      ...coverage,
      home_kind: "modules",
      home_state: "not_a_page",
      pages_list: "read",
      files_list: "read",
      counts: {},
      not_readable: [],
    }));
    await openCourse(DEMO312, { api });
    expect(screen.queryByRole("region", { name: TITLE })).toBeNull();
  });

  it("says it in Chinese", async () => {
    useUiStore.setState({ locale: "zh-CN" });
    await i18n.changeLanguage("zh-CN");
    renderRoute(paths.course(DEMO312), HIDDEN_LISTS);
    const section = await screen.findByRole("region", { name: "PageLamp 没有读取的部分" });
    expect(within(section).getByText("PageLamp 读取了这门课的主页。")).toBeVisible();
    expect(
      within(section).getByText(
        "这门课在 Canvas 中没有显示“页面”和“文件”列表。PageLamp 找到了模块和链接指向的页面和文件，其余的它看不到。",
      ),
    ).toBeVisible();
    expect(within(section).getByText("通过链接找到：2 个页面和 3 个文件")).toBeVisible();
    expect(within(section).getByText("这门课有 2 个文件还没下载。")).toBeVisible();
    expect(within(section).getByRole("region", { name: "等你处理的" })).toBeVisible();
    expect(within(section).getByText("有 1 个文件太大，无法下载。")).toBeVisible();
    expect(within(section).getByText("成绩、人员和讨论：PageLamp 不读取。")).toBeVisible();
  });
});
