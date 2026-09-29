import { useTranslation } from "react-i18next";
import { toApiError } from "@/api/errors";

/**
 * The explanation of a failed AI call, chosen by code (never by the message): the block reason,
 * the model error (with the retry delay when the provider gave one), else the shared wording.
 */
export function useAiErrorText() {
  const { t } = useTranslation("ai");
  const { t: tc } = useTranslation();
  return (error: unknown): string => {
    const e = toApiError(error);
    const fallback = tc(`errors.${e.kind}`);
    if (e.kind === "blocked" && e.blocked) {
      return t(`blocked.${e.blocked}`, { defaultValue: fallback });
    }
    if (e.kind === "model" && e.model_error) {
      if (e.model_error === "rate_limited" && e.retry_after_secs) {
        return t("modelError.rate_limited_after", { seconds: e.retry_after_secs });
      }
      return t(`modelError.${e.model_error}`, { defaultValue: fallback });
    }
    return fallback;
  };
}
