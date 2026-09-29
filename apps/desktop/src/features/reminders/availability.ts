import { mockScreensEnabled } from "@/lib/features";

/**
 * The reminders screens and delivery (M3): Settings → Reminders, onboarding's "Remind me" and
 * the notifications. Mock only until src-tauri has the reminder commands (feat/m3-features);
 * set to true in the commit that wires them.
 */
export const REMINDERS_UI = mockScreensEnabled(import.meta.env);
