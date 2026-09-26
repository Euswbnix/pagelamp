import { useTranslation } from "react-i18next";
import { useRouteError } from "react-router";
import { ErrorState } from "@/components/common/ErrorState";

/** Last-resort error screen when a route crashes (e.g. the desktop backend is unavailable). */
export function RouteError() {
  const error = useRouteError();
  const { t } = useTranslation();
  return (
    <div className="mx-auto max-w-lg p-8">
      <ErrorState
        error={error}
        title={t("states.backendUnavailableTitle")}
        onRetry={() => window.location.reload()}
      />
      <p className="mt-4 text-center text-sm text-muted-foreground">
        {t("states.backendUnavailableBody")}
      </p>
    </div>
  );
}
