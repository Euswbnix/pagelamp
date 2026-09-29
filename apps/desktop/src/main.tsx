import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import { App } from "@/App";
import { API_MODE, createApi } from "@/api";
import { applyBrandTheme, brand } from "@/brand";
import { initI18n } from "@/i18n";
import { applyStaticAppearance, staticAppearance } from "@/lib/appearance";
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
document.title = brand.productName;
if (API_MODE === "tauri") {
  // The native window title follows the brand too (permission: core:window:allow-set-title).
  // A static import: api/tauri.ts imports this module anyway, so a dynamic one splits nothing.
  void getCurrentWindow().setTitle(brand.productName);
}
initI18n(useUiStore.getState().locale);

const root = document.getElementById("root");
if (!root) throw new Error("#root missing from index.html");

createRoot(root).render(
  <StrictMode>
    <App api={createApi()} />
  </StrictMode>,
);
