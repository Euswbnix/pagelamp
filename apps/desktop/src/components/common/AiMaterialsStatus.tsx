import { BookLock, BookOpenCheck, BookX } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AiMaterialsState } from "@/api/types";
import { cn } from "@/lib/utils";

const ICON = {
  readable: BookOpenCheck,
  turned_off: BookX,
  withheld_by_policy: BookLock,
} satisfies Record<AiMaterialsState, unknown>;

interface AiMaterialsStatusProps {
  state: AiMaterialsState;
  /** Materials the AI app can read (only meaningful when readable). */
  indexed: number;
  /** All materials of the course. */
  total: number;
  className?: string;
}

/**
 * Can the student's AI app read this course's materials (docs/ARCHITECTURE.md §3 rule 8)?
 * "10 of 13 materials readable…" / "AI access to materials is off" /
 * "Materials not shared (No AI course)". Icon + text, never colour alone.
 */
export function AiMaterialsStatus({ state, indexed, total, className }: AiMaterialsStatusProps) {
  const { t } = useTranslation();
  const Icon = ICON[state];
  let text: string;
  if (state !== "readable") text = t(`aiMaterials.${state}`);
  else if (total === 0) text = t("aiMaterials.readableNone");
  else text = t("aiMaterials.readable", { count: total, indexed });
  return (
    <span className={cn("inline-flex items-start gap-2", className)}>
      <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
      <span>{text}</span>
    </span>
  );
}
