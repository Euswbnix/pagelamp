import {
  CircleAlert,
  CircleCheck,
  CircleMinus,
  CircleX,
  Hourglass,
  LoaderCircle,
  RefreshCw,
  X,
} from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { ApiError } from "@/api/errors";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { useStopSync, useSyncStore } from "@/stores/sync";
import { SyncProgressRow } from "./SyncProgressRow";
import { type SyncOutcome, useSyncOutcome } from "./useSyncOutcome";

export interface SyncProgressPanelProps {
  /** Run the sync again (offered when the whole run failed, e.g. another sync was running). */
  onRetry: () => void;
  /** When given, a "Hide" button appears once the run is over. */
  onDismiss?: () => void;
  /** Link expired sources to Sources & sync (onboarding); the Sources screen has buttons. */
  showFixLink?: boolean;
  /**
   * Announce the headline (onboarding). Inside the app shell the accessory bar announces this
   * window's runs, so the panel stays quiet there (one voice per event).
   */
  announce?: boolean;
  className?: string;
}

const HEADLINE = {
  running: { icon: LoaderCircle, tone: "animate-spin text-muted-foreground", key: "sync.syncing" },
  done: { icon: CircleCheck, tone: "text-success", key: "sync.done" },
  doneWithErrors: { icon: CircleAlert, tone: "text-warning", key: "sync.doneWithErrors" },
  failed: { icon: CircleX, tone: "text-destructive", key: "sync.failed" },
  stopped: { icon: CircleMinus, tone: "text-muted-foreground", key: "sync.stopped" },
} as const satisfies Record<Exclude<SyncOutcome, "idle">, unknown>;

/**
 * Live progress of the current (or last) sync run, one row per source. Shared by onboarding
 * step 3 and Sources & sync. Renders nothing before the first run.
 */
export function SyncProgressPanel({
  onRetry,
  onDismiss,
  showFixLink = false,
  announce = true,
  className,
}: SyncProgressPanelProps) {
  const { t } = useTranslation("sources");
  const { t: tc } = useTranslation();
  const titleId = useId();
  const outcome = useSyncOutcome();
  const order = useSyncStore((s) => s.order);
  const bySource = useSyncStore((s) => s.bySource);
  const runError = useSyncStore((s) => s.runError);
  const stopping = useSyncStore((s) => s.stopping);
  const stop = useStopSync();
  const stopHintId = useId();

  if (outcome === "idle") return null;
  const { icon: Icon, tone, key } = HEADLINE[outcome];

  return (
    <section aria-labelledby={titleId} className={className}>
      <Card className="gap-2">
        <CardHeader>
          <CardTitle
            className="flex items-center gap-2"
            aria-live={announce ? "polite" : undefined}
          >
            <Icon className={cn("size-4 shrink-0", tone)} aria-hidden />
            <h2 id={titleId}>{tc(key)}</h2>
          </CardTitle>
          {outcome === "running" ? (
            <CardAction>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void stop()}
                aria-disabled={stopping || undefined}
                aria-describedby={stopHintId}
                className="aria-disabled:opacity-50"
              >
                {stopping ? tc("sync.stopping") : tc("sync.stop")}
              </Button>
              <span id={stopHintId} className="sr-only">
                {tc("sync.stopHint")}
              </span>
            </CardAction>
          ) : null}
          {onDismiss && outcome !== "running" ? (
            <CardAction>
              <Button
                variant="ghost"
                size="sm"
                onClick={onDismiss}
                aria-label={t("progress.dismissLabel")}
              >
                <X aria-hidden />
                {t("progress.dismiss")}
              </Button>
            </CardAction>
          ) : null}
        </CardHeader>
        <CardContent className="space-y-3">
          {runError ? <RunError error={runError} onRetry={onRetry} /> : null}
          {outcome === "running" && order.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("progress.starting")}</p>
          ) : null}
          {order.length > 0 ? (
            <ul className="divide-y">
              {order.map((sourceId) => {
                const progress = bySource[sourceId];
                return progress ? (
                  <SyncProgressRow key={sourceId} progress={progress} showFixLink={showFixLink} />
                ) : null;
              })}
            </ul>
          ) : null}
        </CardContent>
      </Card>
    </section>
  );
}

/** The whole run stopped. `busy` gets the dedicated "already running" wording. */
function RunError({ error, onRetry }: { error: ApiError; onRetry: () => void }) {
  const { t: tc } = useTranslation();
  const busy = error.kind === "busy";
  return (
    <Alert variant={busy ? "default" : "destructive"}>
      {busy ? <Hourglass aria-hidden /> : <CircleAlert aria-hidden />}
      <AlertTitle>{busy ? tc("sync.busy") : tc(`errors.${error.kind}`)}</AlertTitle>
      {!busy && error.message ? <AlertDescription>{error.message}</AlertDescription> : null}
      <div className="col-start-2 mt-2">
        <Button size="sm" variant="outline" onClick={onRetry}>
          <RefreshCw aria-hidden />
          {tc("actions.retry")}
        </Button>
      </div>
    </Alert>
  );
}
