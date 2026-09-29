import { setTheme } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import { App } from "@/App";
import { API_MODE, createApi } from "@/api";
import { applyBrandTheme, brand } from "@/brand";
import { initI18n } from "@/i18n";
import {
  applyPreferences,
  applyStaticAppearance,
  recordStartupScheme,
  staticAppearance,
} from "@/lib/appearance";
import { useUiStore } from "@/stores/ui";

applyBrandTheme();
// Platform and backdrop before the first render, so the first paint has the right fonts and radii.
applyStaticAppearance(
  document.documentElement,
  staticAppearance({
    mock: API_MODE === "mock",
    search: window.location.search,
    userAgent: navigator.userAgent,
    windowBackdrop: window.__PAGELAMP_WINDOW__?.backdrop,
  }),
  API_MODE === "mock",
);
// The student's theme, transparency and contrast before the first render too; the window is
// shown once the page has loaded (window.rs), so it never flashes the system theme or a Mica the
// student turned off. The system scheme is read before anything calls setTheme.
const prefs = useUiStore.getState();
applyPreferences(document.documentElement, prefs, recordStartupScheme());
if (API_MODE === "tauri" && prefs.theme !== "system") {
  // The native title bar and Mica, too (useAppearance keeps them in step afterwards).
  setTheme(prefs.theme).catch(() => {});
}
document.title = brand.productName;
if (API_MODE === "tauri") {
  // The native window title follows the brand too (permission: core:window:allow-set-title).
  // A static import: api/tauri.ts imports this module anyway, so a dynamic one splits nothing.
  void getCurrentWindow().setTitle(brand.productName);
}
initI18n(useUiStore.getState().locale);

// Inter (Windows, Linux) is font-display: swap; wait for it briefly so the first frame isn't the
// fallback font followed by a reflow. macOS uses system-ui and never loads it.
const platform = document.documentElement.dataset.platform;
if (platform === "windows" || platform === "linux") {
  await Promise.race([
    document.fonts.load('400 14px "Inter Variable"').catch(() => []),
    new Promise((resolve) => setTimeout(resolve, 150)),
  ]);
}

const root = document.getElementById("root");
if (!root) throw new Error("#root missing from index.html");

createRoot(root).render(
  <StrictMode>
    <App api={createApi()} />
  </StrictMode>,
);
