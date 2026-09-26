import { Ban, BookOpen, CircleCheck, CircleHelp, Quote } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AiPolicy } from "@/api/types";
import { cn } from "@/lib/utils";

const STYLE: Record<AiPolicy, { icon: typeof Ban; className: string }> = {
  unknown: { icon: CircleHelp, className: "border-dashed text-muted-foreground" },
  prohibited: { icon: Ban, className: "border-destructive/30 bg-destructive/10 text-destructive" },
  learning_aid: { icon: BookOpen, className: "border-info/30 bg-info/10 text-info" },
  allowed_with_citation: { icon: Quote, className: "border-success/30 bg-success/10 text-success" },
  unrestricted: { icon: CircleCheck, className: "text-foreground" },
};

/** AI-policy badge: icon + text, so it never relies on colour alone. */
export function PolicyBadge({ policy, className }: { policy: AiPolicy; className?: string }) {
  const { t } = useTranslation();
  const { icon: Icon, className: tone } = STYLE[policy];
  return (
    <span
      className={cn(
        "inline-flex h-6 shrink-0 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium whitespace-nowrap",
        tone,
        className,
      )}
    >
      <Icon className="size-3.5" aria-hidden />
      <span className="sr-only">{t("policy.label")}: </span>
      {t(`policy.${policy}`)}
    </span>
  );
}
