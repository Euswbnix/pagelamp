import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createMockApi } from "@/api/mock";
import { useUiStore } from "@/stores/ui";
import { renderRoute } from "@/test/render";

function coursesRegion() {
  return screen.findByRole("region", { name: "Your courses" });
}

function group(list: HTMLElement, name: string | RegExp): HTMLElement {
  return within(list).getByRole("region", { name });
}

function card(list: HTMLElement, code: string): HTMLElement {
  const link = within(list).getByRole("link", { name: new RegExp(`^${code}\\b`) });
  const article = link.closest("article");
  if (!article) throw new Error(`No card for ${code}`);
  return article;
}

describe("course list groups and phases", () => {
  it("groups by lifecycle and names each phase", async () => {
    useUiStore.setState({ showPastCourses: true });
    const api = createMockApi({ latencyMs: 0, scenario: "phases" });
    renderRoute("/courses", { api });
    const list = await coursesRegion();

    const current = group(list, "Current");
    expect(within(card(current, "PHS110")).getByText("Week 6")).toBeInTheDocument();
    expect(within(card(current, "PHS120")).getByText("Week 7 · Reading week")).toBeInTheDocument();
    expect(
      within(card(current, "PHS130")).getByText("Reading week (after week 6)"),
    ).toBeInTheDocument();
    expect(within(card(current, "PHS140")).getByText("Exams (after week 12)")).toBeInTheDocument();
    // Kept current by the student although its dates have passed.
    expect(within(card(current, "PHS200")).getByText("Ended")).toBeInTheDocument();

    const upcoming = group(list, "Upcoming");
    expect(within(card(upcoming, "PHS160")).getByText(/^Starts /)).toBeInTheDocument();

    const past = group(list, /^Past courses \(2\)/);
    expect(within(card(past, "PHS150")).getByText("Past course")).toBeInTheDocument();
    expect(within(card(past, "PHS190")).getByText("Past course")).toBeInTheDocument();
    // Headings nest: groups are h3, course titles h4.
    expect(within(past).getAllByRole("heading", { level: 4 })).toHaveLength(2);
  });

  it("never shows the wide Canvas term's week (the owner's case)", async () => {
    const api = createMockApi({ latencyMs: 0, scenario: "uoft-fall" });
    renderRoute("/courses", { api });
    const list = await coursesRegion();
    const current = group(list, "Current");
    expect(within(card(current, "DEM332H5")).getByText("Week 4")).toBeInTheDocument();
    expect(within(card(current, "DEM240H5")).getByText("Week unknown")).toBeInTheDocument();
    expect(within(list).queryByText("Week 22")).toBeNull();
    expect(within(list).getByRole("button", { name: "Past courses (2)" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("moves a past course back to Current from its card, keeping focus on it", async () => {
    useUiStore.setState({ showPastCourses: true });
    const api = createMockApi({ latencyMs: 0, scenario: "all-past" });
    const { user } = renderRoute("/courses", { api });
    const list = await coursesRegion();
    const past = group(list, /^Past courses \(4\)/);
    // The action is a sibling of the card's link, not nested inside it.
    const keep = within(card(past, "PHS150")).getByRole("button", {
      name: "I'm still taking this",
    });
    expect(keep.closest("a")).toBeNull();

    await user.click(keep);
    expect(await screen.findByText("PHS150 moved to your current courses")).toBeInTheDocument();
    const current = group(list, "Current");
    await waitFor(() =>
      expect(within(current).getByRole("link", { name: /^PHS150\b/ })).toHaveFocus(),
    );
    expect(within(card(current, "PHS150")).queryByText("Past course")).toBeNull();
  });
});
