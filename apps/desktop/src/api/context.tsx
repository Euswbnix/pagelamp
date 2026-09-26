import { createContext, type ReactNode, useContext } from "react";
import type { WeekmarkApi } from "./client";

const ApiContext = createContext<WeekmarkApi | null>(null);

/** Provides the API implementation (tauri or mock). Tests pass a fresh mock per test. */
export function ApiProvider({ api, children }: { api: WeekmarkApi; children: ReactNode }) {
  return <ApiContext value={api}>{children}</ApiContext>;
}

export function useApi(): WeekmarkApi {
  const api = useContext(ApiContext);
  if (!api) throw new Error("useApi must be used inside <ApiProvider>");
  return api;
}
