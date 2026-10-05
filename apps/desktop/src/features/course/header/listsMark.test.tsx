import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createMockApi } from "@/api/mock";
import i18n from "@/i18n";
import { paths } from "@/lib/routes";
import { useUiStore } from "@/stores/ui";
import { renderRoute } from "@/test/render";
import { DEMO101, DEMO205, DEMO312, openCourse } from "../testing";

const HIDDEN_LISTS = { scenario: "canvas-hidden-lists" } as const;
const status = () => screen.getByRole("list", { name: "Course status" });

describe("the course header's mark for lists a course doesn't show", () => {
  it("says which lists the course doesn't show in Canvas, on every visit", async () => {
    await openCourse(DEMO312, HIDDEN_LISTS);
    expect(
      within(status()).getByText("Pages and Files lists not shown in Canvas"),
    ).toBeInTheDocument();
  });

  it("says it in Chinese", async () => {
    useUiStore.setState({ locale: "zh-CN" });
    await i18n.changeLanguage("zh-CN");
    renderRoute(paths.course(DEMO312), HIDDEN_LISTS);
    expect(await screen.findByText("在 Canvas 中没有开放“页面”和“文件”列表")).toBeInTheDocument();
  });

  it("names one list when only one isn't shown, and is there on every tab", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 0, ...HIDDEN_LISTS });
    const overview = api.courseOverview.bind(api);
    api.courseOverview = async (id) => {
      const real = await overview(id);
      return real.coverage ? { ...real, coverage: { ...real.coverage, pages_list: "read" } } : real;
    };
    await openCourse(DEMO312, { api, query: "tab=policy" });
    expect(within(status()).getByText("Files list not shown in Canvas")).toBeInTheDocument();
  });

  it("says nothing for a course that shows its lists, or one without a record", async () => {
    const first = await openCourse(DEMO205, HIDDEN_LISTS);
    expect(within(status()).queryByText(/not shown in Canvas/)).toBeNull();
    first.unmount();

    // The demo: no full sync has recorded anything yet.
    const second = await openCourse(DEMO205);
    expect(within(status()).queryByText(/not shown in Canvas/)).toBeNull();
    second.unmount();

    await openCourse(DEMO101, HIDDEN_LISTS);
    expect(within(status()).queryByText(/not shown in Canvas/)).toBeNull();
  });
});
