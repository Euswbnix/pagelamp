import { ChevronLeft, ChevronRight, Undo2 } from "lucide-react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface WeekSwitcherProps {
  /** The week shown, or null when the backend fell back to "recent materials". */
  week: number | null;
  currentWeek: number | null;
  /** Ascending, from WeekMaterials.available_weeks. */
  availableWeeks: number[];
  onSelect: (week: number | null) => void;
}

/** "‹ Week 4 (this week) ›" — steps through the weeks that have modules or materials. */
export function WeekSwitcher({ week, currentWeek, availableWeeks, onSelect }: WeekSwitcherProps) {
  const { t } = useTranslation("course");
  const { t: tc } = useTranslation();

  // "Recent materials" (week unknown) counts as newer than every numbered week.
  const previous =
    week === null ? availableWeeks.at(-1) : availableWeeks.filter((w) => w < week).at(-1);
  const next = week === null ? undefined : availableWeeks.find((w) => w > week);
  const showArrows = availableWeeks.length > 0;
  const headingRef = useRef<HTMLHeadingElement>(null);

  return (
    <fieldset className="flex min-w-0 flex-wrap items-center gap-2">
      <legend className="sr-only">{t("week.switcherLabel")}</legend>
      {showArrows ? (
        <Button
          variant="outline"
          size="icon-sm"
          aria-label={t("week.previous")}
          aria-disabled={previous === undefined || undefined}
          className="aria-disabled:opacity-50"
          onClick={() => previous !== undefined && onSelect(previous)}
        >
          <ChevronLeft aria-hidden />
        </Button>
      ) : null}
      <h2
        ref={headingRef}
        tabIndex={-1}
        aria-live="polite"
        className={cn(
          "font-heading text-lg font-semibold tracking-tight outline-none",
          // Centred between the arrows; without them it lines up with the content below.
          showArrows && "min-w-28 px-1 text-center",
        )}
      >
        {week !== null ? tc("week.current", { week }) : t("week.recent")}
        {week !== null && week === currentWeek ? (
          <>
            {" "}
            <span className="text-sm font-normal text-muted-foreground">{t("week.thisWeek")}</span>
          </>
        ) : null}
      </h2>
      {showArrows ? (
        <Button
          variant="outline"
          size="icon-sm"
          aria-label={t("week.next")}
          aria-disabled={next === undefined || undefined}
          className="aria-disabled:opacity-50"
          onClick={() => next !== undefined && onSelect(next)}
        >
          <ChevronRight aria-hidden />
        </Button>
      ) : null}
      {/* Back to the default view: this week, or "Recent materials" when the week is unknown. */}
      {week !== currentWeek ? (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            onSelect(null);
            // This button disappears once back on this week; keep focus on the week's heading.
            headingRef.current?.focus();
          }}
        >
          <Undo2 aria-hidden />
          {currentWeek !== null ? t("week.goToThisWeek") : t("week.showRecent")}
        </Button>
      ) : null}
    </fieldset>
  );
}
