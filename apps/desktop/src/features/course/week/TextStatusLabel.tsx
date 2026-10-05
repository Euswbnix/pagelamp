import {
  CircleAlert,
  CircleCheck,
  CircleSlash,
  CloudOff,
  FileWarning,
  Hourglass,
  Lock,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DownloadBlock, TextProblem, TextStatus } from "@/api/types";
import { cn } from "@/lib/utils";

const STYLE: Record<TextStatus, { icon: LucideIcon; className: string }> = {
  ok: { icon: CircleCheck, className: "text-success" },
  pending: { icon: Hourglass, className: "text-muted-foreground" },
  unsupported: { icon: CircleSlash, className: "text-muted-foreground" },
  not_downloaded: { icon: CloudOff, className: "text-muted-foreground" },
  error: { icon: CircleAlert, className: "text-warning" },
};

const BLOCKED_ICON: Record<DownloadBlock, LucideIcon> = {
  locked: Lock,
  too_large: FileWarning,
};

/**
 * Whether the AI app can read a material: icon + text, never colour alone. `aiReadable` is false
 * when the course's materials are withheld from the AI app (switch off or "No AI"); an indexed
 * material then only shows its size, without the "readable" claim.
 */
export function TextStatusLabel({
  status,
  chunks,
  aiReadable,
  blocked,
  problem,
}: {
  status: TextStatus;
  chunks: number;
  aiReadable: boolean;
  /** Why a not-downloaded file can't be downloaded on request (Canvas). */
  blocked?: DownloadBlock | null;
  /** Why there is no text (MaterialView.text_problem): a scan reads "No text found". */
  problem?: TextProblem | null;
}) {
  const { t } = useTranslation("course");
  const { t: tc } = useTranslation();
  const { icon: Icon, className } = STYLE[status];
  if (status === "not_downloaded" && blocked) {
    const BlockedIcon = BLOCKED_ICON[blocked];
    return (
      <span className="inline-flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
        <BlockedIcon className={cn("size-3.5 shrink-0", className)} aria-hidden />
        {tc(`downloadBlock.${blocked}`)}
      </span>
    );
  }
  // Read without errors, but nothing in it (a scan): not a success, and not "0 sections".
  if (status === "ok" && problem === "no_text") {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs font-medium text-foreground">
        <CircleAlert className="size-3.5 shrink-0 text-warning" aria-hidden />
        {tc("textStatus.noText")}
      </span>
    );
  }
  if (status === "ok" && !aiReadable) {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
        <Icon className="size-3.5 shrink-0" aria-hidden />
        {t("week.sections", { count: chunks })}
      </span>
    );
  }
  // The status colour is the glyph's; the words stay ink (§4.2: 3.7:1 amber is never text).
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 text-xs font-medium",
        status === "error" ? "text-foreground" : "text-muted-foreground",
      )}
    >
      <Icon className={cn("size-3.5 shrink-0", className)} aria-hidden />
      {status === "ok" ? (
        // Sighted users see the tick icon; screen readers get "readable" spelled out.
        <>
          <span aria-hidden>{t("week.sections", { count: chunks })}</span>
          <span className="sr-only">{t("week.readableSections", { count: chunks })}</span>
        </>
      ) : status === "not_downloaded" ? (
        // A file that is locked or too large was labelled above.
        tc("textStatus.not_downloaded_yet")
      ) : (
        tc(`textStatus.${status}`)
      )}
    </span>
  );
}
