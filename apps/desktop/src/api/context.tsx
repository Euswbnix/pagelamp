import { createContext, type ReactNode, useContext } from "react";
import type { PageLampApi } from "./client";

const ApiContext = createContext<PageLampApi | null>(null);

/** Provides the API implementation (tauri or mock). Tests pass a fresh mock per test. */
export function ApiProvider({ api, children }: { api: PageLampApi; children: ReactNode }) {
  return <ApiContext value={api}>{children}</ApiContext>;
}

export function useApi(): PageLampApi {
  const api = useContext(ApiContext);
  if (!api) throw new Error("useApi must be used inside <ApiProvider>");
  return api;
}
