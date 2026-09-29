import { useEffect } from "react";
import { useUiStore } from "@/stores/ui";

/**
 * Keeps <html data-transparency>, <html data-contrast> and <html data-window-active> in step with
 * the student's settings and the window's focus. The system's own settings (prefers-reduced-
 * transparency, prefers-contrast) apply through media queries in tokens.css and glass.css; these
 * switches add them where the web view can't see the system setting (WebKitGTK).
 */
export function useAppearance() {
  const transparency = useUiStore((s) => s.transparency);
  const contrast = useUiStore((s) => s.contrast);

  useEffect(() => {
    document.documentElement.dataset.transparency = transparency;
  }, [transparency]);

  useEffect(() => {
    document.documentElement.dataset.contrast = contrast;
  }, [contrast]);

  useEffect(() => {
    const root = document.documentElement;
    const update = () => {
      root.dataset.windowActive = String(document.hasFocus());
    };
    update();
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    return () => {
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
    };
  }, []);
}
