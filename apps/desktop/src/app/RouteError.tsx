import { useTranslation } from "react-i18next";
import { useRouteError } from "react-router";
import { ErrorState } from "@/components/common/ErrorState";

/** Last-resort screen when a route crashes while rendering. Data on disk is not affected. */
export function RouteError() {
  const error = useRouteError();
  const { t } = useTranslation();
  return (
    <div className="mx-auto max-w-lg p-8">
      <h1 className="sr-only">{t("states.crashTitle")}</h1>
      <ErrorState
        error={error}
        title={t("states.crashTitle")}
        onRetry={() => window.location.reload()}
      />
      <p className="mt-4 text-center text-sm text-muted-foreground">{t("states.crashBody")}</p>
    </div>
  );
}
