import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { Course } from "@/api/types";
import { ReadSyllabus } from "./ReadSyllabus";
import { SyllabusSources } from "./SyllabusSources";

/**
 * "Read the dates from the syllabus" on the Timeline tab (F3): which materials are read, then the
 * scan and "Read the syllabus with AI" with its cost, reminder and progress.
 */
export function SyllabusSection({ course }: { course: Course }) {
  const { t } = useTranslation("proposals");
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-4">
      <div className="space-y-1">
        <h2 id={headingId} className="font-heading text-base font-semibold tracking-tight">
          {t("reading.title")}
        </h2>
        <p className="text-sm text-muted-foreground">{t("reading.description")}</p>
      </div>
      <SyllabusSources course={course} />
      <ReadSyllabus course={course} />
    </section>
  );
}
