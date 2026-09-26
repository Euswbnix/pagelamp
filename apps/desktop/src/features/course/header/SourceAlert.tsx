import { CircleAlert, KeyRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useSources } from "@/api/queries";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { paths } from "@/lib/routes";

/**
 * Warns when the source this course comes from failed its last sync. An expired Canvas token
 * gets its own wording, since replacing the token is the fix.
 */
export function SourceAlert({ sourceId }: { sourceId: string }) {
  const { t } = useTranslation("course");
  const { t: tc } = useTranslation();
  const sources = useSources();
  const kind = sources.data?.find((s) => s.id === sourceId)?.last_error_kind;
  if (!kind) return null;

  const expired = kind === "auth_expired_or_revoked";
  const Icon = expired ? KeyRound : CircleAlert;
  return (
    <Alert variant={expired ? "destructive" : "default"} className="mt-4">
      <Icon aria-hidden />
      <AlertTitle>
        {expired
          ? t("sourceAlert.expiredTitle")
          : t("sourceAlert.failedTitle", { reason: tc(`sourceError.${kind}`) })}
      </AlertTitle>
      <AlertDescription>
        <p>
          {expired ? t("sourceAlert.expiredBody") : t("sourceAlert.failedBody")}{" "}
          <Link to={paths.sources}>{t("sourceAlert.action")}</Link>
        </p>
      </AlertDescription>
    </Alert>
  );
}
