import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { EvidenceItem } from "@/api/provisional/courseCalendar";
import { evidenceText } from "./evidence";

/**
 * "How we worked this out": the facade's evidence items, translated. Without items (an older
 * backend), the English `evidence` lines are shown as they are.
 */
export function EvidenceList({
  items,
  fallback,
  title,
  emptyText,
}: {
  items: EvidenceItem[];
  fallback: string[];
  title: string;
  emptyText?: string;
}) {
  const { t, i18n } = useTranslation("calendar");
  const headingId = useId();
  // De-duplicated so keys stay unique (and a repeated line isn't read twice).
  const lines =
    items.length > 0
      ? [...new Set(items.map((item) => evidenceText(item, t, i18n.language)))]
      : [...new Set(fallback)];
  const english = items.length === 0;

  return (
    <div className="space-y-2">
      <h3 id={headingId} className="text-sm font-medium">
        {title}
      </h3>
      {lines.length > 0 ? (
        <ul
          aria-labelledby={headingId}
          className="list-disc space-y-1 pl-5 text-sm text-muted-foreground"
        >
          {lines.map((line) => (
            <li key={line} lang={english ? "en" : undefined}>
              {line}
            </li>
          ))}
        </ul>
      ) : emptyText ? (
        <p className="text-sm text-muted-foreground">{emptyText}</p>
      ) : null}
    </div>
  );
}
