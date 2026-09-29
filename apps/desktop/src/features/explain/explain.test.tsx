import { act, screen, waitFor, within } from "@testing-library/react";
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

  it("deletes an explanation after asking", async () => {
    const api = mockApi();
    const { user } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    await user.click(await explainButton());
    await user.click(await screen.findByRole("button", { name: "Delete…" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Delete this explanation?" });
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("Explanation deleted.")).toBeInTheDocument();
    expect(await api.savedExplanations(READABLE, 4)).toEqual([]);
    await waitFor(() => expect(screen.queryByRole("article")).toBeNull());
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
    // The button went with its tab: the Policy panel takes the focus.
    await waitFor(() => expect(screen.getByRole("tabpanel", { name: "AI policy" })).toHaveFocus());
  });

  it("keeps a run going on another tab, and stops it when the course is left", async () => {
    const api = mockApi({ syncStepMs: 300 });
    const cancel = vi.spyOn(api, "cancelGeneration");
    const { user, router } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    await user.click(await explainButton());
    expect(await screen.findByRole("button", { name: "Stop" })).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "This week" }));
    await user.click(screen.getByRole("tab", { name: "Explain" }));
    expect(screen.getByRole("button", { name: "Stop" })).toHaveAccessibleDescription(
      /leaving this course stops it/,
    );
    expect(cancel).not.toHaveBeenCalled();
    await act(() => router.navigate(paths.courses));
    await waitFor(() => expect(cancel).toHaveBeenCalledTimes(1));
  });

  it("moves the focus to what Show shows, and to the heading when the last one is deleted", async () => {
    const api = mockApi();
    const { user } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    await user.click(await explainButton());
    await screen.findByRole("region", { name: "Explanation of week 4" });
    await user.click(await explainButton());
    await screen.findByText("The explanation is ready.");
    const show = await screen.findByRole("button", { name: /^Show / });
    await user.click(show);
    await waitFor(() =>
      expect(screen.getByRole("region", { name: "Explanation of week 4" })).toHaveFocus(),
    );
    for (let i = 0; i < 2; i++) {
      await user.click(await screen.findByRole("button", { name: "Delete…" }));
      const dialog = await screen.findByRole("alertdialog", { name: "Delete this explanation?" });
      await user.click(within(dialog).getByRole("button", { name: "Delete" }));
      await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
      await waitFor(() =>
        expect(document.activeElement).toBe(
          screen.queryByRole("region", { name: "Explanation of week 4" }) ??
            screen.getByRole("heading", { level: 3, name: "Explain a week" }),
        ),
      );
    }
    expect(screen.queryByRole("region", { name: "Explanation of week 4" })).toBeNull();
  });
});

describe("Course → Explain: what the facade does", () => {
  const DEMO101 = "folder:demo-courses/course/DEMO101"; // "ai-key": week 4 has one graded-looking

  it("offers to include what looks like graded work, and sends only that", async () => {
    const api = mockApi({ scenario: "ai-key" });
    const explain = vi.spyOn(api, "explainWeek");
    const { user } = renderRoute(`${paths.course(DEMO101)}?tab=explain`, { api });
    const button = await screen.findByRole("button", { name: "Explain week 4" });
    await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
    await user.click(button);
    const article = await screen.findByRole("article");
    expect(within(article).getByText(/Assignment 4 — Survey Simulation/)).toHaveTextContent(
      "(looks like graded work)",
    );
    // Over the length limit stays out whatever include says: no button for it.
    expect(within(article).getByText(/Week 4 practice questions/)).toHaveTextContent(
      "(over the length limit)",
    );
    await user.click(
      within(article).getByRole("button", {
        name: "Include the 1 that looks like graded work and write again",
      }),
    );
    await waitFor(() => expect(explain).toHaveBeenCalledTimes(2));
    const include = explain.mock.calls[1]?.[3].include ?? [];
    expect(include).toHaveLength(1);
    const assignment = (await api.weekMaterials(DEMO101, 4)).materials.find((m) =>
      m.title.startsWith("Assignment 4"),
    );
    expect(include).toEqual([assignment?.id]);
  });

  it("stops a run when AI access is turned off from the policy tab", async () => {
    const api = mockApi({ syncStepMs: 300 });
    const cancel = vi.spyOn(api, "cancelGeneration");
    const { user } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    await user.click(await explainButton());
    await screen.findByRole("button", { name: "Stop" });
    await user.click(screen.getByRole("tab", { name: "AI policy" }));
    await user.click(
      await screen.findByRole("switch", { name: "Let my AI app read this course's materials" }),
    );
    await waitFor(() => expect(cancel).toHaveBeenCalledTimes(1));
  });

  it("explains the recent materials when the course has no week now", async () => {
    const api = mockApi();
    const overview = api.courseOverview;
    vi.spyOn(api, "courseOverview").mockImplementation(async (id) => {
      const o = await overview(id);
      return { ...o, timeline: { ...o.timeline, current_week: null, default_week: null } };
    });
    const saved = vi.spyOn(api, "savedExplanations");
    const explain = vi.spyOn(api, "explainWeek");
    const { user } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    const button = await screen.findByRole("button", { name: "Explain the recent materials" });
    await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
    // null would be every week's explanations: never asked for.
    expect(saved.mock.calls.filter(([, week]) => week === null)).toEqual([]);
    await user.click(button);
    await waitFor(() =>
      expect(explain).toHaveBeenCalledWith(
        READABLE,
        null,
        expect.any(String),
        expect.anything(),
        expect.any(Function),
      ),
    );
  });

  it("tells same-day explanations apart by their time", async () => {
    let clock = new Date(2026, 8, 29, 10, 5);
    const api = mockApi({ now: () => clock });
    const { user } = renderRoute(`${paths.course(READABLE)}?tab=explain`, { api });
    await user.click(await explainButton());
    await screen.findByRole("region", { name: "Explanation of week 4" });
    clock = new Date(2026, 8, 29, 14, 40);
    await user.click(await explainButton());
    await screen.findByText("The explanation is ready.");
    const show = await screen.findByRole("button", { name: /^Show / });
    expect(show.getAttribute("aria-label")).toMatch(/10:05/);
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
