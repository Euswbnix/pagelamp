import { CalendarCheck } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useDeadlines } from "@/api/queries";
import { DeadlineRow } from "@/components/common/DeadlineRow";
import { ErrorState } from "@/components/common/ErrorState";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { formatDay } from "@/lib/format";
import { countDue, type DayGroup, groupByDay } from "./lib/thisWeek";
import { Section } from "./parts/Section";
import { ThisWeekSkeleton } from "./Skeletons";

/** How far ahead the strip looks. */
const DAYS_AHEAD = 7;

/**
 * "This week": every deadline and class in the next 7 days across visible courses, grouped
 * by day. This is the in-app answer to "what do I have to do this week?".
 */
export function ThisWeek() {
  const { t } = useTranslation("courses");
  const deadlines = useDeadlines(null, DAYS_AHEAD, 0);

  let summary: string | null = null;
  let body: ReactNode;
  if (deadlines.isPending) {
    body = <ThisWeekSkeleton />;
  } else if (deadlines.isError) {
    body = (
      <ErrorState
        error={deadlines.error}
        title={t("thisWeek.errorTitle")}
        onRetry={() => void deadlines.refetch()}
      />
    );
  } else {
    // Group first: events without a time can't be placed on a day, so they don't count.
    const groups = groupByDay(deadlines.data);
    if (groups.length === 0) {
      body = <NothingDue />;
    } else {
      const due = countDue(groups);
      summary = due > 0 ? t("thisWeek.summary", { count: due }) : t("thisWeek.nothingDue");
      body = (
        <ol className="divide-y">
          {groups.map((group) => (
            <DayRow key={group.dayDiff} group={group} />
          ))}
        </ol>
      );
    }
  }

  return (
    <Section card title={t("thisWeek.title")} description={summary}>
      {body}
    </Section>
  );
}

function NothingDue() {
  const { t } = useTranslation("courses");
  return (
    <Empty className="border p-6">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <CalendarCheck aria-hidden />
        </EmptyMedia>
        <EmptyTitle>{t("thisWeek.nothingDue")}</EmptyTitle>
        <EmptyDescription>{t("thisWeek.emptyHint")}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  );
}

function DayRow({ group }: { group: DayGroup }) {
  const { t: tc, i18n } = useTranslation();
  const date = formatDay(group.iso, i18n.language);
  const relative =
    group.dayDiff === 0 ? tc("time.today") : group.dayDiff === 1 ? tc("time.tomorrow") : null;

  return (
    <li className="grid gap-x-6 py-2 first:pt-0 last:pb-0 sm:grid-cols-[8rem_1fr]">
      <h3 className="pt-2.5 text-sm font-medium">
        {relative ?? date}
        {relative ? (
          <>
            {" "}
            <span className="block text-xs font-normal text-muted-foreground">{date}</span>
          </>
        ) : null}
      </h3>
      <div>
        {group.deadlines.length > 0 ? (
          <ul>
            {group.deadlines.map((d) => (
              <DeadlineRow key={d.id} deadline={d} showCourse />
            ))}
          </ul>
        ) : null}
        {group.classes.length > 0 ? (
          // Classes are context, not to-dos: keep them visibly quieter.
          <ul className="opacity-70">
            {group.classes.map((d) => (
              <DeadlineRow key={d.id} deadline={d} showCourse />
            ))}
          </ul>
        ) : null}
      </div>
    </li>
  );
}
