import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PageHeader } from "@/components/common/PageHeader";
import { renderRoute } from "@/test/render";

function toolbarRow(): HTMLElement {
  const row = document.querySelector<HTMLElement>(".pl-toolbar");
  if (!row) throw new Error("no toolbar row");
  return row;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("toolbar row", () => {
  it("holds a screen's actions above its heading", async () => {
    renderRoute("/sources");
    const heading = await screen.findByRole("heading", { level: 1, name: "Sources & sync" });
    const syncAll = screen.getByRole("button", { name: "Sync all" });
    expect(toolbarRow()).toContainElement(syncAll);
    expect(heading.closest("header")).not.toContainElement(syncAll);
    // Keyboard order follows the page: the toolbar's actions come first.
    expect(
      syncAll.compareDocumentPosition(heading) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("starts with a course page's way back", async () => {
    renderRoute(`/courses/${encodeURIComponent("folder:demo-courses/course/DEMO101")}`);
    await screen.findByRole("heading", { level: 1, name: "Intro to Demo Studies" });
    const back = screen.getByRole("link", { name: "All courses" });
    expect(toolbarRow()).toContainElement(back);
    expect(toolbarRow().querySelector("a, button")).toBe(back);
  });

  it("turns to glass only once content scrolls under it", async () => {
    renderRoute("/sources");
    await screen.findByRole("heading", { level: 1, name: "Sources & sync" });
    const main = screen.getByRole("main");
    expect(toolbarRow()).not.toHaveClass("pl-glass");

    main.scrollTop = 120;
    fireEvent.scroll(main);
    expect(toolbarRow()).toHaveClass("pl-glass");
    expect(toolbarRow()).toHaveAttribute("data-scrolled");

    main.scrollTop = 0;
    fireEvent.scroll(main);
    expect(toolbarRow()).not.toHaveClass("pl-glass");
  });

  it("echoes the title (hidden from screen readers) once the heading is under the row", async () => {
    let report: IntersectionObserverCallback = () => {};
    vi.stubGlobal(
      "IntersectionObserver",
      class {
        constructor(callback: IntersectionObserverCallback) {
          report = callback;
        }
        observe() {}
        disconnect() {}
      },
    );
    renderRoute("/settings");
    await screen.findByRole("heading", { level: 1, name: "Settings" });
    const echo = toolbarRow().querySelector(".pl-toolbar-title");
    expect(echo).toHaveTextContent("Settings");
    expect(echo).toHaveAttribute("aria-hidden", "true");
    expect(echo).not.toHaveAttribute("data-shown");

    const under = {
      isIntersecting: false,
      boundingClientRect: { top: -20 },
      rootBounds: { top: 48 },
    } as unknown as IntersectionObserverEntry;
    act(() => report([under], {} as IntersectionObserver));
    expect(echo).toHaveAttribute("data-shown");

    const back = { ...under, isIntersecting: true, boundingClientRect: { top: 80 } };
    act(() => report([back as IntersectionObserverEntry], {} as IntersectionObserver));
    expect(echo).not.toHaveAttribute("data-shown");
  });

  it("keeps the leading item and actions in the page outside the app shell", () => {
    render(
      <PageHeader
        title="Welcome"
        leading={<a href="#back">Back</a>}
        actions={<button type="button">Next</button>}
      />,
    );
    const header = screen.getByRole("banner");
    expect(header).toContainElement(screen.getByRole("button", { name: "Next" }));
    const back = screen.getByRole("link", { name: "Back" });
    expect(back.compareDocumentPosition(header) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(document.querySelector(".pl-toolbar-title")).toBeNull();
  });
});
