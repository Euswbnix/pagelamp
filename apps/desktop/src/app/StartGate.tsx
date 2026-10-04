import { useTranslation } from "react-i18next";
import { Navigate } from "react-router";
import { isApiError } from "@/api/errors";
import { useStatus } from "@/api/queries";
import { ErrorState } from "@/components/common/ErrorState";
import { Spinner } from "@/components/ui/spinner";
import { ProblemActions } from "@/features/diagnostics/ProblemActions";
import { SchemaErrorScreen } from "@/features/updates/SchemaErrorScreen";
import { paths } from "@/lib/routes";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";

/** First screen: onboarding when there are no sources yet, otherwise the course list. */
export function StartGate() {
  const { t } = useTranslation();
  const status = useStatus();
  const skipped = useUiStore((s) => s.onboardingSkipped);

  if (status.isPending) {
    return (
      <div className="grid h-dvh place-items-center">
        <h1 className="sr-only">{t("states.loading")}</h1>
        <Spinner className="size-6 text-muted-foreground" aria-label={t("states.loading")} />
      </div>
    );
  }
  if (
    status.isError &&
    (isApiError(status.error, "schema_too_new") || isApiError(status.error, "schema_too_old"))
  ) {
    return <SchemaErrorScreen error={status.error} />;
  }
  if (status.isError) {
    return (
      <div className="mx-auto max-w-lg p-8">
        <h1 className="sr-only">{t("states.backendUnavailableTitle")}</h1>
        <ErrorState
          error={status.error}
          onRetry={() => {
            // The student is here: a sync that is due once the app opens is attended.
            useSyncStore.getState().noteStudentAction();
            void status.refetch();
          }}
        />
        {/* Diagnostics work even when the database can't be opened. */}
        <div className="mt-4">
          <ProblemActions />
        </div>
      </div>
    );
  }
  const hasSources = status.data.sources.length > 0;
  return <Navigate to={hasSources || skipped ? paths.courses : paths.welcome} replace />;
}
