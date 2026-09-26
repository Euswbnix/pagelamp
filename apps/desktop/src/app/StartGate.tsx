import { useTranslation } from "react-i18next";
import { Navigate } from "react-router";
import { useStatus } from "@/api/queries";
import { ErrorState } from "@/components/common/ErrorState";
import { Spinner } from "@/components/ui/spinner";
import { paths } from "@/lib/routes";
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
  if (status.isError) {
    return (
      <div className="mx-auto max-w-lg p-8">
        <h1 className="sr-only">{t("states.backendUnavailableTitle")}</h1>
        <ErrorState error={status.error} onRetry={() => status.refetch()} />
      </div>
    );
  }
  const hasSources = status.data.sources.length > 0;
  return <Navigate to={hasSources || skipped ? paths.courses : paths.welcome} replace />;
}
