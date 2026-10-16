import { useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi } from "@/api/context";
import { queryKeys, useApplyCourseDatesTo, useSessionCourses } from "@/api/queries";
import type { ReplacedDates, SessionCourse } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";

/**
 * "Also use these dates in": the other courses of this course's session (one term, one week).
 * Courses with no dates of their own come ticked; one with dates of its own comes unticked and
 * says that ticking it replaces them. A hidden course is offered like the others (its dates say
 * nothing about it), and says that it is hidden. Every line can be ticked or unticked.
 */
export interface SameSessionChoice {
  courses: SessionCourse[];
  /** The listing couldn't be had: the form says so instead of listing nothing. */
  failed: boolean;
  chosen: ReadonlySet<string>;
  setChosen: (courseId: string, chosen: boolean) => void;
  /** The courses ticked, in the order listed. */
  ticked: SessionCourse[];
  /** After an apply: the courses that got the dates are unticked; the others stay as chosen. */
  untick: (courseIds: string[]) => void;
}

export function useSameSession(courseId: string): SameSessionChoice {
  const listing = useSessionCourses(courseId);
  const courses = listing.data?.courses ?? [];
  // What the student chose stays when the listing is read again (after a save, a sync); a
  // course listed for the first time comes with its default.
  const [state, setState] = useState({ seen: new Set<string>(), chosen: new Set<string>() });
  let { chosen } = state;
  const unseen = courses.filter((c) => !state.seen.has(c.course_id));
  if (unseen.length > 0) {
    chosen = new Set([...chosen, ...unseen.filter((c) => !c.own_dates).map((c) => c.course_id)]);
    setState({ seen: new Set([...state.seen, ...unseen.map((c) => c.course_id)]), chosen });
  }
  const change = (update: (next: Set<string>) => void) =>
    setState((s) => {
      const next = new Set(s.chosen);
      update(next);
      return { ...s, chosen: next };
    });
  return {
    courses,
    failed: listing.isError,
    chosen,
    ticked: courses.filter((c) => chosen.has(c.course_id)),
    setChosen: (id, on) =>
      change((next) => {
        if (on) next.add(id);
        else next.delete(id);
      }),
    untick: (ids) =>
      change((next) => {
        for (const id of ids) next.delete(id);
      }),
  };
}

/**
 * The list itself: a group of checkboxes under its label. Nothing when there is no other
 * course; a line when the courses couldn't be listed.
 */
export function SameSessionCourses({ choice }: { choice: SameSessionChoice }) {
  const { t } = useTranslation("calendar");
  if (choice.failed) {
    return <p className="text-sm text-muted-foreground">{t("sameSession.listFailed")}</p>;
  }
  if (choice.courses.length === 0) return null;
  return (
    <fieldset className="space-y-1">
      <legend className="text-sm font-medium">{t("sameSession.legend")}</legend>
      <p className="text-sm text-muted-foreground">{t("sameSession.hint")}</p>
      <ul className="divide-y">
        {choice.courses.map((course) => (
          <CourseRow
            key={course.course_id}
            course={course}
            checked={choice.chosen.has(course.course_id)}
            onCheckedChange={(on) => choice.setChosen(course.course_id, on)}
          />
        ))}
      </ul>
    </fieldset>
  );
}

function CourseRow({
  course,
  checked,
  onCheckedChange,
}: {
  course: SessionCourse;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  const { t } = useTranslation("calendar");
  const id = useId();
  return (
    <li className="flex items-start gap-3 py-2">
      <Checkbox
        id={id}
        checked={checked}
        onCheckedChange={(value) => onCheckedChange(value === true)}
        aria-describedby={course.own_dates ? `${id}-own` : undefined}
        className="mt-0.5"
      />
      <div className="min-w-0 flex-1">
        <Label htmlFor={id} className="flex flex-wrap items-center gap-2 font-medium">
          {course.code ?? course.name}
          {course.code ? (
            <span className="font-normal text-muted-foreground">{course.name}</span>
          ) : null}
          {course.hidden ? <Badge variant="outline">{t("sameSession.hidden")}</Badge> : null}
        </Label>
        {course.own_dates ? (
          <p id={`${id}-own`} className="text-xs text-muted-foreground">
            {t("sameSession.ownDates")}
          </p>
        ) : null}
      </div>
    </li>
  );
}

/**
 * After this course's dates are saved: gives them to the courses ticked, and says so in one
 * toast, with Undo: that puts every course back exactly as it was (no dates, or the ones that
 * were replaced, unconfirmed ones from version 0.1 included). `saved`: something was saved
 * here, which the toast says when no other course got the dates.
 * Resolves to the courses that got them, or null when the apply failed (this course's dates
 * are saved all the same).
 */
export function useApplyToSession(courseId: string) {
  const { t } = useTranslation("calendar");
  const api = useApi();
  const client = useQueryClient();
  const apply = useApplyCourseDatesTo();

  async function undo(replaced: ReplacedDates[]) {
    try {
      await api.restoreCourseDates(replaced);
      toast.success(t("sameSession.undone", { count: replaced.length }));
    } catch {
      toast.error(t("sameSession.undoFailed"));
    } finally {
      await client.invalidateQueries({ queryKey: queryKeys.all });
    }
  }

  return async (targets: SessionCourse[], options: { saved: boolean }) => {
    let report: Awaited<ReturnType<typeof apply.mutateAsync>>;
    try {
      report = await apply.mutateAsync({ from: courseId, to: targets.map((c) => c.course_id) });
    } catch {
      return null;
    }
    const applied = targets.filter((c) => report.applied.includes(c.course_id));
    const skipped = targets.filter((c) => report.skipped.some((s) => s.course_id === c.course_id));
    if (applied.length > 0) {
      toast.success(t("sameSession.saved", { count: applied.length }), {
        action: { label: t("sameSession.undo"), onClick: () => void undo(report.replaced) },
      });
    } else if (options.saved) toast.success(t("form.saved"));
    if (skipped.length > 0) {
      toast.warning(
        t("sameSession.skipped", { courses: skipped.map((c) => c.code ?? c.name).join(", ") }),
      );
    }
    return report.applied;
  };
}
