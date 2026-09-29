import { mockScreensEnabled } from "@/lib/features";

/**
 * The removal screens and the course dates form v2 (F2: the "courses look finished" banner,
 * "Review past courses…", "Remove from PageLamp…", "Removed courses", breaks and exam dates).
 * Like AI_SETUP_ENABLED on main, they run against the mock until src-tauri has the B5/B6
 * commands (alpha.2), so a real build (alpha.1) never shows screens whose commands don't exist.
 * VITE_PAGELAMP_REMOVAL_UI=1 turns them on in a real build for testing. Delete this switch in
 * the commit that wires the commands. `?shipped` hides them in the mock (lib/features.ts).
 */
export function removalUiEnabled(env: Record<string, unknown>, search?: string): boolean {
  return mockScreensEnabled(env, search) || env.VITE_PAGELAMP_REMOVAL_UI === "1";
}

export const REMOVAL_UI = removalUiEnabled(import.meta.env);
