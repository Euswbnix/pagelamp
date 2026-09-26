import { useCallback } from "react";
import { toast } from "sonner";
import { useApi } from "@/api/context";
import { useApiErrorText } from "@/lib/useApiErrorText";

/**
 * Open an http(s) link in the student's browser; if that fails (e.g. no default browser), say
 * so in a toast instead of failing silently. Use this for every external link.
 */
export function useOpenExternal() {
  const api = useApi();
  const errorText = useApiErrorText();
  return useCallback(
    (url: string) => {
      api.openExternal(url).catch((error: unknown) => toast.error(errorText(error)));
    },
    [api, errorText],
  );
}
