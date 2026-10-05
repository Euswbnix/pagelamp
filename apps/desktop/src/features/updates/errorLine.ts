import { useTranslation } from "react-i18next";
import type { ApiError } from "@/api/errors";

/**
 * The line under "Couldn't check for updates." or "The update couldn't be installed.". A
 * network failure gets a line of its own: the shared one speaks of "the address", and there is
 * none here that the student typed.
 */
export function useUpdateErrorLine(): (error: ApiError) => string {
  const { t } = useTranslation("updates");
  const { t: tc } = useTranslation();
  return (error) => (error.kind === "network" ? t("errors.network") : tc(`errors.${error.kind}`));
}
