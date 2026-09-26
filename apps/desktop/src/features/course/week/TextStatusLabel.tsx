import {
  CircleAlert,
  CircleCheck,
  CircleSlash,
  CloudOff,
  Hourglass,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { TextStatus } from "@/api/types";
import { cn } from "@/lib/utils";

const STYLE: Record<TextStatus, { icon: LucideIcon; className: string }> = {
  ok: { icon: CircleCheck, className: "text-success" },
  pending: { icon: Hourglass, className: "text-muted-foreground" },
  unsupported: { icon: CircleSlash, className: "text-muted-foreground" },
  not_downloaded: { icon: CloudOff, className: "text-muted-foreground" },
  error: { icon: CircleAlert, className: "text-warning" },
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
}: {
  status: TextStatus;
  chunks: number;
  aiReadable: boolean;
}) {
  const { t } = useTranslation("course");
  const { t: tc } = useTranslation();
  const { icon: Icon, className } = STYLE[status];
  if (status === "ok" && !aiReadable) {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
        <Icon className="size-3.5 shrink-0" aria-hidden />
        {t("week.sections", { count: chunks })}
      </span>
    );
  }
  return (
    <span className={cn("inline-flex items-center gap-1.5 text-xs font-medium", className)}>
      <Icon className="size-3.5 shrink-0" aria-hidden />
      {status === "ok" ? (
        // Sighted users see the tick icon; screen readers get "readable" spelled out.
        <>
          <span aria-hidden>{t("week.sections", { count: chunks })}</span>
          <span className="sr-only">{t("week.readableSections", { count: chunks })}</span>
        </>
      ) : (
        tc(`textStatus.${status}`)
      )}
    </span>
  );
}
