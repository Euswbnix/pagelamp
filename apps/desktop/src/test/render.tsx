// Render a route of the real app against a fresh mock API — the standard way to test screens.
//
//   const { user, api } = renderRoute("/courses");
//   expect(await screen.findByText("DEMO101")).toBeInTheDocument();

import { QueryClient } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router";
import type { StudentOsApi } from "@/api/client";
import { createMockApi, type MockOptions } from "@/api/mock";
import { Providers } from "@/app/Providers";
import { routes } from "@/app/router";

export interface RenderRouteOptions extends MockOptions {
  /** Use this API instead of a new mock (e.g. a mock wrapped with vi.fn spies). */
  api?: StudentOsApi;
}

export function renderRoute(path: string, options: RenderRouteOptions = {}) {
  const api = options.api ?? createMockApi({ latencyMs: 0, syncStepMs: 0, ...options });
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Number.POSITIVE_INFINITY } },
  });
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  const user = userEvent.setup();
  const result = render(
    <Providers api={api} queryClient={queryClient}>
      <RouterProvider router={router} />
    </Providers>,
  );
  return { ...result, api, router, user, queryClient };
}
