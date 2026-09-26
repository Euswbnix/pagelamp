import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Skeleton } from "@/components/ui/skeleton";

/** Wraps placeholder shapes so assistive tech hears "Loading…" once instead of nothing. */
function Loading({ children, className }: { children: ReactNode; className?: string }) {
  const { t: tc } = useTranslation();
  return (
    <div aria-busy="true" className={className}>
      <span className="sr-only">{tc("states.loading")}</span>
      {children}
    </div>
  );
}

/** The outline of a card section (`<Section card>`) with a heading placeholder. */
function CardOutline({ children }: { children: ReactNode }) {
  return (
    <div className="space-y-4 rounded-xl p-5 ring-1 ring-foreground/10">
      <Skeleton className="h-6 w-32" />
      {children}
    </div>
  );
}

function DayRows() {
  return (
    <div className="space-y-4">
      {[0, 1, 2].map((row) => (
        <div key={row} className="grid gap-3 sm:grid-cols-[8rem_1fr]">
          <Skeleton className="h-4 w-20" />
          <div className="space-y-2">
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-3 w-1/3" />
          </div>
        </div>
      ))}
    </div>
  );
}

function PlanRows() {
  return (
    <div className="space-y-4">
      <Skeleton className="h-10 w-full" />
      {[0, 1].map((row) => (
        <div key={row} className="space-y-2">
          <Skeleton className="h-4 w-24" />
          <Skeleton className="h-4 w-2/3" />
          <Skeleton className="h-4 w-1/2" />
        </div>
      ))}
    </div>
  );
}

function CardGrid() {
  return (
    <div className="space-y-4">
      <Skeleton className="h-6 w-32" />
      <div className="grid gap-3 sm:grid-cols-2">
        {[0, 1, 2, 3].map((card) => (
          <div key={card} className="space-y-3 rounded-xl p-4 ring-1 ring-foreground/10">
            <div className="flex justify-between gap-3">
              <div className="space-y-2">
                <Skeleton className="h-5 w-24" />
                <Skeleton className="h-4 w-40" />
              </div>
              <Skeleton className="h-6 w-24 rounded-full" />
            </div>
            <Skeleton className="h-4 w-32" />
            <Skeleton className="h-4 w-48" />
            <Skeleton className="h-4 w-44" />
          </div>
        ))}
      </div>
    </div>
  );
}

/** Shaped like the day rows of the "This week" strip. */
export function ThisWeekSkeleton() {
  return (
    <Loading>
      <DayRows />
    </Loading>
  );
}

/** Shaped like the notes and a few days of the study plan. */
export function PlanSkeleton() {
  return (
    <Loading>
      <PlanRows />
    </Loading>
  );
}

/** The whole page while the course list loads: This week, the study plan and the course grid. */
export function CoursesPageSkeleton() {
  return (
    <Loading className="space-y-6">
      <CardOutline>
        <DayRows />
      </CardOutline>
      <CardOutline>
        <PlanRows />
      </CardOutline>
      <CardGrid />
    </Loading>
  );
}
