import { CircleAlert, CircleCheck, CircleDashed, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SourceRecord } from "@/api/types";
import { Badge } from "@/components/ui/badge";

/** Source status as icon + text (never colour alone): Syncing / error kind / New / OK. */
export function SourceStatusBadge({ source, syncing }: { source: SourceRecord; syncing: boolean }) {
  const { t } = useTranslation("sources");
  const { t: tc } = useTranslation();

  let content = (
    <>
      <CircleCheck className="text-success" aria-hidden />
      {t("status.ok")}
    </>
  );
  let variant: "outline" | "secondary" | "destructive" = "outline";

  if (syncing) {
    variant = "secondary";
    content = (
      <>
        <LoaderCircle className="animate-spin" aria-hidden />
        {t("status.syncing")}
      </>
    );
  } else if (source.last_error_kind) {
    variant = "destructive";
    content = (
      <>
        <CircleAlert aria-hidden />
        {tc(`sourceError.${source.last_error_kind}`)}
      </>
    );
  } else if (!source.last_synced_at) {
    content = (
      <>
        <CircleDashed className="text-muted-foreground" aria-hidden />
        {t("status.new")}
      </>
    );
  }

  return (
    <Badge variant={variant}>
      <span className="sr-only">{t("status.label")}: </span>
      {content}
    </Badge>
  );
}
