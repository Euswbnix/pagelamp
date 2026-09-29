import { useTranslation } from "react-i18next";
import type { BlockReason } from "@/api/ai";

/** The gate reasons with calendar-specific wording (what still works without AI). */
const CALENDAR_BLOCKS = [
  "course_policy_prohibited",
  "course_ai_turned_off",
  "course_hidden",
  "no_readable_materials",
  "material_sharing_not_allowed",
  "disclosure_not_acknowledged",
  "no_model_chosen",
] as const satisfies readonly BlockReason[];
type CalendarBlock = (typeof CALENDAR_BLOCKS)[number];

function isCalendarBlock(block: BlockReason): block is CalendarBlock {
  return (CALENDAR_BLOCKS as readonly BlockReason[]).includes(block);
}

/** Blocks of the course itself: no AI run can help, whatever the settings. */
export const COURSE_BLOCKS: ReadonlySet<BlockReason> = new Set([
  "course_policy_prohibited",
  "course_ai_turned_off",
  "course_hidden",
  "no_readable_materials",
]);

/** Why AI reading can't run: the calendar's wording where it has one, else the AI settings'. */
export function useBlockText(): (block: BlockReason) => string {
  const { t } = useTranslation("proposals");
  const { t: tAi } = useTranslation("ai");
  return (block) =>
    isCalendarBlock(block) ? t(`sources.blocked.${block}`) : tAi(`blocked.${block}`);
}
