import { ArrowLeft, BookOpenText, Cable } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import type { SyncSummary } from "@/api/types";
import { PageHeader } from "@/components/common/PageHeader";
import { Button } from "@/components/ui/button";
import { SyncProgressPanel } from "@/features/sources/SyncProgressPanel";
import { useSyncOutcome } from "@/features/sources/useSyncOutcome";
import { paths } from "@/lib/routes";
import { useStartSync, useSyncStore } from "@/stores/sync";

const COPY = {
  running: { title: "sync.title", description: "sync.description" },
  done: { title: "sync.doneTitle", description: "sync.doneDescription" },
  doneWithErrors: { title: "sync.problemsTitle", description: "sync.problemsDescription" },
  failed: { title: "sync.failedTitle", description: "sync.failedDescription" },
} as const;

/** Step 3: sync everything once, show progress, then point to "Connect your AI app". */
export function FirstSyncStep({ onBack }: { onBack: () => void }) {
  const { t } = useTranslation("onboarding");
  const { t: tc } = useTranslation();
  const startSync = useStartSync();
  const outcome = useSyncOutcome();
  const summary = useSyncStore((s) => s.lastSummary);
  // True once *our* run has ended, so an older run's result is never shown as this one's.
  const [finished, setFinished] = useState(false);
  const started = useRef(false);

  const run = useCallback(() => {
    setFinished(false);
    void startSync().then(() => setFinished(true));
  }, [startSync]);

  // Start automatically, once per visit to this step (the ref also covers StrictMode).
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    run();
  }, [run]);

  const view = !finished || outcome === "running" || outcome === "idle" ? "running" : outcome;
  const copy = COPY[view];

  return (
    <div>
      <PageHeader title={t(copy.title)} description={t(copy.description)} />
      <div className="space-y-6">
        <SyncProgressPanel onRetry={run} showFixLink />

        {(view === "done" || view === "doneWithErrors") && summary ? (
          <SummaryStats summary={summary} />
        ) : null}

        <div className="flex flex-wrap items-center justify-between gap-3">
          <Button type="button" variant="ghost" onClick={onBack} disabled={view === "running"}>
            <ArrowLeft aria-hidden />
            {tc("actions.back")}
          </Button>
          {view === "running" ? null : (
            <div className="flex flex-wrap gap-3">
              <Button asChild size="lg" variant="outline">
                <Link to={paths.courses}>
                  <BookOpenText aria-hidden />
                  {t("sync.goToCourses")}
                </Link>
              </Button>
              <Button asChild size="lg">
                <Link to={paths.connect}>
                  <Cable aria-hidden />
                  {t("sync.connect")}
                </Link>
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/** Totals of what the run found, plus a nudge when no course turned up. */
function SummaryStats({ summary }: { summary: SyncSummary }) {
  const { t, i18n } = useTranslation("onboarding");
  const headingId = useId();
  const number = new Intl.NumberFormat(i18n.language);
  const sum = (pick: (r: SyncSummary["results"][number]) => number) =>
    summary.results.reduce((total, r) => total + pick(r), 0);
  const courses = sum((r) => r.courses);
  const stats = [
    { key: "courses", label: t("sync.courses"), value: courses },
    { key: "materials", label: t("sync.materials"), value: sum((r) => r.materials) },
    { key: "events", label: t("sync.events"), value: sum((r) => r.events) },
  ];

  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <h2 id={headingId} className="text-sm font-medium">
        {t("sync.summaryLabel")}
      </h2>
      <dl className="grid grid-cols-3 gap-3">
        {stats.map((stat) => (
          <div key={stat.key} className="rounded-lg border bg-card p-3">
            <dt className="text-xs text-muted-foreground">{stat.label}</dt>
            <dd className="font-heading text-2xl font-semibold tabular-nums">
              {number.format(stat.value)}
            </dd>
          </div>
        ))}
      </dl>
      {courses === 0 ? (
        <p className="text-sm text-muted-foreground">{t("sync.noCourses")}</p>
      ) : null}
    </section>
  );
}
