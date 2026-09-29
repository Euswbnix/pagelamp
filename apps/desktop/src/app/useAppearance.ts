import { setTheme } from "@tauri-apps/api/app";
import { useEffect, useRef } from "react";
import { API_MODE } from "@/api";
import { systemNativeTheme } from "@/lib/appearance";
import { useUiStore } from "@/stores/ui";

/**
 * Keeps <html data-transparency>, <html data-contrast> and <html data-window-active> in step with
 * the student's settings and the window's focus. The system's own settings (prefers-reduced-
 * transparency, prefers-contrast) apply through media queries in tokens.css and glass.css; these
 * switches add them where the web view can't see the system setting (WebKitGTK).
 */
export function useAppearance() {
  const theme = useUiStore((s) => s.theme);
  const transparency = useUiStore((s) => s.transparency);
  const contrast = useUiStore((s) => s.contrast);

  // The native title bar and Mica follow the in-app theme. "system" leaves the window alone
  // until the student picked light or dark this session (setTheme(null) would force light on
  // Linux); then it goes back to the system's (systemNativeTheme).
  const nativeSet = useRef(false);
  useEffect(() => {
    if (API_MODE !== "tauri") return;
    if (theme === "system" && !nativeSet.current) return;
    nativeSet.current = true;
    const native =
      theme === "system" ? systemNativeTheme(document.documentElement.dataset.platform) : theme;
    setTheme(native).catch(() => {
      // Cosmetic: the page itself already follows the theme.
    });
  }, [theme]);

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
