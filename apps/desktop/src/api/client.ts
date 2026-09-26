import type {
  AiPolicy,
  AppStatus,
  CourseOverview,
  CourseSummary,
  Deadline,
  IsoDate,
  McpClientConfig,
  SearchHit,
  SourceRecord,
  SourceSyncResult,
  StoredStudyPlan,
  SyncEvent,
  SyncRequest,
  SyncSummary,
  WeekMaterials,
} from "./types";

/**
 * Everything the UI can ask of the backend. One method per facade method in
 * crates/studentos-app (docs/ARCHITECTURE.md §5), plus a few desktop-only helpers at the end.
 *
 * Two implementations:
 * - `tauri.ts` — calls the Rust commands in src-tauri (the real app).
 * - `mock/`    — in-memory synthetic data for the browser and tests (`pnpm dev:mock`).
 *
 * Every method rejects with an `ApiError` (see errors.ts) — branch on `error.kind`.
 * `course` parameters always take `course.id` (never a code).
 */
export interface StudentOsApi {
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

  // ----- "connect your AI app" -------------------------------------------------------------
  /** The Rust side decides which `studentos` binary the snippets point at. */
  mcpClientConfigs(): Promise<McpClientConfig[]>;

  // ----- desktop helpers (not part of the facade) --------------------------------------------
  /** Native folder picker. Resolves null when cancelled. */
  pickFolder(): Promise<string | null>;
  /** Open an http(s) link in the default browser. Other schemes are rejected. */
  openExternal(url: string): Promise<void>;
  /** Show the StudentOS data folder in Finder / Explorer. */
  revealDataDir(): Promise<void>;
}
