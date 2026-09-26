import { CalendarCheck, Info } from "lucide-react";
import { type ReactNode, useId } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useDeadlines } from "@/api/queries";
import type { Deadline } from "@/api/types";
import { DeadlineRow } from "@/components/common/DeadlineRow";
import { ErrorState } from "@/components/common/ErrorState";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { paths } from "@/lib/routes";

const DAYS_AHEAD = 21;
const DAYS_BACK = 7;

function isPast(deadline: Deadline, now: number) {
  const when = deadline.due_at ?? deadline.starts_at;
  return when ? Date.parse(when) < now : false;
}

/** Upcoming deadlines (next 21 days) and, de-emphasised, the ones from the last 7 days. */
export function DeadlinesTab({ courseId }: { courseId: string }) {
  const { t } = useTranslation("course");
  const query = useDeadlines(courseId, DAYS_AHEAD, DAYS_BACK);

  let body: ReactNode;
  if (query.isError) {
    body = (
      <ErrorState
        error={query.error}
        title={t("deadlines.loadError")}
        onRetry={() => void query.refetch()}
      />
    );
  } else if (query.data) {
    const now = Date.now();
    // Class meetings are calendar events, not deadlines.
    const deadlines = query.data.filter((d) => d.kind !== "class_event");
    const upcoming = deadlines.filter((d) => !isPast(d, now));
    const past = deadlines.filter((d) => isPast(d, now)).reverse(); // most recent first
    body = (
      <>
        <DeadlineSection
          title={t("deadlines.upcomingTitle")}
          description={t("deadlines.upcomingDescription", { days: DAYS_AHEAD })}
        >
          {upcoming.length > 0 ? <DeadlineList deadlines={upcoming} /> : <NothingDue />}
        </DeadlineSection>
        <DeadlineSection
          title={t("deadlines.pastTitle")}
          description={t("deadlines.pastDescription", { days: DAYS_BACK })}
          muted
        >
          {past.length > 0 ? (
            <DeadlineList deadlines={past} />
          ) : (
            <p className="text-sm text-muted-foreground">
              {t("deadlines.pastEmpty", { days: DAYS_BACK })}
            </p>
          )}
        </DeadlineSection>
      </>
    );
  } else {
    body = <DeadlinesSkeleton />;
  }

  return (
    <div className="max-w-3xl space-y-8">
      <p className="flex gap-2 text-sm text-muted-foreground">
        <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
        {t("deadlines.note")}
      </p>
      {body}
    </div>
  );
}

function DeadlineSection({
  title,
  description,
  muted = false,
  children,
}: {
  title: string;
  description: string;
  muted?: boolean;
  children: ReactNode;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-2">
      <div>
        <h2
          id={headingId}
          className={
            muted
              ? "font-heading text-sm font-medium text-muted-foreground"
              : "font-heading text-base font-semibold tracking-tight"
          }
        >
          {title}
        </h2>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      {children}
    </section>
  );
}

function DeadlineList({ deadlines }: { deadlines: Deadline[] }) {
  return (
    <ul className="divide-y rounded-lg border bg-card px-4">
      {deadlines.map((deadline) => (
        <DeadlineRow key={deadline.id} deadline={deadline} />
      ))}
    </ul>
  );
}

function NothingDue() {
  const { t } = useTranslation("course");
  return (
    <Empty className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <CalendarCheck aria-hidden />
        </EmptyMedia>
        <EmptyTitle>{t("deadlines.empty.title", { days: DAYS_AHEAD })}</EmptyTitle>
        <EmptyDescription>{t("deadlines.empty.description")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button asChild variant="outline">
          <Link to={paths.sources}>{t("deadlines.empty.action")}</Link>
        </Button>
      </EmptyContent>
    </Empty>
  );
}

const ROWS = ["a", "b", "c"];

function DeadlinesSkeleton() {
  const { t: tc } = useTranslation();
  return (
    <div className="space-y-3" aria-busy="true">
      <span className="sr-only" role="status">
        {tc("states.loading")}
      </span>
      <Skeleton className="h-5 w-32" />
      {ROWS.map((row) => (
        <Skeleton key={row} className="h-12 w-full" />
      ))}
    </div>
  );
}
