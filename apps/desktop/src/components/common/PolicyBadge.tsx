import { Ban, BookOpen, CircleCheck, CircleHelp, type LucideIcon, Quote } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AiPolicy } from "@/api/types";
import { cn } from "@/lib/utils";

/** One icon per policy — reuse it wherever a policy is shown so they always match. */
export const POLICY_ICON: Record<AiPolicy, LucideIcon> = {
  unknown: CircleHelp,
  prohibited: Ban,
  learning_aid: BookOpen,
  allowed_with_citation: Quote,
  unrestricted: CircleCheck,
};

/** A tinted wash and edge per policy; the words stay ink and the glyph carries the colour. */
const TONE: Record<AiPolicy, string> = {
  unknown: "border-dashed text-muted-foreground",
  prohibited: "border-destructive/30 bg-destructive/10 text-foreground [&>svg]:text-destructive",
  learning_aid: "border-info/30 bg-info/10 text-foreground [&>svg]:text-info",
  allowed_with_citation: "border-success/30 bg-success/10 text-foreground [&>svg]:text-success",
  unrestricted: "text-foreground",
};

/**
 * AI-policy badge: icon + text, so it never relies on colour alone. `plain`: a neutral glyph and
 * ink text, for lists (docs/design/macos-shell.md §3.2: the AI status line is never colour-coded).
 */
export function PolicyBadge({
  policy,
  className,
  plain = false,
}: {
  policy: AiPolicy;
  className?: string;
  plain?: boolean;
}) {
  const { t } = useTranslation();
  const Icon = POLICY_ICON[policy];
  const tone = TONE[policy];
  return (
    <span
      className={cn(
        plain
          ? "inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap"
          : "inline-flex h-6 shrink-0 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium whitespace-nowrap",
        !plain && tone,
        className,
      )}
    >
      <Icon className="size-3.5" aria-hidden />
      <span className="sr-only">{t("policy.label")}: </span>
      {t(`policy.${policy}`)}
    </span>
  );
}
