import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createMockApi } from "@/api/mock";
import { paths } from "@/lib/routes";
import { renderRoute } from "@/test/render";

const READABLE = "canvas:canvas.demo.test/course/205"; // "proposals": week 4 has one material
const WITHHELD = "folder:demo-courses/course/DEMO310"; // "demo": the AI policy withholds it

function mockApi(options: Parameters<typeof createMockApi>[0] = {}) {
  return createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "proposals", ...options });
}

async function explainButton() {
  const button = await screen.findByRole("button", { name: "Explain week 4" });
  await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
  return button;
}

describe("Course → Explain", () => {
  it("explains the week with cited paragraphs, check questions and the AI-generated line", async () => {
    const api = mockApi();
    const explain = vi.spyOn(api, "explainWeek");
    const { user } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    await user.click(await explainButton());

    expect(await screen.findByText("The explanation is ready.")).toHaveAttribute("role", "status");
    expect(screen.getByRole("region", { name: "Explanation of week 4" })).toHaveFocus();
    expect(explain).toHaveBeenCalledWith(
      READABLE,
      4,
      expect.any(String),
      expect.objectContaining({ include: [], override_budget: false, ui_language: "en" }),
      expect.any(Function),
    );
    const article = screen.getByRole("article");
    expect(within(article).getByText(/^AI-generated · /)).toBeInTheDocument();
    const sources = within(article).getAllByRole("list", { name: "Sources" });
    expect(sources.length).toBeGreaterThan(0);
    expect(
      within(article).getByRole("heading", { name: "Check your understanding" }),
    ).toBeInTheDocument();
    // The course's first cloud run: the question (b) reminder.
    expect(screen.getByText(/Check whether your instructor allows sharing/)).toBeInTheDocument();
  });

  it("opens a citation's file on this computer", async () => {
    const api = mockApi();
    const open = vi.spyOn(api, "openMaterial");
    const { user } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    await user.click(await explainButton());
    const [chip] = within(await screen.findByRole("article")).getAllByRole("button", {
      name: /, p\. \d$/,
    });
    if (!chip) throw new Error("no citation");
    await user.click(chip);
    expect(open).toHaveBeenCalledWith(expect.any(String));
  });

  it("stops a run, and nothing is saved", async () => {
    const api = mockApi({ syncStepMs: 200 });
    const { user } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    await user.click(await explainButton());
    const stop = await screen.findByRole("button", { name: "Stop" });
    expect(stop).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(await screen.findByText("Stopped. Nothing was saved.")).toBeInTheDocument();
    expect(await api.savedExplanations(READABLE, 4)).toEqual([]);
  });

  it("says why a course's weeks can't be explained, and leads to its AI policy", async () => {
    const { user } = renderRoute(`${paths.course(WITHHELD)}?tab=explain`, { scenario: "demo" });
    expect(
      await screen.findByText(/This course's AI policy doesn't allow AI to read its materials/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Explain week/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Open the AI policy tab" }));
    expect(
      await screen.findByRole("tab", { name: "AI policy", selected: true }),
    ).toBeInTheDocument();
  });
});

describe("Settings → Language of AI explanations", () => {
  it("switches to the course materials' language", async () => {
    const api = mockApi();
    const { user } = renderRoute("/settings", { api });
    const heading = await screen.findByRole("heading", {
      level: 2,
      name: "Language of AI explanations",
    });
    const section = heading.closest("section") as HTMLElement;
    const course = await within(section).findByRole("radio", {
      name: "The course materials' language",
    });
    await user.click(course);
    await waitFor(async () => expect(await api.aiOutputLanguage()).toBe("course"));
  });
});
