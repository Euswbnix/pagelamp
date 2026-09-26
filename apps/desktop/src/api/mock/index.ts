// In-memory implementation of StudentOsApi for `pnpm dev:mock` and tests.
//
// It behaves like the real facade where the UI can tell the difference: validation errors use
// the same AppError kinds, sync streams SyncEvents over time, and settings persist for the
// session. Pick a state to look at with `?scenario=` in the URL, e.g.
//   http://localhost:1420/?scenario=expired#/sources
// Scenarios: demo (default) · empty · expired · error · busy.
//
// Secrets passed to this mock (tokens, feed URLs) are validated and then dropped — never stored,
// never logged.

import type { StudentOsApi } from "../client";
import { ApiError } from "../errors";
import {
  type AppStatus,
  aiMaterialsState,
  type CourseSummary,
  type Deadline,
  type SourceKind,
  type SourceRecord,
  type SourceSyncResult,
  type SyncEvent,
  type SyncRequest,
  type SyncSummary,
} from "../types";
import {
  buildMockDb,
  MOCK_BINARY_PATH,
  type MockCourse,
  type MockDb,
  type MockScenario,
  mcpClientConfigs,
} from "./fixtures";

export { MOCK_SCENARIOS, type MockScenario } from "./fixtures";

export interface MockOptions {
  scenario?: MockScenario;
  /** Simulated latency of every call in ms (0 in tests). */
  latencyMs?: number;
  /** Delay between streamed sync events in ms. */
  syncStepMs?: number;
  /** Fixed clock for deterministic tests. */
  now?: () => Date;
}

const DAY = 24 * 60 * 60 * 1000;

function sleep(ms: number): Promise<void> {
  return ms > 0 ? new Promise((resolve) => setTimeout(resolve, ms)) : Promise.resolve();
}

function clone<T>(value: T): T {
  return structuredClone(value);
}

function whenOf(d: Deadline): number | null {
  const iso = d.due_at ?? d.starts_at;
  return iso ? Date.parse(iso) : null;
}

function parseHttpUrl(value: string, allowWebcal = false): URL | null {
  try {
    const url = new URL(value.trim());
    const ok =
      url.protocol === "https:" ||
      url.protocol === "http:" ||
      (allowWebcal && url.protocol === "webcal:");
    return ok ? url : null;
  } catch {
    return null;
  }
}

export function createMockApi(options: MockOptions = {}): StudentOsApi {
  const scenario = options.scenario ?? "demo";
  const latency = options.latencyMs ?? 250;
  const syncStep = options.syncStepMs ?? 350;
  const now = options.now ?? (() => new Date());
  const db: MockDb = buildMockDb(now(), scenario);
  let syncing = false;
  let nextId = 1;

  async function respond<T>(value: T | (() => T), extraLatency = 0): Promise<T> {
    await sleep(latency + extraLatency);
    const result = typeof value === "function" ? (value as () => T)() : value;
    return clone(result);
  }

  function findCourse(courseId: string): MockCourse {
    const found = db.courses.find((c) => c.course.id === courseId);
    if (!found) throw new ApiError("not_found", `No course with id ${courseId}`);
    return found;
  }

  function findSource(sourceId: string): SourceRecord {
    const found = db.sources.find((s) => s.id === sourceId);
    if (!found) throw new ApiError("not_found", `No source with id ${sourceId}`);
    return found;
  }

  function sourceLabel(sourceId: string) {
    return db.sources.find((s) => s.id === sourceId)?.label ?? sourceId;
  }

  function sourceSyncedAt(sourceId: string) {
    return db.sources.find((s) => s.id === sourceId)?.last_synced_at ?? null;
  }

  function deadlinesWithin(courses: MockCourse[], daysAhead: number, daysBack: number) {
    const t = now().getTime();
    return courses
      .flatMap((c) => c.deadlines)
      .filter((d) => {
        const w = whenOf(d);
        return w !== null && w >= t - daysBack * DAY && w <= t + daysAhead * DAY;
      })
      .sort((a, b) => (whenOf(a) ?? 0) - (whenOf(b) ?? 0));
  }

  /** Every week with a module or material, plus the current week, ascending (like Rust). */
  function availableWeeks(c: MockCourse): number[] {
    const weeks = new Set<number>();
    for (const m of [...c.modules, ...c.materials]) if (m.week_hint) weeks.add(m.week_hint);
    if (c.timeline.current_week) weeks.add(c.timeline.current_week);
    return [...weeks].sort((a, b) => a - b);
  }

  function summary(c: MockCourse): CourseSummary {
    const upcoming = deadlinesWithin([c], 21, 0).filter((d) => d.kind !== "class_event");
    const aiMaterials = aiMaterialsState(c.course);
    return {
      course: c.course,
      timeline: c.timeline,
      ai_materials: aiMaterials,
      counts: {
        modules: c.modules.length,
        materials: c.materials.length,
        // Like the facade: "readable by your AI app" is 0 unless the AI may read materials.
        indexed_materials:
          aiMaterials === "readable" ? c.materials.filter((m) => m.text_status === "ok").length : 0,
        upcoming_deadlines: upcoming.length,
      },
      next_deadline: upcoming[0] ?? null,
      source_label: sourceLabel(c.course.source_id),
      last_synced_at: sourceSyncedAt(c.course.source_id),
    };
  }

  function status(): AppStatus {
    const visible = db.courses.filter((c) => !c.course.hidden);
    const materials = db.courses.flatMap((c) => c.materials);
    const indexed = materials.filter((m) => m.text_status === "ok");
    const synced = db.sources
      .map((s) => s.last_synced_at)
      .filter((v): v is string => !!v)
      .sort();
    return {
      version: "0.1.0-mock",
      data_dir: db.dataDir,
      db_path: `${db.dataDir}/studentos.db`,
      sources: db.sources,
      counts: {
        courses: visible.length,
        hidden_courses: db.courses.length - visible.length,
        modules: db.courses.reduce((n, c) => n + c.modules.length, 0),
        materials: materials.length,
        indexed_materials: indexed.length,
        chunks: indexed.reduce((n, m) => n + m.chunk_count, 0),
        events: db.courses.reduce((n, c) => n + c.deadlines.length, 0),
        study_plans: db.studyPlan ? 1 : 0,
      },
      last_synced_at: synced.at(-1) ?? null,
      sync_in_progress: syncing || db.externalSyncRunning,
    };
  }

  function addSource(kind: SourceKind, id: string, label: string, config: Record<string, unknown>) {
    if (db.sources.some((s) => s.id === id)) {
      throw new ApiError("invalid", `${label} is already added`);
    }
    const record: SourceRecord = {
      id,
      kind,
      label,
      config,
      last_synced_at: null,
      last_error: null,
      last_error_kind: null,
    };
    db.sources.push(record);
    return record;
  }

  // Shared by add_canvas_source and update_source_secret: what a real Canvas would say.
  function validateCanvasToken(token: string) {
    const t = token.trim();
    if (t.length < 8)
      throw new ApiError("invalid", "That doesn't look like a Canvas access token.");
    if (/expired|revoked|bad/i.test(t)) {
      throw new ApiError(
        "auth",
        "Canvas rejected this token. It may have expired or been revoked.",
      );
    }
  }

  function validateFeedUrl(feedUrl: string) {
    const url = parseHttpUrl(feedUrl, true);
    if (!url) throw new ApiError("invalid", "Paste the full calendar feed address (https://…).");
    if (url.pathname.includes("404")) {
      throw new ApiError("not_found", "The calendar feed address returned 404 Not Found.");
    }
    if (url.hostname.includes("offline")) {
      throw new ApiError("network", `Couldn't reach ${url.hostname}.`);
    }
  }

  // First-run demo: in the "empty" scenario the first folder/Canvas source to sync "finds" the
  // demo courses, so onboarding → courses can be walked through end to end.
  function seedCoursesOnFirstSync(source: SourceRecord) {
    if (scenario !== "empty" || source.kind === "ical" || db.courses.length > 0) return;
    db.courses = buildMockDb(now(), "demo").courses.map((c) => ({
      ...c,
      course: { ...c.course, source_id: source.id },
    }));
  }

  async function runSync(
    sourceIds: string[],
    onEvent: (event: SyncEvent) => void,
  ): Promise<SourceSyncResult[]> {
    if (syncing || db.externalSyncRunning) {
      await sleep(latency);
      throw new ApiError("busy", "Another StudentOS process is already syncing.");
    }
    syncing = true;
    const results: SourceSyncResult[] = [];
    try {
      for (const sourceId of sourceIds) {
        const source = findSource(sourceId);
        const startedAt = now().toISOString();
        onEvent({ type: "source_started", source_id: source.id, label: source.label });
        seedCoursesOnFirstSync(source);
        const courses = db.courses.filter((c) => c.course.source_id === source.id);
        const total = Math.max(courses.length, 1) * 3;
        const warnings: string[] = [];
        for (let step = 1; step <= total; step++) {
          await sleep(syncStep);
          onEvent({
            type: "progress",
            source_id: source.id,
            message: step < total ? `Indexing materials (${step}/${total})` : "Updating timelines",
            current: step,
            total,
          });
          if (source.kind === "folder" && step === 2) {
            const w = "Skipped 'Week 3 lecture recording.mp4' — video files can't be read.";
            warnings.push(w);
            onEvent({ type: "warning", source_id: source.id, message: w });
          }
        }

        // Failure scenarios stay failed until fixed (token replaced, folder re-added).
        const failure =
          source.last_error_kind === "auth_expired_or_revoked" ||
          source.last_error_kind === "not_found"
            ? { error: source.last_error ?? "Sync failed", kind: source.last_error_kind }
            : null;
        if (failure) {
          onEvent({
            type: "source_finished",
            source_id: source.id,
            ok: false,
            error: failure.error,
            error_kind: failure.kind,
          });
        } else {
          source.last_synced_at = now().toISOString();
          source.last_error = null;
          source.last_error_kind = null;
          onEvent({ type: "source_finished", source_id: source.id, ok: true });
        }
        results.push({
          source_id: source.id,
          label: source.label,
          kind: source.kind,
          ok: !failure,
          error: failure?.error ?? null,
          error_kind: failure?.kind ?? null,
          started_at: startedAt,
          finished_at: now().toISOString(),
          courses: courses.length,
          modules: courses.reduce((n, c) => n + c.modules.length, 0),
          materials: courses.reduce((n, c) => n + c.materials.length, 0),
          files_downloaded: failure ? 0 : 2,
          files_indexed: failure ? 0 : 2,
          events: courses.reduce((n, c) => n + c.deadlines.length, 0),
          warnings,
        });
      }
    } finally {
      syncing = false;
    }
    return results;
  }

  return {
    status: () => respond(status),
    listSources: () => respond(() => db.sources),

    addCanvasSource: async (baseUrl, token) => {
      await sleep(latency + 500);
      const url = parseHttpUrl(baseUrl);
      if (url?.protocol !== "https:") {
        throw new ApiError("invalid", "Enter your Canvas address, e.g. https://canvas.example.edu");
      }
      if (!url || url.hostname.includes("offline")) {
        throw new ApiError("network", `Couldn't reach ${url?.hostname ?? baseUrl}.`);
      }
      validateCanvasToken(token);
      return clone(
        addSource("canvas", `canvas:${url.hostname}`, url.hostname, { base_url: url.origin }),
      );
    },

    addFolderSource: async (path, termStart, label) => {
      await sleep(latency);
      const p = path.trim();
      if (!p) throw new ApiError("invalid", "Choose a folder first.");
      if (p.includes("missing")) throw new ApiError("not_found", `Folder ${p} was not found.`);
      if (termStart && !/^\d{4}-\d{2}-\d{2}$/.test(termStart)) {
        throw new ApiError("invalid", "Term start must be a date like 2026-09-08.");
      }
      const name = label?.trim() || p.split("/").filter(Boolean).at(-1) || p;
      return clone(
        addSource("folder", `folder:mock-${nextId++}`, name, {
          path: p,
          ...(termStart ? { term_start: termStart } : {}),
        }),
      );
    },

    addIcalSource: async (feedUrl, label) => {
      await sleep(latency + 500);
      validateFeedUrl(feedUrl);
      return clone(
        addSource("ical", `ical:mock-${nextId++}`, label?.trim() || "Calendar feed", {}),
      );
    },

    updateSourceSecret: async (sourceId, secret) => {
      await sleep(latency + 500);
      const source = findSource(sourceId);
      if (source.kind === "canvas") validateCanvasToken(secret);
      else if (source.kind === "ical") validateFeedUrl(secret);
      else throw new ApiError("invalid", "Folder sources have no secret.");
      source.last_error = null;
      source.last_error_kind = null;
      return clone(source);
    },

    removeSource: async (sourceId) => {
      await sleep(latency);
      findSource(sourceId);
      db.sources = db.sources.filter((s) => s.id !== sourceId);
      db.courses = db.courses.filter((c) => c.course.source_id !== sourceId);
    },

    syncAll: async (_req: SyncRequest, onEvent) => {
      const startedAt = now().toISOString();
      const results = await runSync(
        db.sources.map((s) => s.id),
        onEvent,
      );
      const out: SyncSummary = {
        started_at: startedAt,
        finished_at: now().toISOString(),
        ok: results.every((r) => r.ok),
        results,
      };
      return clone(out);
    },

    syncSource: async (sourceId, _req, onEvent) => {
      findSource(sourceId);
      const [result] = await runSync([sourceId], onEvent);
      if (!result) throw new ApiError("internal", "Sync produced no result");
      return clone(result);
    },

    downloadCourseFiles: async (courseId, onEvent) => {
      const c = findCourse(courseId);
      const source = findSource(c.course.source_id);
      if (source.kind !== "canvas") {
        await sleep(latency);
        throw new ApiError(
          "invalid",
          "Only Canvas courses have files to download; folder courses are always indexed.",
        );
      }
      const [result] = await runSync([source.id], onEvent);
      if (!result) throw new ApiError("internal", "Sync produced no result");
      let downloaded = 0;
      if (result.ok) {
        for (const m of c.materials) {
          if (m.kind === "file" && m.text_status === "not_downloaded") {
            m.text_status = "ok";
            m.chunk_count = 6;
            downloaded += 1;
          }
        }
      }
      return clone({ ...result, files_downloaded: downloaded, files_indexed: downloaded });
    },

    listCourses: () => respond(() => db.courses.map(summary)),

    courseOverview: (courseId) =>
      respond(() => {
        const c = findCourse(courseId);
        const t = now().getTime();
        const recent = (m: { published_at?: string | null }) =>
          !!m.published_at && Date.parse(m.published_at) >= t - 14 * DAY;
        return {
          course: c.course,
          timeline: c.timeline,
          current_modules: c.modules.filter((m) => c.timeline.current_module_ids.includes(m.id)),
          recent_materials: c.materials
            .filter(recent)
            .sort((a, b) => Date.parse(b.published_at ?? "") - Date.parse(a.published_at ?? "")),
          upcoming_deadlines: deadlinesWithin([c], 21, 0),
          recent_announcements: c.announcements.filter(recent),
          source_label: sourceLabel(c.course.source_id),
          last_synced_at: sourceSyncedAt(c.course.source_id),
          ai_materials: aiMaterialsState(c.course),
        };
      }),

    weekMaterials: (courseId, week) =>
      respond(() => {
        const c = findCourse(courseId);
        const shown = week ?? c.timeline.current_week ?? null;
        if (shown === null) {
          const t = now().getTime();
          return {
            course: c.course,
            week: null,
            requested_week: week ?? null,
            ai_materials: aiMaterialsState(c.course),
            available_weeks: availableWeeks(c),
            timeline: c.timeline,
            modules: [],
            materials: c.materials.filter(
              (m) => !!m.published_at && Date.parse(m.published_at) >= t - 14 * DAY,
            ),
            note: "Current week unknown — showing materials of the last 14 days.",
            note_kind: "current_week_unknown",
          };
        }
        return {
          course: c.course,
          week: shown,
          requested_week: week ?? null,
          ai_materials: aiMaterialsState(c.course),
          available_weeks: availableWeeks(c),
          timeline: c.timeline,
          modules: c.modules.filter((m) => m.week_hint === shown),
          materials: c.materials.filter((m) => m.week_hint === shown),
          ...(c.materials.some((m) => m.week_hint === shown)
            ? { note: null, note_kind: null }
            : {
                note: `No modules or materials for week ${shown}.`,
                note_kind: "no_materials_this_week" as const,
              }),
        };
      }),

    listDeadlines: (courseId, daysAhead, daysBack) =>
      respond(() => {
        const courses = courseId
          ? [findCourse(courseId)]
          : db.courses.filter((c) => !c.course.hidden);
        return deadlinesWithin(courses, daysAhead, daysBack);
      }),

    search: (query, courseId, limit) =>
      respond(() => {
        const q = query.trim().toLowerCase();
        if (!q) return [];
        const courses = courseId ? [findCourse(courseId)] : db.courses;
        return courses
          .flatMap((c) =>
            c.materials
              .filter((m) => m.text_status === "ok" && m.title.toLowerCase().includes(q))
              .map((m, i) => ({
                material_id: m.id,
                material_title: m.title,
                course_id: c.course.id,
                course_code: c.course.code ?? null,
                chunk_ord: 0,
                locator: "p. 1",
                snippet: `…«${query.trim()}» appears in ${m.title}…`,
                url: m.url ?? null,
                week_hint: m.week_hint ?? null,
                score: -1 - i,
              })),
          )
          .slice(0, limit);
      }),

    latestStudyPlan: () => respond(() => db.studyPlan),

    setCoursePolicy: async (courseId, policy, note) => {
      await sleep(latency);
      const c = findCourse(courseId);
      c.course.ai_policy = policy;
      c.course.ai_policy_note = note?.trim() ? note.trim() : null;
    },

    setCourseTerm: async (courseId, start, end) => {
      await sleep(latency);
      const isDate = (v: string | null) => v === null || /^\d{4}-\d{2}-\d{2}$/.test(v);
      if (!isDate(start) || !isDate(end))
        throw new ApiError("invalid", "Use dates like 2026-09-08.");
      if (start && end && end < start) {
        throw new ApiError("invalid", "The term can't end before it starts.");
      }
      const c = findCourse(courseId);
      if (start === null && end === null) {
        // Clearing the override falls back to what the source reported (like the facade).
        c.course.term_start = c.synced.termStart;
        c.course.term_end = c.synced.termEnd;
        c.course.term_source = c.synced.termStart ? "synced" : "none";
        c.timeline = c.synced.timeline;
        return;
      }
      c.course.term_start = start;
      c.course.term_end = end;
      c.course.term_source = "user";
      if (start) {
        const weeks = Math.floor((now().getTime() - Date.parse(start)) / (7 * DAY)) + 1;
        c.timeline = {
          ...c.timeline,
          current_week: weeks >= 1 ? weeks : null,
          confidence: "medium",
          evidence: [`Term start set to ${start} by you → week ${weeks}`],
          outside_term: weeks < 1,
        };
      }
    },

    setCourseHidden: async (courseId, hidden) => {
      await sleep(latency);
      findCourse(courseId).course.hidden = hidden;
    },

    setCourseAiAccess: async (courseId, allowed) => {
      await sleep(latency);
      findCourse(courseId).course.ai_access = allowed;
    },

    mcpClientConfigs: () => respond(() => mcpClientConfigs(MOCK_BINARY_PATH)),

    pickFolder: () => respond("/Users/demo/Documents/Courses"),
    openExternal: async () => {
      // Mock mode never leaves the page: demo links point at *.demo.test.
    },
    revealDataDir: async () => {},
  };
}
