// Whole-page states of the course detail screen: loading, not found, failed to load.
// Each renders exactly one h1 (visually hidden where the design shows none).

import { SearchX } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
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
import { BackToCourses } from "./BackToCourses";

const ROWS = ["a", "b", "c", "d"];

/** Shaped like the header + tabs + a list of materials. */
export function CourseDetailSkeleton() {
  const { t } = useTranslation("course");
  return (
    <div aria-busy="true">
      <h1 className="sr-only">{t("loading")}</h1>
      <Skeleton className="h-4 w-20" />
      <Skeleton className="mt-2 h-8 w-80 max-w-full" />
      <Skeleton className="mt-3 h-4 w-72 max-w-full" />
      <div className="mt-4 flex gap-3">
        <Skeleton className="h-6 w-32 rounded-full" />
        <Skeleton className="h-6 w-40" />
      </div>
      <Skeleton className="mt-8 h-8 w-md max-w-full" />
      <div className="mt-6 space-y-3">
        {ROWS.map((row) => (
          <Skeleton key={row} className="h-14 w-full" />
        ))}
      </div>
    </div>
  );
}

export function CourseNotFound() {
  const { t } = useTranslation("course");
  return (
    <Empty className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <SearchX aria-hidden />
        </EmptyMedia>
        <EmptyTitle>
          <h1 className="font-heading text-lg font-semibold">{t("notFound.title")}</h1>
        </EmptyTitle>
        <EmptyDescription>{t("notFound.description")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button asChild variant="outline">
          <Link to={paths.courses}>{t("notFound.action")}</Link>
        </Button>
      </EmptyContent>
    </Empty>
  );
}

export function CourseLoadError({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  const { t } = useTranslation("course");
  return (
    <div>
      <div className="mb-4">
        <BackToCourses />
      </div>
      <h1 className="sr-only">{t("loadError")}</h1>
      <ErrorState error={error} onRetry={onRetry} />
    </div>
  );
}
