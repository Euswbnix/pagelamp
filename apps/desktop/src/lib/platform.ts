import { API_MODE } from "@/api";

/**
 * The v0.1 beta is unsigned, so macOS may quarantine the bundled `pagelamp` binary when an AI
 * app starts it. The hint only makes sense in a real (production) macOS build of the desktop
 * app — not in dev, mock mode or on other systems.
 */
export function showsMacQuarantineHint(env: {
  apiMode: "mock" | "tauri";
  production: boolean;
  userAgent: string;
}): boolean {
  return env.apiMode === "tauri" && env.production && /Macintosh|Mac OS X/.test(env.userAgent);
}

export const MAC_QUARANTINE_HINT = showsMacQuarantineHint({
  apiMode: API_MODE,
  production: import.meta.env.PROD,
  userAgent: typeof navigator === "undefined" ? "" : navigator.userAgent,
});
