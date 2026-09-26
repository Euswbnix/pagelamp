import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { SOURCE_FOLDER } from "@/api/mock/fixtures";
import { paths } from "@/lib/routes";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";
import { renderRoute } from "@/test/render";
import { DEMO099, DEMO101, DEMO205, openCourse } from "./testing";

describe("CourseDetailPage", () => {
  it("shows the header: code, name, policy, week and data freshness", async () => {
    await openCourse(DEMO101);

    expect(
      screen.getByRole("heading", { level: 1, name: "Intro to Demo Studies" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.getByText("DEMO101")).toBeInTheDocument();
    const freshness = screen.getByText(/^Data from Course folder · synced/);
    expect(within(freshness).getByText("2 hours ago")).toBeInTheDocument();

    const status = screen.getByRole("list", { name: "Course status" });
    expect(within(status).getByText("Learning aid only")).toBeInTheDocument();
    expect(within(status).getByText("Week 4")).toBeInTheDocument();
    expect(within(status).queryByText("Hidden")).not.toBeInTheDocument();

    expect(screen.getByRole("link", { name: "All courses" })).toHaveAttribute(
      "href",
      paths.courses,
    );
    // DEMO101 has no course website.
    expect(screen.queryByRole("link", { name: "Open course website" })).not.toBeInTheDocument();
  });

  it("opens the course website in the browser when the course has an http(s) URL", async () => {
    const { user, api } = await openCourse(DEMO205);
    const openExternal = vi.spyOn(api, "openExternal");

    const link = screen.getByRole("link", { name: "Open course website" });
    expect(link).toHaveAttribute("href", "https://canvas.demo.test/courses/205");
    await user.click(link);
    expect(openExternal).toHaveBeenCalledWith("https://canvas.demo.test/courses/205");
  });

  it("shows 'Syncing now…' while this course's source is syncing", async () => {
    await openCourse(DEMO101);
    useSyncStore.setState({
      running: true,
      order: [SOURCE_FOLDER],
      bySource: {
        [SOURCE_FOLDER]: {
          sourceId: SOURCE_FOLDER,
          label: "Course folder",
          message: null,
          current: null,
          total: null,
          warnings: [],
          result: null,
          stopped: false,
        },
      },
    });
    expect(await screen.findByText("Syncing now…")).toBeInTheDocument();
  });

  it("warns when the course's Canvas token has expired", async () => {
    await openCourse(DEMO205, { scenario: "expired" });
    expect(await screen.findByText("Your access token has expired")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Sources & sync" })).toHaveAttribute(
      "href",
      paths.sources,
    );
  });

  it("warns when the course's source failed for another reason", async () => {
    await openCourse(DEMO101, { scenario: "error" });
    expect(
      await screen.findByText("This course's source didn't sync last time: Not found"),
    ).toBeInTheDocument();
  });

  it("marks a hidden course and can show it in the course list again", async () => {
    const { user, api } = await openCourse(DEMO099);
    const setHidden = vi.spyOn(api, "setCourseHidden");
    const status = screen.getByRole("list", { name: "Course status" });
    expect(within(status).getByText("Hidden")).toBeInTheDocument();

    await user.click(within(status).getByRole("button", { name: "Show in course list" }));

    expect(setHidden).toHaveBeenCalledWith(DEMO099, false);
    expect(
      await screen.findByText("Orientation Placeholder is back in your course list."),
    ).toBeInTheDocument();
    expect(within(status).queryByText("Hidden")).not.toBeInTheDocument();
  });

  it("shows 'Course not found' for an unknown course id", async () => {
    renderRoute(paths.course("folder:demo-courses/course/NOPE404"));
    expect(
      await screen.findByRole("heading", { level: 1, name: "Course not found" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to courses" })).toHaveAttribute(
      "href",
      paths.courses,
    );
  });

  it("shows an error with a working retry when the course fails to load", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0 });
    vi.spyOn(api, "courseOverview").mockRejectedValueOnce(
      new ApiError("internal", "The database is locked."),
    );
    const { user } = renderRoute(paths.course(DEMO101), { api });

    expect(await screen.findByText("Something went wrong")).toBeInTheDocument();
    expect(screen.getByText("The database is locked.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "Intro to Demo Studies" }),
    ).toBeInTheDocument();
  });

  it("opens the tab named in ?tab= and keeps the selected tab in the URL", async () => {
    const { user, router } = await openCourse(DEMO205, { query: "tab=policy" });
    expect(screen.getByRole("tab", { name: "AI policy" })).toHaveAttribute("aria-selected", "true");
    expect(
      screen.getByRole("radiogroup", { name: "How can you use AI in this course?" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Deadlines" }));
    expect(screen.getByRole("tab", { name: "Deadlines" })).toHaveAttribute("aria-selected", "true");
    expect(router.state.location.search).toBe("?tab=deadlines");
  });

  it("falls back to the 'This week' tab for an unknown ?tab= value", async () => {
    await openCourse(DEMO101, { query: "tab=nonsense" });
    expect(screen.getByRole("tab", { name: "This week" })).toHaveAttribute("aria-selected", "true");
  });

  it("renders in Simplified Chinese", async () => {
    useUiStore.setState({ locale: "zh-CN" });
    renderRoute(paths.course(DEMO101));
    expect(await screen.findByRole("tab", { name: "本周" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "AI 使用规定" })).toBeInTheDocument();
    expect(screen.getByText(/^数据来源：Course folder/)).toBeInTheDocument();
  });

  it("shows 'not found' instead of stale data once the course is gone", async () => {
    const { api, queryClient } = await openCourse(DEMO101);
    expect(
      screen.getByRole("heading", { level: 1, name: "Intro to Demo Studies" }),
    ).toBeInTheDocument();

    // Its source is removed (e.g. from another window); the next refresh finds nothing.
    await api.removeSource(SOURCE_FOLDER);
    await queryClient.invalidateQueries();

    expect(
      await screen.findByRole("heading", { level: 1, name: "Course not found" }),
    ).toBeInTheDocument();
  });

  it("marks a past course in the header", async () => {
    await openCourse(DEMO099);
    expect(screen.getByRole("list", { name: "Course status" })).toHaveTextContent("Past course");
  });
});
