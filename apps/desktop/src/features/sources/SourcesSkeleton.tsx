import { useTranslation } from "react-i18next";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

/** Placeholder cards shaped like SourceCard while the list loads. */
export function SourcesSkeleton() {
  const { t } = useTranslation("sources");
  return (
    <div className="space-y-4" aria-busy="true">
      <span className="sr-only" role="status">
        {t("list.loading")}
      </span>
      {["a", "b", "c"].map((key) => (
        <Card key={key} aria-hidden>
          <CardHeader>
            <Skeleton className="h-5 w-48" />
            <Skeleton className="h-4 w-24" />
          </CardHeader>
          <CardContent className="space-y-2">
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-1/3" />
          </CardContent>
          <CardFooter className="gap-2">
            <Skeleton className="h-7 w-16" />
            <span className="flex-1" />
            <Skeleton className="h-7 w-20" />
          </CardFooter>
        </Card>
      ))}
    </div>
  );
}
