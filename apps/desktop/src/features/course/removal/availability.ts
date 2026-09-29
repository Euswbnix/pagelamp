/**
 * The removal screens (F2: the "courses look finished" banner, "Review past courses…", "Remove
 * from PageLamp…", "Removed courses" and its count). The commands are real on
 * feat/course-removal; the screens stay behind this switch until that line reaches main, so a
 * real build from main never shows screens whose commands don't exist there.
 * VITE_PAGELAMP_REMOVAL_UI=1 turns them on in a real build. Delete this switch in the commit
 * that makes them real on main. (The dates form v2 has its own switch: timeline/availability.)
 */
export function removalUiEnabled(env: Record<string, unknown>): boolean {
  return env.VITE_API === "mock" || env.VITE_PAGELAMP_REMOVAL_UI === "1";
}

export const REMOVAL_UI = removalUiEnabled(import.meta.env);
