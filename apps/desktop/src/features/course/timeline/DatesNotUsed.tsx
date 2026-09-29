import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { RejectedDates } from "@/api/provisional/courseCalendar";
import { formatIsoDate } from "@/lib/format";
import { useSpanText } from "./DatesUsed";

/**
 * "Dates not used": dates a source reported that don't count weeks, each with the reason
 * (e.g. a May → January Canvas term that is really an enrollment window; design §6.3).
 */
export function DatesNotUsed({ items }: { items: RejectedDates[] }) {
  const { t, i18n } = useTranslation("calendar");
  const headingId = useId();
  const span = useSpanText();
  if (items.length === 0) return null;

  return (
    <div className="space-y-2">
      <h3 id={headingId} className="text-sm font-medium">
        {t("where.datesNotUsed")}
      </h3>
      <ul aria-labelledby={headingId} className="space-y-2 text-sm">
        {items.map((item) => {
          const source = t(`anchorTitle.${item.source}`);
          const what =
            item.end_only && item.end
              ? t("where.notUsedEnd", { source, date: formatIsoDate(item.end, i18n.language) })
              : t("where.notUsedDates", {
                  source,
                  span: item.start
                    ? span(item.start, item.end)
                    : item.end
                      ? formatIsoDate(item.end, i18n.language)
                      : "",
                });
          return (
            <li key={`${item.source}-${item.start}-${item.end}-${item.end_only}`}>
              <span className="block">{what}</span>
              <span className="block text-muted-foreground">{t(`reject.${item.reason}`)}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
