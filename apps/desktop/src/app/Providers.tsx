import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { I18nextProvider } from "react-i18next";
import type { PageLampApi } from "@/api/client";
import { ApiProvider } from "@/api/context";
import { isApiError } from "@/api/errors";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import i18n from "@/i18n";
import { useAppearance } from "./useAppearance";
import { usePreferences } from "./usePreferences";

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // Every call goes to PageLamp's own backend on this computer: whether the browser
        // thinks it is online says nothing about it. (By default a request made while offline
        // waits, and goes on only once the window is visible again. In the tray that never
        // comes: the status poll would stop, and with it everything that waits for a sync
        // elsewhere to end.)
        networkMode: "always",
        // Local data: no need to hammer the backend when the window regains focus.
        refetchOnWindowFocus: false,
        // One retry for transient failures; "not found" won't change, so show it right away.
        retry: (failures, error) => !isApiError(error, "not_found") && failures < 1,
        staleTime: 30_000,
      },
      mutations: { networkMode: "always" },
    },
  });
}

function PreferencesSync() {
  usePreferences();
  useAppearance();
  return null;
}

/** Every context the screens need. Tests use the same component with a mock API. */
export function Providers({
  api,
  queryClient,
  children,
}: {
  api: PageLampApi;
  queryClient?: QueryClient;
  children: ReactNode;
}) {
  const [client] = useState(() => queryClient ?? createQueryClient());
  return (
    <I18nextProvider i18n={i18n}>
      <ApiProvider api={api}>
        <QueryClientProvider client={client}>
          <TooltipProvider>
            <PreferencesSync />
            {children}
            <Toaster position="bottom-right" />
          </TooltipProvider>
        </QueryClientProvider>
      </ApiProvider>
    </I18nextProvider>
  );
}
