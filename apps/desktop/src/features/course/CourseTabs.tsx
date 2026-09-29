import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { CourseOverview } from "@/api/types";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { DeadlinesTab } from "./deadlines/DeadlinesTab";
import { PolicyTab } from "./policy/PolicyTab";
import { SettingsTab } from "./settings/SettingsTab";
import { TimelineTab } from "./timeline/TimelineTab";
import { COURSE_TABS, isCourseTab, useCourseTab } from "./useCourseParams";
import { WeekTab } from "./week/WeekTab";

// Radix makes each panel focusable (Tab from the tab list lands on it), so show where focus is.
const PANEL = "rounded-lg pt-4 focus-visible:ring-[3px] focus-visible:ring-ring/50";

/**
 * The five course tabs. The selected tab is kept in `?tab=` (see useCourseParams).
 * Tabs with a form stay mounted while hidden, so unsaved edits survive switching tabs.
 */
export function CourseTabs({ overview }: { overview: CourseOverview }) {
  const { t } = useTranslation("course");
  const [tab, setTab] = useCourseTab();
  const { course } = overview;

  // "Set term dates" (This week tab) disappears with its tab. Move focus to the Timeline
  // panel it opens, so keyboard and screen-reader users aren't dropped at the top of the page.
  const timelinePanel = useRef<HTMLDivElement>(null);
  const focusTimeline = useRef(false);
  useEffect(() => {
    if (tab === "timeline" && focusTimeline.current) {
      focusTimeline.current = false;
      timelinePanel.current?.focus();
    }
  }, [tab]);
  function openTermDates() {
    focusTimeline.current = true;
    setTab("timeline");
  }

  return (
    <Tabs value={tab} onValueChange={(value) => isCourseTab(value) && setTab(value)}>
      <TabsList aria-label={t("tabs.label")} className="max-w-full overflow-x-auto">
        {COURSE_TABS.map((value) => (
          <TabsTrigger key={value} value={value} className="px-3">
            {t(`tabs.${value}`)}
          </TabsTrigger>
        ))}
      </TabsList>
      <TabsContent value="week" className={PANEL}>
        <WeekTab overview={overview} onSetTermDates={openTermDates} />
      </TabsContent>
      <TabsContent
        ref={timelinePanel}
        value="timeline"
        className={PANEL}
        forceMount
        hidden={tab !== "timeline"}
      >
        <TimelineTab course={course} timeline={overview.timeline} lifecycle={overview.lifecycle} />
      </TabsContent>
      <TabsContent value="deadlines" className={PANEL}>
        <DeadlinesTab courseId={course.id} />
      </TabsContent>
      <TabsContent value="policy" className={PANEL} forceMount hidden={tab !== "policy"}>
        <PolicyTab course={course} aiMaterials={overview.ai_materials} />
      </TabsContent>
      <TabsContent value="settings" className={PANEL}>
        <SettingsTab course={course} lifecycle={overview.lifecycle} />
      </TabsContent>
    </Tabs>
  );
}
