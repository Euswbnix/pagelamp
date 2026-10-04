import { act, configure, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CourseSyncSummary } from "@/api/types";
import i18n from "@/i18n";
import type { SourceProgress } from "@/stores/sync";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";
import { renderRoute, renderWithProviders } from "@/test/render";
import { SyncProgressRow } from "./SyncProgressRow";

configure({ asyncUtilTimeout: 3000 });

const HIDDEN_LISTS = { scenario: "canvas-hidden-lists" } as const;
const DEMO312_EN =
  "DEMO312: Pages and Files lists not shown in Canvas · Found through links: 2 pages and 3 files";
const DEMO312_ZH =
  "DEMO312：在 Canvas 中没有开放“页面”和“文件”列表 · 通过链接找到：2 个页面和 3 个文件";

function line(fields: Partial<CourseSyncSummary> & { course: string }): CourseSyncSummary {
  return { modules: 1, pages: 2, files: 2, events: 0, warnings: 0, ...fields };
}

/** A Canvas row whose run has ended well, with these lines. */
function finished(courses: CourseSyncSummary[]): SourceProgress {
  return {
    sourceId: "canvas:canvas.demo.test",
    label: "Demo Canvas",
    message: null,
    stage: null,
    course: null,
    current: null,
    total: null,
    warnings: [],
    result: { ok: true, error: null, errorKind: null },
    stopped: false,
    courses,
  };
}

function renderRow(courses: CourseSyncSummary[], courseLines?: boolean) {
  return renderWithProviders(
    <ul>
      <SyncProgressRow progress={finished(courses)} showFixLink={false} courseLines={courseLines} />
    </ul>,
  );
}

describe("a sync's line for each course", () => {
  it("says after a sync which lists a course doesn't show and what was found through links", async () => {
    const { user } = renderRoute("/sources", HIDDEN_LISTS);
    await user.click(await screen.findByRole("button", { name: "Sync Demo Canvas" }));
    const panel = await screen.findByRole("region", { name: "Sync finished" });

    const lines = within(panel).getByRole("list", { name: "Courses in Demo Canvas" });
    // Only the course with something to say: the others have only what is never read.
    expect(within(lines).getAllByRole("listitem")).toHaveLength(1);
    expect(lines).toHaveTextContent(DEMO312_EN);
    expect(within(panel).queryByText(/DEMO205/)).toBeNull();
    expect(within(panel).queryByText(/not read/i)).toBeNull();

    // It goes with the row.
    await user.click(within(panel).getByRole("button", { name: "Hide sync results" }));
    expect(screen.queryByRole("list", { name: "Courses in Demo Canvas" })).toBeNull();
    expect(screen.queryByText(/DEMO312/)).toBeNull();
  });

  it("says it in Chinese", async () => {
    useUiStore.setState({ locale: "zh-CN" });
    await i18n.changeLanguage("zh-CN");
    const { user } = renderRoute("/sources", HIDDEN_LISTS);
    await user.click(await screen.findByRole("button", { name: "同步 Demo Canvas" }));
    const lines = await screen.findByRole("list", { name: "Demo Canvas 的课程" });
    expect(lines).toHaveTextContent(DEMO312_ZH);
  });

  it("has lines under the Canvas source only when every source syncs", async () => {
    const { user } = renderRoute("/sources", HIDDEN_LISTS);
    await user.click(await screen.findByRole("button", { name: "Sync all" }));
    const panel = await screen.findByRole("region", { name: "Sync finished" });
    expect(within(panel).getByText("Course folder")).toBeInTheDocument();
    expect(within(panel).getByText("Course calendar")).toBeInTheDocument();
    // A folder's and a calendar's syncs say nothing of the kind.
    const lists = within(panel).getAllByRole("list", { name: /^Courses in/ });
    expect(lists).toHaveLength(1);
    expect(lists[0]).toHaveAccessibleName("Courses in Demo Canvas");
    // A list that isn't shown is no warning.
    const canvas = lists[0]?.closest("li");
    if (!(canvas instanceof HTMLElement)) throw new Error("no row for Demo Canvas");
    expect(within(canvas).queryByText(/warning/)).toBeNull();
  });

  it("takes the lines away when the source they belong to is removed", async () => {
    const { user } = renderRoute("/sources", HIDDEN_LISTS);
    await user.click(await screen.findByRole("button", { name: "Sync Demo Canvas" }));
    await screen.findByRole("list", { name: "Courses in Demo Canvas" });

    await user.click(screen.getByRole("button", { name: "Remove Demo Canvas" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Remove Demo Canvas?" });
    await user.click(within(dialog).getByRole("button", { name: "Remove source" }));
    expect(await screen.findByText("Removed Demo Canvas")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Courses in Demo Canvas" })).toBeNull();
    expect(screen.queryByText(/DEMO312/)).toBeNull();
  });

  it("says only what there is to say", () => {
    renderRow([
      line({ course: "DEMO201", files_hidden: true }),
      line({ course: "DEMO202", pages_hidden: true, linked_pages: 1 }),
      line({ course: "DEMO203", linked_files: 1 }),
      // What a light sync gives for every course: no line, and no conclusion from it.
      line({ course: "DEMO204", linked_pages: 0, linked_files: 0, not_read: 0 }),
      // How much wasn't read is not said on the row.
      line({ course: "DEMO205", not_read: 14, pages_hidden: false, files_hidden: false }),
      // A facade from before these fields.
      line({ course: "DEMO206" }),
    ]);
    const items = within(screen.getByRole("list", { name: "Courses in Demo Canvas" }))
      .getAllByRole("listitem")
      .map((item) => item.textContent);
    expect(items).toEqual([
      "DEMO201: Files list not shown in Canvas",
      "DEMO202: Pages list not shown in Canvas · Found through links: 1 page",
      "DEMO203: Found through links: 1 file",
    ]);
  });

  it("shows no list at all when no course has anything to say", () => {
    renderRow([line({ course: "DEMO204", not_read: 9 }), line({ course: "DEMO206" })]);
    expect(screen.getByText("Demo Canvas")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Courses in Demo Canvas" })).toBeNull();
  });

  it("shows a course's code or name as it is written, and two courses with one code as two", () => {
    renderRow([
      line({ course: "DEMO{{x}}101 $t(common:sync.failed)", pages_hidden: true }),
      line({ course: "DEMO300", files_hidden: true }),
      line({ course: "DEMO300", pages_hidden: true }),
    ]);
    const items = within(screen.getByRole("list", { name: "Courses in Demo Canvas" }))
      .getAllByRole("listitem")
      .map((item) => item.textContent);
    expect(items).toEqual([
      "DEMO{{x}}101 $t(common:sync.failed): Pages list not shown in Canvas",
      "DEMO300: Files list not shown in Canvas",
      "DEMO300: Pages list not shown in Canvas",
    ]);
  });

  it("leaves the lines out where there is no room for them", () => {
    renderRow([line({ course: "DEMO201", files_hidden: true })], false);
    expect(screen.getByText("Demo Canvas")).toBeInTheDocument();
    expect(screen.queryByText(/DEMO201/)).toBeNull();
  });

  it("keeps them out of the capsule's details", async () => {
    const { user } = renderRoute("/settings");
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    // The capsule shows a run it has seen running.
    act(() => {
      const store = useSyncStore.getState();
      store.begin(1, null);
      store.apply({ type: "source_started", source_id: "canvas", label: "Demo Canvas" });
    });
    act(() => {
      const store = useSyncStore.getState();
      store.apply({ type: "source_finished", source_id: "canvas", ok: true });
      store.finish(
        {
          started_at: "2026-10-05T13:00:00Z",
          finished_at: "2026-10-05T13:00:01Z",
          ok: true,
          results: [
            {
              source_id: "canvas",
              label: "Demo Canvas",
              kind: "canvas",
              ok: true,
              error: null,
              error_kind: null,
              started_at: "2026-10-05T13:00:00Z",
              finished_at: "2026-10-05T13:00:01Z",
              courses: 1,
              modules: 1,
              materials: 4,
              files_downloaded: 0,
              files_indexed: 0,
              events: 0,
              warnings: [],
              course_summaries: [line({ course: "DEMO201", files_hidden: true })],
            },
          ],
        },
        null,
      );
    });
    // The store has the line; the popover doesn't show it.
    expect(useSyncStore.getState().bySource.canvas?.courses).toHaveLength(1);
    await user.click(await screen.findByRole("button", { name: "Sync finished" }));
    const details = await screen.findByRole("dialog", { name: "Sync details" });
    expect(within(details).getByText("Demo Canvas")).toBeInTheDocument();
    expect(within(details).queryByText(/DEMO201/)).toBeNull();
  });
});
