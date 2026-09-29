import {
  ChevronDown,
  CircleAlert,
  CircleCheck,
  CircleMinus,
  LoaderCircle,
  TriangleAlert,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Progress } from "@/components/ui/progress";
import { paths } from "@/lib/routes";
import type { SourceProgress } from "@/stores/sync";

/** One source in the sync panel: status, latest message, progress bar, warnings, failure. */
export function SyncProgressRow({
  progress,
  showFixLink,
}: {
  progress: SourceProgress;
  /** Link to Sources & sync when access expired (the Sources screen has its own button). */
  showFixLink: boolean;
}) {
  const { t, i18n } = useTranslation("sources");
  const { t: tc } = useTranslation();
  const { label, message, current, total, warnings, result, stopped } = progress;
  // The facade's step text ("DEMO101: indexing files") is English with no stage code yet, so it
  // shows only in an English UI; other languages keep the translated status and counter.
  const detail = (i18n.resolvedLanguage ?? i18n.language).startsWith("en") ? message : null;
  const steps =
    current !== null && total !== null && total > 0
      ? t("progress.steps", { current, total })
      : null;
  const percent =
    current !== null && total !== null && total > 0
      ? Math.min(100, Math.round((current / total) * 100))
      : null;

  let icon = <LoaderCircle className="size-4 animate-spin text-muted-foreground" aria-hidden />;
  let status = t("progress.running");
  if (result?.ok) {
    icon = <CircleCheck className="size-4 text-success" aria-hidden />;
    status = t("progress.ok");
  } else if (result) {
    icon = <CircleAlert className="size-4 text-destructive" aria-hidden />;
    status = tc(`sourceError.${result.errorKind ?? "other"}`);
  } else if (stopped) {
    // The run ended before this source finished (e.g. the whole sync failed).
    icon = <CircleMinus className="size-4 text-muted-foreground" aria-hidden />;
    status = t("progress.stopped");
  }
  const inProgress = !result && !stopped;

  return (
    <li className="flex gap-3 py-3">
      <span className="mt-0.5 shrink-0">{icon}</span>
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex flex-wrap items-baseline justify-between gap-x-3">
          <span className="font-medium">{label}</span>
          <span className="text-xs text-muted-foreground">
            {status}
            {inProgress && steps ? ` · ${steps}` : null}
          </span>
        </div>

        {inProgress && detail ? <p className="text-xs text-muted-foreground">{detail}</p> : null}
        {inProgress && percent !== null ? (
          // The shared Progress does not forward `value` to the progressbar role, so the
          // aria-value* attributes are passed explicitly.
          <Progress
            value={percent}
            aria-label={t("progress.barLabel", { label })}
            aria-valuenow={percent}
            aria-valuetext={steps ?? undefined}
          />
        ) : null}

        {result && !result.ok && result.error ? (
          <p lang="en" className="text-xs text-destructive">
            {result.error}
          </p>
        ) : null}
        {result?.errorKind === "auth_expired_or_revoked" ? (
          <p className="text-xs">
            {t("progress.expiredHint")}{" "}
            {showFixLink ? (
              <Link to={paths.sources} className="font-medium underline underline-offset-4">
                {t("progress.fixOnSources")}
              </Link>
            ) : null}
          </p>
        ) : null}

        {warnings.length > 0 ? <Warnings warnings={warnings} /> : null}
      </div>
    </li>
  );
}

/** Collapsed "Warnings (2)" list — skipped files and similar, not failures. */
function Warnings({ warnings }: { warnings: string[] }) {
  const { t } = useTranslation("sources");
  const unique = [...new Set(warnings)];
  return (
    <Collapsible>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="xs" className="group -ml-2 text-warning">
          <TriangleAlert aria-hidden />
          {t("progress.warnings", { n: unique.length })}
          <ChevronDown
            className="transition-transform group-aria-expanded:rotate-180"
            aria-hidden
          />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul lang="en" className="list-disc space-y-1 pl-5 text-xs text-muted-foreground">
          {unique.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  );
}
