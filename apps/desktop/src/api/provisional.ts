// PROVISIONAL contract types for M0.4 (update preferences and what's due at launch).
//
// The backend is adding these to the facade (target 2026-10-04, a `settings` table in schema v3):
// update_prefs / set_update_prefs, startup_tasks, acknowledge_whats_new,
// acknowledge_update_disclosure, last_update_check. They mirror the proposal agreed with the
// leader; names may still shift. Once they are in `pagelamp schema`, they come from generated.ts
// (through types.ts) and this file is deleted.

export type UpdateChannel = "stable" | "beta";

export interface UpdatePrefs {
  /** Check for updates at launch and then daily (on by default, decision D2). */
  auto_check: boolean;
  /** Chosen by the student; null = the default for this build. */
  channel?: UpdateChannel | null;
}

export interface UpdatePrefsView extends UpdatePrefs {
  /** The channel actually used: the choice, else beta for a pre-release build, else stable. */
  effective_channel: UpdateChannel;
}

/** Topics of the one-time "What's new" sheet (codes; the UI has the text). */
export type WhatsNewTopic = "update_check" | "course_weeks";

export interface WhatsNew {
  /** The version the student upgraded from, when known. */
  since?: string | null;
  topics: WhatsNewTopic[];
}

/**
 * What a surface should do now (the facade decides; the UI only renders). Only the launch
 * classification is cached: `update_check_due` and `whats_new` are recomputed on every call, so
 * the UI asks at launch, hourly while running, and after each acknowledgement.
 */
export interface StartupTasks {
  /** Upgraders only: shown once, before the first automatic update check. */
  whats_new?: WhatsNew | null;
  /** Check automatically now (on, disclosed, and not checked in the last 24 h). */
  update_check_due: boolean;
  /** Set on the first launch after an update: the version it was updated from. */
  updated_from?: string | null;
}

/** How a check ended (serde-tagged by `kind`). An error carries a short code, never a URL. */
export type UpdateCheckOutcome =
  | { kind: "up_to_date" }
  | { kind: "available"; version: string }
  | { kind: "error"; code: string };

export interface UpdateCheckRecord {
  at: string;
  channel: UpdateChannel;
  outcome: UpdateCheckOutcome;
}
