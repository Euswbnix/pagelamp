// Appearance contexts on <html> for the Lamplight tokens (docs/design/macos-shell.md §8,
// design/tokens/README.md): data-platform and data-backdrop are fixed for the window's life and
// set before the first render; data-transparency, data-contrast and data-window-active follow the
// student's settings and the window's focus (app/useAppearance.ts).

export type Platform = "macos" | "windows" | "linux";
export type Backdrop = "none" | "mica";

declare global {
  interface Window {
    /** What the Rust side built the window with (its initialization script sets it). */
    __PAGELAMP_WINDOW__?: { backdrop?: string };
  }
}

export function platformFromUserAgent(userAgent: string): Platform {
  if (/Windows/i.test(userAgent)) return "windows";
  if (/Macintosh|Mac OS X/i.test(userAgent)) return "macos";
  return "linux";
}

/**
 * The window's fixed contexts. In mock mode, `?platform=` and `?backdrop=` simulate another
 * system (screenshots, tests); a real window only gets Mica when Rust built it with Mica.
 */
export function staticAppearance(env: {
  mock: boolean;
  search: string;
  userAgent: string;
  windowBackdrop?: string;
}): { platform: Platform; backdrop: Backdrop } {
  const params = new URLSearchParams(env.search);
  const asked = env.mock ? params.get("platform") : null;
  const platform: Platform =
    asked === "macos" || asked === "windows" || asked === "linux"
      ? asked
      : platformFromUserAgent(env.userAgent);
  const backdropAsked = env.mock ? params.get("backdrop") : env.windowBackdrop;
  // Mica exists only on Windows 11 (Rust decides; the page just follows).
  const backdrop: Backdrop = backdropAsked === "mica" && platform === "windows" ? "mica" : "none";
  return { platform, backdrop };
}

/** Sets data-platform and data-backdrop; call once, before the first render. */
export function applyStaticAppearance(
  root: HTMLElement,
  appearance: { platform: Platform; backdrop: Backdrop },
) {
  root.dataset.platform = appearance.platform;
  root.dataset.backdrop = appearance.backdrop;
}
