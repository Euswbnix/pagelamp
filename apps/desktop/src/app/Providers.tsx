import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { I18nextProvider } from "react-i18next";
import type { StudentOsApi } from "@/api/client";
import { ApiProvider } from "@/api/context";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import i18n from "@/i18n";
import { usePreferences } from "./usePreferences";

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // Local data: no need to hammer the backend when the window regains focus.
        refetchOnWindowFocus: false,
        retry: 1,
        staleTime: 30_000,
      },
    },
  });
}

function PreferencesSync() {
  usePreferences();
  return null;
}

/** Every context the screens need. Tests use the same component with a mock API. */
export function Providers({
  api,
  queryClient,
  children,
}: {
  api: StudentOsApi;
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
