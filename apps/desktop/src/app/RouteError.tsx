import { useTranslation } from "react-i18next";
import { useRouteError } from "react-router";
import { ErrorState } from "@/components/common/ErrorState";
import { ProblemActions } from "@/features/diagnostics/ProblemActions";
import { useLogUiError } from "@/features/diagnostics/useLogUiError";

/**
 * Last-resort screen when a route crashes while rendering. Data on disk is not affected. The
 * error (message + stack) goes to the log so it shows up in the diagnostic report.
 */
export function RouteError() {
  const error = useRouteError();
  const { t } = useTranslation();
  useLogUiError(error);
  return (
    <div className="mx-auto max-w-lg p-8">
      <h1 className="sr-only">{t("states.crashTitle")}</h1>
      <ErrorState
        error={error}
        title={t("states.crashTitle")}
        onRetry={() => window.location.reload()}
      />
      <p className="mt-4 text-center text-sm text-muted-foreground">{t("states.crashBody")}</p>
      <div className="mt-4">
        <ProblemActions />
      </div>
    </div>
  );
}
