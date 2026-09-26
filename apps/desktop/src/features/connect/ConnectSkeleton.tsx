import { useTranslation } from "react-i18next";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

/** Placeholder shaped like the setup cards while the configs load. */
export function ConnectSkeleton() {
  const { t } = useTranslation();
  return (
    <div aria-busy="true" className="space-y-4">
      <span className="sr-only" role="status">
        {t("states.loading")}
      </span>
      {["a", "b", "c"].map((key) => (
        <Card key={key} aria-hidden>
          <CardHeader>
            <Skeleton className="h-5 w-40" />
            <Skeleton className="h-4 w-32" />
          </CardHeader>
          <CardContent className="space-y-4">
            <Skeleton className="h-16 w-full" />
            <div className="flex gap-3">
              <Skeleton className="size-6 rounded-full" />
              <Skeleton className="h-4 w-2/3" />
            </div>
            <Skeleton className="h-24 w-full" />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
