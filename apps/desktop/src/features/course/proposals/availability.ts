/**
 * The course calendar proposal screens (F3: proposal cards, the candidate picker, syllabus
 * reading, the stale banner). Like REMOVAL_UI and AI_SETUP_ENABLED on main, they run against the
 * mock until src-tauri has the B6–B9 commands (alpha.3), so real builds before that never show
 * screens whose commands don't exist. VITE_PAGELAMP_CALENDAR_UI=1 turns them on in a real build
 * for testing. Delete this switch in the commit that wires the commands.
 */
export function calendarUiEnabled(env: Record<string, unknown>): boolean {
  return env.VITE_API === "mock" || env.VITE_PAGELAMP_CALENDAR_UI === "1";
}

export const CALENDAR_UI = calendarUiEnabled(import.meta.env);
