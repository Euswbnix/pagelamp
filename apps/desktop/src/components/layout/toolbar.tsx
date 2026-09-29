import { createContext, type RefObject, useContext, useEffect, useState } from "react";

/** Height of the in-page toolbar row (--pl-size-toolbar-row). */
export const TOOLBAR_ROW_PX = 48;

export interface ToolbarTarget {
  /** Where a screen's toolbar items go (AppShell's sticky row). */
  slot: HTMLElement;
  /** The element that scrolls under the row. */
  scroller: HTMLElement;
}

/** Null outside AppShell (onboarding, tests): screens then keep their actions in the header. */
export const ToolbarContext = createContext<ToolbarTarget | null>(null);

export function useToolbar(): ToolbarTarget | null {
  return useContext(ToolbarContext);
}

/** True once content has scrolled under the toolbar row. */
export function useScrolled(scroller: HTMLElement | null): boolean {
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    if (!scroller) return;
    const update = () => setScrolled(scroller.scrollTop > 0);
    update();
    scroller.addEventListener("scroll", update, { passive: true });
    return () => scroller.removeEventListener("scroll", update);
  }, [scroller]);
  return scrolled;
}

/** True once `target` has scrolled up under the toolbar row. */
export function useScrolledUnder(
  target: RefObject<Element | null>,
  scroller: HTMLElement | null,
): boolean {
  const [under, setUnder] = useState(false);
  useEffect(() => {
    const element = target.current;
    if (!element || !scroller || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry) return;
        const top = entry.rootBounds?.top ?? 0;
        setUnder(!entry.isIntersecting && entry.boundingClientRect.top < top);
      },
      { root: scroller, rootMargin: `-${TOOLBAR_ROW_PX}px 0px 0px 0px` },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [target, scroller]);
  return under;
}
