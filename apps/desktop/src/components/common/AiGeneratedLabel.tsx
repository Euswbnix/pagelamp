import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { formatDate } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * What the label needs: typed structurally, so both a result's GenerationMeta and the course
 * calendar's AiLabel ({ backend_label, model, created_at }) fit without adapting either.
 */
export interface AiLabelSource {
  backend_label: string;
  model: string;
  /** RFC 3339. */
  created_at: string;
  usage?: { input_tokens: number; output_tokens: number } | null;
  /** Some numbers are estimates (e.g. after a cancel): the token count gets "≈". */
  estimated?: boolean;
}

/**
 * "AI-generated · <backend> · <model> · <date> · N tokens" (Canvas API Policy §2E: every output
 * and every copy is labelled). The plain-text form is what Copy appends to the copied text.
 */
export function aiGeneratedLabelText(
  meta: AiLabelSource,
  t: TFunction<"ai">,
  locale: string,
): string {
  const parts = [
    t("aiLabel.prefix"),
    meta.backend_label,
    meta.model,
    formatDate(meta.created_at, locale),
  ];
  if (meta.usage) {
    // Totals: input includes cached tokens, output includes reasoning (TokenUsage).
    const tokens = new Intl.NumberFormat(locale).format(
      meta.usage.input_tokens + meta.usage.output_tokens,
    );
    parts.push(
      meta.estimated ? t("aiLabel.tokensApprox", { tokens }) : t("aiLabel.tokens", { tokens }),
    );
  }
  return parts.join(" · ");
}

/** The label under (or above) anything PageLamp wrote with AI. One line, muted. */
export function AiGeneratedLabel({ meta, className }: { meta: AiLabelSource; className?: string }) {
  const { t, i18n } = useTranslation("ai");
  return (
    <p className={cn("text-xs text-muted-foreground", className)}>
      {aiGeneratedLabelText(meta, t, i18n.language)}
    </p>
  );
}
