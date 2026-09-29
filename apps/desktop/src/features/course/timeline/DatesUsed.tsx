import { type ReactNode, useId } from "react";
import { useTranslation } from "react-i18next";
import type { TermResolution } from "@/api/provisional/courseCalendar";
import { formatIsoDate } from "@/lib/format";
import { translateWithText } from "./evidence";

/** A date as <time>, e.g. "Sep 8, 2026". */
export function DateText({ date }: { date: string }) {
  const { i18n } = useTranslation();
  return <time dateTime={date}>{formatIsoDate(date, i18n.language)}</time>;
}

/** "Sep 8, 2026 – Dec 8, 2026", or "From Sep 8, 2026" without an end. */
export function useSpanText() {
  const { t, i18n } = useTranslation("calendar");
  return (start: string, end: string | null | undefined) =>
    end
      ? t("where.span", {
          start: formatIsoDate(start, i18n.language),
          end: formatIsoDate(end, i18n.language),
        })
      : t("where.spanOpen", { start: formatIsoDate(start, i18n.language) });
}

/** Where confirmed dates came from: "Set by you", "Dates read from the syllabus by …". */
export function useProvenance(term: TermResolution): string | null {
  const { t, i18n } = useTranslation("calendar");
  if (term.anchor !== "student_confirmed" || !term.anchor_origin) return null;
  const label = term.ai_label;
  switch (term.anchor_origin) {
    case "ai":
    case "ai_app":
      if (!label)
        return t(term.anchor_origin === "ai" ? "origin.aiNoLabel" : "origin.ai_appNoLabel");
      // The backend label and model name come from outside the translations: plain text.
      return translateWithText(
        t,
        `origin.${term.anchor_origin}`,
        { date: formatIsoDate(label.created_at.slice(0, 10), i18n.language) },
        { backend: label.backend_label, model: label.model },
      );
    default:
      return t(`origin.${term.anchor_origin}`);
  }
}

/** "Dates used": the dates the resolver counts weeks from, and where they came from. */
export function DatesUsed({ term }: { term: TermResolution }) {
  const { t, i18n } = useTranslation("calendar");
  const headingId = useId();
  const span = useSpanText();
  const provenance = useProvenance(term);
  const [first, second] = term.teaching;
  const hasDates = term.anchor !== "none" || !!term.week_one_monday || term.teaching.length > 0;

  const rows: { label: string; value: ReactNode }[] = [];
  if (hasDates) {
    rows.push({
      label: t("where.source"),
      value: (
        <>
          {t(`anchorTitle.${term.anchor}`)}
          {provenance ? <span className="block text-muted-foreground">{provenance}</span> : null}
        </>
      ),
    });
  }
  if (term.week_one_monday) {
    rows.push({
      label: t("where.weekOne"),
      value: t("where.weekOf", { date: formatIsoDate(term.week_one_monday, i18n.language) }),
    });
  }
  if (first) {
    rows.push({ label: t("where.firstClass"), value: <DateText date={first.first_class} /> });
    if (first.last_class) {
      rows.push({ label: t("where.lastClass"), value: <DateText date={first.last_class} /> });
    }
  }
  if (second) {
    rows.push({ label: t("where.secondPart"), value: span(second.first_class, second.last_class) });
  }
  if (term.breaks.length > 0) {
    rows.push({
      label: t("where.breaks"),
      value: (
        <ul className="space-y-1">
          {term.breaks.map((b) => (
            <li key={`${b.span.start}-${b.kind}`}>
              {t("where.breakItem", {
                kind: t(`breakKind.${b.kind}`),
                span: span(b.span.start, b.span.end),
              })}
              {/* The label is the syllabus's own wording (material text), shown as is. */}
              {b.label ? <span className="block text-muted-foreground">{b.label}</span> : null}
            </li>
          ))}
        </ul>
      ),
    });
  }
  if (term.exams_end) {
    rows.push({ label: t("where.examsEnd"), value: <DateText date={term.exams_end} /> });
  }

  return (
    <div className="space-y-2">
      <h3 id={headingId} className="text-sm font-medium">
        {t("where.datesUsed")}
      </h3>
      {rows.length > 0 ? (
        <dl
          aria-labelledby={headingId}
          className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1.5 text-sm"
        >
          {rows.map((row) => (
            <div key={row.label} className="contents">
              <dt className="text-muted-foreground">{row.label}</dt>
              <dd>{row.value}</dd>
            </div>
          ))}
        </dl>
      ) : (
        <p className="text-sm text-muted-foreground">{t("where.noDates")}</p>
      )}
    </div>
  );
}
