// ─── Tauri boundary ────────────────────────────────────────────────────────────────────────
// This file is the ONLY place the UI talks to Rust. Each method invokes one command defined in
// src-tauri/src/commands.rs, which is a thin wrapper over `studentos_app::App`.
//
// Argument names: Tauri maps Rust snake_case parameters to camelCase keys, so the Rust
// parameter `base_url` is passed here as `baseUrl`.
// Errors: commands reject with a serialised `AppError`; `call` turns that into an ApiError.
// ───────────────────────────────────────────────────────────────────────────────────────────

import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isHttpUrl } from "@/lib/url";
import type { StudentOsApi } from "./client";
import { ApiError, toApiError } from "./errors";
import type { SyncEvent } from "./types";

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toApiError(error);
  }
}

function eventChannel(onEvent: (event: SyncEvent) => void): Channel<SyncEvent> {
  const channel = new Channel<SyncEvent>();
  channel.onmessage = onEvent;
  return channel;
}

export function createTauriApi(): StudentOsApi {
  return {
    status: () => call("status"),
    listSources: () => call("list_sources"),
    addCanvasSource: (baseUrl, token) => call("add_canvas_source", { baseUrl, token }),
    addFolderSource: (path, termStart, label) =>
      call("add_folder_source", { path, termStart: termStart ?? null, label: label ?? null }),
    addIcalSource: (feedUrl, label) => call("add_ical_source", { feedUrl, label: label ?? null }),
    updateSourceSecret: (sourceId, secret) => call("update_source_secret", { sourceId, secret }),
    removeSource: (sourceId) => call("remove_source", { sourceId }),

    syncAll: (req, onEvent) => call("sync_all", { req, onEvent: eventChannel(onEvent) }),
    syncSource: (sourceId, req, onEvent) =>
      call("sync_source", { sourceId, req, onEvent: eventChannel(onEvent) }),

    downloadCourseFiles: (courseId, onEvent) =>
      call("download_course_files", { course: courseId, onEvent: eventChannel(onEvent) }),

    listCourses: () => call("list_courses"),
    courseOverview: (courseId) => call("course_overview", { course: courseId }),
    weekMaterials: (courseId, week) =>
      call("week_materials", { course: courseId, week: week ?? null }),
    listDeadlines: (courseId, daysAhead, daysBack) =>
      call("list_deadlines", { course: courseId, daysAhead, daysBack }),
    search: (query, courseId, limit) => call("search", { query, course: courseId, limit }),
    latestStudyPlan: () => call("latest_study_plan"),

    setCoursePolicy: (courseId, policy, note) =>
      call("set_course_policy", { course: courseId, policy, note }),
    setCourseTerm: (courseId, start, end) =>
      call("set_course_term", { course: courseId, start, end }),
    setCourseHidden: (courseId, hidden) => call("set_course_hidden", { course: courseId, hidden }),
    setCourseAiAccess: (courseId, allowed) =>
      call("set_course_ai_access", { course: courseId, allowed }),

    mcpClientConfigs: () => call("mcp_client_configs"),

    // Plugin calls, like commands, reject only with an ApiError.
    pickFolder: async () => {
      try {
        // Needs capability `dialog:allow-open` (src-tauri/capabilities/default.json).
        const picked = await open({ directory: true, multiple: false });
        return typeof picked === "string" ? picked : null;
      } catch (error) {
        throw toApiError(error);
      }
    },
    openExternal: async (url) => {
      // The opener capability is scoped to http(s) too; this check gives a clearer error.
      if (!isHttpUrl(url)) throw new ApiError("invalid", "Only web links can be opened");
      try {
        // Normalised (scheme/host lower-cased, spaces trimmed) so it matches the scope glob.
        await openUrl(new URL(url.trim()).href);
      } catch (error) {
        throw toApiError(error);
      }
    },
    revealDataDir: () => call("reveal_data_dir"),
  };
}
