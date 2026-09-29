import { ChevronRight } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { useStatus } from "@/api/queries";
import type { CourseSummary, SourceErrorKind } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useUiStore } from "@/stores/ui";
import { CourseCard } from "./CourseCard";
import { sourceErrors } from "./lib/courses";
import { groupCourses } from "./lib/groups";
import { Section } from "./parts/Section";

/**
 * The course cards, grouped by the facade's lifecycle: Current, Upcoming, and "Past courses"
 * (collapsed by default). Hidden courses appear, in their group, only when "Show hidden
 * courses" is on.
 */
export function CourseList({ courses }: { courses: CourseSummary[] }) {
  const { t } = useTranslation("courses");
  const { t: tcal } = useTranslation("calendar");
  const switchId = useId();
  const showHidden = useUiStore((s) => s.showHiddenCourses);
  const setShowHidden = useUiStore((s) => s.setShowHiddenCourses);
  const status = useStatus();
  const errors = sourceErrors(status.data?.sources);

  const hiddenCount = courses.filter((c) => c.course.hidden).length;
  const groups = groupCourses(courses, showHidden);
  const total = groups.current.length + groups.upcoming.length + groups.past.length;
  // Group headings only when there is more than one group to tell apart.
  const grouped = groups.upcoming.length > 0 || groups.past.length > 0;

  return (
    <Section
      title={t("list.title")}
      actions={
        hiddenCount > 0 ? (
          <div className="flex items-center gap-2">
            <Switch id={switchId} checked={showHidden} onCheckedChange={setShowHidden} />
            <Label htmlFor={switchId} className="font-normal">
              {t("list.showHidden", { count: hiddenCount })}
            </Label>
          </div>
        ) : null
      }
    >
      {total === 0 ? (
        <p className="rounded-xl border border-dashed p-6 text-center text-sm text-muted-foreground">
          {t("list.allHidden")}
        </p>
      ) : !grouped ? (
        <CourseGrid courses={groups.current} errors={errors} headingLevel="h3" />
      ) : (
        <div className="space-y-8">
          <Group title={tcal("groups.current")}>
            {groups.current.length > 0 ? (
              <CourseGrid courses={groups.current} errors={errors} />
            ) : (
              <p className="rounded-xl border border-dashed p-6 text-center text-sm text-muted-foreground">
                {tcal("groups.noCurrent")}
              </p>
            )}
          </Group>
          {groups.upcoming.length > 0 ? (
            <Group title={tcal("groups.upcoming")}>
              <CourseGrid courses={groups.upcoming} errors={errors} />
            </Group>
          ) : null}
          {groups.past.length > 0 ? <PastGroup courses={groups.past} errors={errors} /> : null}
        </div>
      )}
    </Section>
  );
}

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="space-y-3">
      <h3 id={id} className="text-sm font-medium text-muted-foreground">
        {title}
      </h3>
      {children}
    </section>
  );
}

/** "Past courses (N)": collapsed by default; the student's choice is remembered. */
function PastGroup({
  courses,
  errors,
}: {
  courses: CourseSummary[];
  errors: Map<string, SourceErrorKind>;
}) {
  const { t } = useTranslation("calendar");
  const id = useId();
  const open = useUiStore((s) => s.showPastCourses);
  const setOpen = useUiStore((s) => s.setShowPastCourses);
  return (
    <Collapsible open={open} onOpenChange={setOpen} asChild>
      <section aria-labelledby={id} className="space-y-3">
        <h3 id={id} className="text-sm font-medium text-muted-foreground">
          <CollapsibleTrigger asChild>
            <Button variant="ghost" size="sm" className="-ml-2 text-muted-foreground">
              <ChevronRight
                aria-hidden
                className="transition-transform motion-reduce:transition-none [[data-state=open]_&]:rotate-90"
              />
              {t("groups.past", { count: courses.length })}
            </Button>
          </CollapsibleTrigger>
        </h3>
        <CollapsibleContent className="space-y-3">
          <p className="text-sm text-muted-foreground">{t("groups.pastHint")}</p>
          <CourseGrid courses={courses} errors={errors} />
        </CollapsibleContent>
      </section>
    </Collapsible>
  );
}

function CourseGrid({
  courses,
  errors,
  headingLevel = "h4",
}: {
  courses: CourseSummary[];
  errors: Map<string, SourceErrorKind>;
  headingLevel?: "h3" | "h4";
}) {
  return (
    <ul className="grid gap-3 sm:grid-cols-2">
      {courses.map((summary) => (
        <li key={summary.course.id}>
          <CourseCard
            summary={summary}
            sourceError={errors.get(summary.course.source_id) ?? null}
            headingLevel={headingLevel}
          />
        </li>
      ))}
    </ul>
  );
}
