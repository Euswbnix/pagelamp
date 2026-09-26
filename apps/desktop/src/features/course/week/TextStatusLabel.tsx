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

/** Whether the AI app can read a material: icon + text, never colour alone. */
export function TextStatusLabel({ status, chunks }: { status: TextStatus; chunks: number }) {
  const { t } = useTranslation("course");
  const { t: tc } = useTranslation();
  const { icon: Icon, className } = STYLE[status];
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
