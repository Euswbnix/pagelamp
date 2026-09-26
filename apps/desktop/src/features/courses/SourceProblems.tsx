import { CircleAlert, KeyRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useStatus } from "@/api/queries";
import type { SourceRecord } from "@/api/types";
import { Button } from "@/components/ui/button";
import { paths } from "@/lib/routes";
import { Notice } from "./parts/Notice";

/**
 * One notice per source whose last sync failed, so stale courses are explained. An expired
 * or revoked secret gets its own wording (Canvas tokens expire; the limit varies by school).
 */
export function SourceProblems() {
  const status = useStatus();
  const failing = (status.data?.sources ?? []).filter((s) => s.last_error_kind);
  if (failing.length === 0) return null;
  return (
    <div className="mb-6 space-y-2">
      {failing.map((source) => (
        <SourceProblem key={source.id} source={source} />
      ))}
    </div>
  );
}

function SourceProblem({ source }: { source: SourceRecord }) {
  const { t } = useTranslation("courses");
  const { t: tc } = useTranslation();
  const kind = source.last_error_kind ?? "other";
  const expired = kind === "auth_expired_or_revoked";

  let title: string;
  let body: string;
  let action: string;
  if (expired && source.kind === "canvas") {
    title = t("problems.tokenExpiredTitle", { source: source.label });
    body = t("problems.tokenExpired");
    action = t("problems.replaceToken");
  } else if (expired && source.kind === "ical") {
    title = t("problems.feedRejectedTitle", { source: source.label });
    body = t("problems.feedRejected");
    action = t("problems.replaceFeed");
  } else {
    // "other" has no useful reason to name ("…couldn't sync: Error"), so leave it out.
    title =
      kind === "other"
        ? t("problems.failedTitleGeneric", { source: source.label })
        : t("problems.failedTitle", { source: source.label, reason: tc(`sourceError.${kind}`) });
    body = t("problems.failed");
    action = t("problems.open");
  }

  return (
    <Notice
      icon={
        expired ? (
          <KeyRound className="size-4 text-warning" aria-hidden />
        ) : (
          <CircleAlert className="size-4 text-warning" aria-hidden />
        )
      }
      title={title}
      action={
        <Button asChild size="sm" variant="outline">
          <Link to={paths.sources}>{action}</Link>
        </Button>
      }
    >
      {body}
    </Notice>
  );
}
