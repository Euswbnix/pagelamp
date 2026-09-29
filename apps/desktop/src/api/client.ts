import type {
  AiPolicy,
  AppStatus,
  CourseOverview,
  CourseSummary,
  CrashReport,
  Deadline,
  IsoDate,
  McpClientConfig,
  SearchHit,
  SourceRecord,
  SourceSyncResult,
  StartupTasks,
  StoredStudyPlan,
  SyncEvent,
  SyncRequest,
  SyncSummary,
  UpdateChannel,
  UpdateCheckRecord,
  UpdatePrefs,
  WeekMaterials,
} from "./types";

// ----- updater (desktop only: src-tauri's updates.rs, not the facade) -------------------------

/** How this build updates: in the app, or (deb/rpm) only by downloading the new package. */
export type UpdateInstallMode = "in_app" | "download_only";

export interface UpdaterStatus {
  /** The real version (CARGO_PKG_VERSION, e.g. "0.3.0-alpha.1"). */
  current_version: string;
  install: UpdateInstallMode;
  platform: "macos" | "windows" | "linux";
}

export interface AvailableUpdate {
  version: string;
  /** RFC 3339 publication date, when the manifest has one. */
  date?: string | null;
  /** Release notes (plain text / Markdown from the manifest). */
  notes?: string | null;
  /** Release page for "download only" installs (deb/rpm). */
  download_url?: string | null;
}

/** Progress of "Install and restart", streamed from Rust. */
export type UpdateEvent =
  | { type: "download_started"; total_bytes?: number | null }
  | { type: "progress"; downloaded_bytes: number; total_bytes?: number | null }
  | { type: "installing" }
  | { type: "restarting" };

/**
 * Everything the UI can ask of the backend. One method per facade method in
 * crates/pagelamp-app (docs/ARCHITECTURE.md §5), plus a few desktop-only helpers at the end.
 *
 * Two implementations:
 * - `tauri.ts` — calls the Rust commands in src-tauri (the real app).
 * - `mock/`    — in-memory synthetic data for the browser and tests (`pnpm dev:mock`).
 *
 * Every method rejects with an `ApiError` (see errors.ts) — branch on `error.kind`.
 * `course` parameters always take `course.id` (never a code).
 */
export interface PageLampApi {
  // ----- status & sources ------------------------------------------------------------------
  status(): Promise<AppStatus>;
  listSources(): Promise<SourceRecord[]>;
  /** Validates the token. The token is passed through and never stored by the UI. */
  addCanvasSource(baseUrl: string, token: string): Promise<SourceRecord>;
  addFolderSource(
    path: string,
    termStart?: IsoDate | null,
    label?: string | null,
  ): Promise<SourceRecord>;
  /** Validates the feed by fetching it. The URL is a secret, like a token. */
  addIcalSource(feedUrl: string, label?: string | null): Promise<SourceRecord>;
  /** Replace an expired Canvas token or a changed feed URL without removing the source. */
  updateSourceSecret(sourceId: string, secret: string): Promise<SourceRecord>;
  /** Removes the source, everything synced from it, and its stored secret. */
  removeSource(sourceId: string): Promise<void>;

  // ----- sync --------------------------------------------------------------------------------
  syncAll(req: SyncRequest, onEvent: (event: SyncEvent) => void): Promise<SyncSummary>;
  syncSource(
    sourceId: string,
    req: SyncRequest,
    onEvent: (event: SyncEvent) => void,
  ): Promise<SourceSyncResult>;

  /**
   * "Download & index this course's files" (Canvas only; folder courses → `invalid`). The UI
   * must first say that downloading through Canvas can count as viewing a file.
   */
  downloadCourseFiles(
    courseId: string,
    onEvent: (event: SyncEvent) => void,
  ): Promise<SourceSyncResult>;

  // ----- read views ----------------------------------------------------------------------------
  /** All courses, hidden ones included (check `course.hidden`). */
  listCourses(): Promise<CourseSummary[]>;
  courseOverview(courseId: string): Promise<CourseOverview>;
  /** `week` omitted/null = the course's current week. */
  weekMaterials(courseId: string, week?: number | null): Promise<WeekMaterials>;
  listDeadlines(courseId: string | null, daysAhead: number, daysBack: number): Promise<Deadline[]>;
  search(query: string, courseId: string | null, limit: number): Promise<SearchHit[]>;
  latestStudyPlan(): Promise<StoredStudyPlan | null>;

  // ----- course settings -------------------------------------------------------------------
  setCoursePolicy(courseId: string, policy: AiPolicy, note: string | null): Promise<void>;
  setCourseTerm(courseId: string, start: IsoDate | null, end: IsoDate | null): Promise<void>;
  setCourseHidden(courseId: string, hidden: boolean): Promise<void>;
  /**
   * "I'm still taking this": the course counts as current until `until` (null = the facade's
   * default: the end of the course's outer date frame, else today + 120 days).
   */
  keepCourseCurrent(courseId: string, until: IsoDate | null): Promise<void>;
  /** Undo "I'm still taking this". */
  clearKeepCourseCurrent(courseId: string): Promise<void>;
  /** "These dates are right": the student checked dates kept from version 0.1. */
  confirmCourseDates(courseId: string): Promise<void>;
  /** "Let my AI app read this course's materials" (§3 rule 8). "No AI" still wins over it. */
  setCourseAiAccess(courseId: string, allowed: boolean): Promise<void>;

  // ----- "connect your AI app" -------------------------------------------------------------
  /** The Rust side decides which `pagelamp` binary the snippets point at. */
  mcpClientConfigs(): Promise<McpClientConfig[]>;

  // ----- diagnostics (work even when the database can't be opened) ----------------------------
  /**
   * Markdown report for bug reports: versions, source states, recent log lines and the last
   * crash. Redacted and pseudonymised by the Rust side; the UI shows it before copying.
   */
  diagnosticReport(): Promise<string>;
  /** The crash the panic hook recorded, until `clearLastCrash()`. */
  lastCrash(): Promise<CrashReport | null>;
  clearLastCrash(): Promise<void>;

  // ----- updates (facade: preferences, what's due now, the last check) ---------------------
  updatePrefs(): Promise<UpdatePrefs>;
  setUpdatePrefs(prefs: UpdatePrefs): Promise<void>;
  /** The chosen channel, else beta for a pre-release build, else stable (decision D3). */
  effectiveUpdateChannel(): Promise<UpdateChannel>;
  /** What to do at launch: the "What's new" sheet, an automatic check, the post-update banner. */
  startupTasks(): Promise<StartupTasks>;
  acknowledgeWhatsNew(): Promise<void>;
  /** The student saw (in onboarding) that PageLamp checks for updates. */
  acknowledgeUpdateDisclosure(): Promise<void>;
  lastUpdateCheck(): Promise<UpdateCheckRecord | null>;

  // ----- desktop helpers (not part of the facade) --------------------------------------------
  /** Native folder picker. Resolves null when cancelled. */
  pickFolder(): Promise<string | null>;
  /** Open an http(s) link in the default browser. Other schemes are rejected. */
  openExternal(url: string): Promise<void>;
  /** Show the PageLamp data folder in Finder / Explorer. */
  revealDataDir(): Promise<void>;
  /**
   * Calls `onFocus` whenever the app window gains focus, e.g. after the student saved a study
   * plan in their AI app. Returns a function that stops listening.
   */
  onWindowFocus(onFocus: () => void): () => void;
  /** Show the folder with PageLamp's log files in Finder / Explorer. */
  revealLogsDir(): Promise<void>;
  /**
   * Write a UI crash (error-boundary) to the log: message and stack only, never app data.
   * Never rejects — logging must not cause a second error.
   */
  logUiError(message: string, stack: string | null): Promise<void>;
  updaterStatus(): Promise<UpdaterStatus>;
  /** Checks the effective channel; the result is recorded (codes only) for diagnostics. */
  checkForUpdate(): Promise<AvailableUpdate | null>;
  /**
   * Downloads, verifies and installs the update found by the last check, then restarts
   * PageLamp (on Windows the installer closes it). Only ever called after the student asked.
   */
  installUpdate(onEvent: (event: UpdateEvent) => void): Promise<void>;
}
