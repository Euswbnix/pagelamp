import { useTranslation } from "react-i18next";
import { toApiError } from "@/api/errors";

/** Localised explanation for a failed call, chosen by `error.kind` (never by message). */
export function useApiErrorText() {
  const { t } = useTranslation();
  return (error: unknown) => t(`errors.${toApiError(error).kind}`);
}
