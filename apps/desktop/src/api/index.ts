import type { StudentOsApi } from "./client";
import { createMockApi, MOCK_SCENARIOS, type MockScenario } from "./mock";
import { createTauriApi } from "./tauri";

export type { StudentOsApi } from "./client";
export { ApiError, isApiError, toApiError } from "./errors";
export * from "./types";

/** `mock` when built with VITE_API=mock (`pnpm dev:mock`, tests), otherwise the real app. */
export const API_MODE: "mock" | "tauri" = import.meta.env.VITE_API === "mock" ? "mock" : "tauri";

function scenarioFromUrl(): MockScenario {
  const value = new URLSearchParams(window.location.search).get("scenario");
  return MOCK_SCENARIOS.includes(value as MockScenario) ? (value as MockScenario) : "demo";
}

export function createApi(): StudentOsApi {
  return API_MODE === "mock" ? createMockApi({ scenario: scenarioFromUrl() }) : createTauriApi();
}
