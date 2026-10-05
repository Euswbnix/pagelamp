import type { TFunction } from "i18next";
import { CircleAlert, CircleCheck, CircleMinus, LoaderCircle, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useCourses } from "@/api/queries";
import type { CourseSyncSummary, SyncStage } from "@/api/types";
import { Progress } from "@/components/ui/progress";
import { translateWithText } from "@/features/course/timeline/evidence";
import { listsNotShown } from "@/lib/canvasLists";
import { paths } from "@/lib/routes";
import type { SourceProgress } from "@/stores/sync";
import { TechnicalDetails } from "./TechnicalDetails";

/** One source in the sync panel: status, latest message, progress bar, warnings, failure. */
export function SyncProgressRow({
  progress,
  showFixLink,
  courseLines = true,
  linkCourses = false,
}: {
  progress: SourceProgress;
  /** Link to Sources & sync when access expired (the Sources screen has its own button). */
  showFixLink: boolean;
  /** One line per course with something to say. Off where there is no room (the capsule). */
  courseLines?: boolean;
  /** "N not read" leads to the course's page. */
  linkCourses?: boolean;
}) {
  const { t, i18n } = useTranslation("sources");
  const { t: tc } = useTranslation();
  const { label, message, current, total, warnings, result, stopped } = progress;
  // The step, translated from its stage code; without one (an older facade), the English step
  // text shows only in an English UI.
  const detail = progress.stage
    ? stageText(t, progress.stage, progress.course, total)
    : (i18n.resolvedLanguage ?? i18n.language).startsWith("en")
      ? message
      : null;
  const steps =
    current !== null && total !== null && total > 0
      ? t("progress.steps", { current, total })
      : null;
  const percent =
    current !== null && total !== null && total > 0
      ? Math.min(100, Math.round((current / total) * 100))
      : null;

  let icon = <LoaderCircle className="size-4 animate-spin text-muted-foreground" aria-hidden />;
  let status = t("progress.running");
  if (result?.ok) {
    icon = <CircleCheck className="size-4 text-success" aria-hidden />;
    status = t("progress.ok");
  } else if (result) {
    icon = <CircleAlert className="size-4 text-destructive" aria-hidden />;
    status = tc(`sourceError.${result.errorKind ?? "other"}`);
  } else if (stopped) {
    // The run ended before this source finished (e.g. the whole sync failed).
    icon = <CircleMinus className="size-4 text-muted-foreground" aria-hidden />;
    status = t("progress.stopped");
  }
  const inProgress = !result && !stopped;
  const uniqueWarnings = [...new Set(warnings)];

  return (
    <li className="flex gap-3 py-3">
      <span className="mt-0.5 shrink-0">{icon}</span>
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex flex-wrap items-baseline justify-between gap-x-3">
          <span className="font-medium">{label}</span>
          <span className="text-xs text-muted-foreground">
            {status}
            {inProgress && steps ? ` · ${steps}` : null}
          </span>
        </div>

        {inProgress && detail ? <p className="text-xs text-muted-foreground">{detail}</p> : null}
        {inProgress && percent !== null ? (
          // The shared Progress does not forward `value` to the progressbar role, so the
          // aria-value* attributes are passed explicitly.
          <Progress
            value={percent}
            aria-label={t("progress.barLabel", { label })}
            aria-valuenow={percent}
            aria-valuetext={steps ?? undefined}
          />
        ) : null}

        {result?.errorKind === "auth_expired_or_revoked" ? (
          <p className="text-xs">
            {t("progress.expiredHint")}{" "}
            {showFixLink ? (
              <Link to={paths.sources} className="font-medium underline underline-offset-4">
                {t("progress.fixOnSources")}
              </Link>
            ) : null}
          </p>
        ) : null}

        {courseLines && progress.courses ? (
          <CourseLines
            label={label}
            sourceId={progress.sourceId}
            courses={progress.courses}
            link={linkCourses}
          />
        ) : null}

        {uniqueWarnings.length > 0 ? (
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <TriangleAlert className="size-3.5 shrink-0 text-warning" aria-hidden />
            {t("progress.warningCount", { count: uniqueWarnings.length })}
          </p>
        ) : null}
        {/* The error and the warnings as the backend wrote them, for a bug report. */}
        <TechnicalDetails
          lines={[
            ...(result && !result.ok && result.error ? [result.error] : []),
            ...uniqueWarnings,
          ]}
          subject={label}
        />
      </div>
    </li>
  );
}

/**
 * What a full Canvas sync says about each course: which of its lists the course doesn't show,
 * what was found through links, and how much of it wasn't read that went wrong or is for the
 * student to act on (the course's page says what and why). A course with nothing to say has
 * no line, and nothing is concluded from that (a light sync reports no course's lists at all).
 */
function CourseLines({
  label,
  sourceId,
  courses,
  link,
}: {
  label: string;
  sourceId: string;
  courses: CourseSyncSummary[];
  link: boolean;
}) {
  const { t } = useTranslation("sources");
  const { t: tc } = useTranslation();
  // The summary names a course by its code or name, not by its id: a link needs the one course
  // of this source that carries it.
  const known = useCourses().data;
  const courseId = (name: string): string | null => {
    const matches = (known ?? []).filter(
      (c) => c.course.source_id === sourceId && (c.course.code ?? c.course.name) === name,
    );
    return matches.length === 1 ? (matches[0]?.course.id ?? null) : null;
  };
  const lines = courses
    .map((course) => ({
      course: course.course,
      parts: courseParts(t, tc, course),
      notRead: course.not_read ?? 0,
    }))
    .filter((line) => line.parts.length > 0 || line.notRead > 0);
  if (lines.length === 0) return null;
  return (
    <ul
      aria-label={t("progress.course.listLabel", { label })}
      className="space-y-1 text-xs text-muted-foreground"
    >
      {lines.map((line, index) => {
        const id = link && line.notRead > 0 ? courseId(line.course) : null;
        const notRead = t("progress.course.notRead", { count: line.notRead });
        return (
          // The summary has no course id, and two courses can share a code: the place is the key.
          // biome-ignore lint/suspicious/noArrayIndexKey: the list never reorders.
          <li key={index}>
            {/* The course's code or name is the instructor's text: a text node of its own. */}
            <span className="font-medium text-foreground">{line.course}</span>
            {tc("punctuation.colon")}
            {line.parts.join(" · ")}
            {line.notRead > 0 && line.parts.length > 0 ? " · " : null}
            {line.notRead > 0 ? (
              id ? (
                <Link
                  to={paths.courseNotRead(id)}
                  aria-label={translateWithText(
                    t,
                    "progress.course.notReadLabel",
                    { count: line.notRead },
                    { course: line.course },
                  )}
                  className="font-medium text-foreground underline underline-offset-4"
                >
                  {notRead}
                </Link>
              ) : (
                notRead
              )
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

function courseParts(t: TFunction<"sources">, tc: TFunction, course: CourseSyncSummary): string[] {
  const parts: string[] = [];
  const lists = listsNotShown(course.pages_hidden === true, course.files_hidden === true);
  if (lists) parts.push(tc(`canvasLists.${lists}`));
  const pages = course.linked_pages ?? 0;
  const files = course.linked_files ?? 0;
  const pageText = pages > 0 ? t("progress.course.linkedPages", { count: pages }) : null;
  const fileText = files > 0 ? t("progress.course.linkedFiles", { count: files }) : null;
  const items =
    pageText && fileText
      ? t("progress.course.linkedBoth", { pages: pageText, files: fileText })
      : (pageText ?? fileText);
  if (items) parts.push(t("progress.course.linked", { items }));
  return parts;
}

/** Stages about one course; without the course they use their "_no_course" wording. */
const COURSE_STAGES: ReadonlySet<SyncStage> = new Set([
  "reading_course",
  "downloading_files",
  "scanning_files",
  "indexing_files",
]);

/** "Indexing DEM101's files", "Saving 12 calendar events", … (the course code is plain text). */
function stageText(
  t: TFunction<"sources">,
  stage: SyncStage,
  course: string | null,
  total: number | null,
): string {
  if (stage === "saving_events") return t("progress.stage.saving_events", { count: total ?? 0 });
  if (COURSE_STAGES.has(stage)) {
    return course
      ? translateWithText(t, `progress.stage.${stage}`, {}, { course })
      : t(`progress.stage.${stage}_no_course` as "progress.stage.reading_course_no_course");
  }
  return t(`progress.stage.${stage}` as "progress.stage.checking_access");
}
