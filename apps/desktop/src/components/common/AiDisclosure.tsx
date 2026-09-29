import { ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

/**
 * The required AI disclosure (docs/ARCHITECTURE.md §3.6). `full` is the exact policy wording;
 * `short` is a one-line summary for tight spaces — use `full` at least once per flow.
 */
export function AiDisclosure({
  variant = "full",
  className,
}: {
  variant?: "full" | "short";
  className?: string;
}) {
  const { t } = useTranslation();
  return (
    <div
      className={cn(
        "pl-callout pl-prose flex gap-3 p-4 text-sm leading-relaxed text-muted-foreground",
        className,
      )}
    >
      <ShieldCheck className="mt-0.5 size-4 shrink-0 text-foreground" aria-hidden />
      <p>{variant === "full" ? t("disclosure.full") : t("disclosure.short")}</p>
    </div>
  );
}
