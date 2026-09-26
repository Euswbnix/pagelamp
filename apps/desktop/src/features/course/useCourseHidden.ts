import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSetCourseHidden } from "@/api/queries";
import type { Course } from "@/api/types";
import { useApiErrorText } from "@/lib/useApiErrorText";

/**
 * Hide/show a course with a confirmation toast. Shared by the header's "Show in course list"
 * button and the Settings tab switch.
 */
export function useCourseHidden(course: Course) {
  const { t } = useTranslation("course");
  const errorText = useApiErrorText();
  const mutation = useSetCourseHidden();

  async function setHidden(hidden: boolean) {
    // mutateAsync (not mutate + callbacks): the button that triggered this may unmount when the
    // course refreshes, and per-call callbacks are skipped for unmounted components.
    try {
      await mutation.mutateAsync({ courseId: course.id, hidden });
      toast.success(
        hidden
          ? t("settings.hiddenToast", { course: course.name })
          : t("settings.shownToast", { course: course.name }),
      );
    } catch (error) {
      toast.error(errorText(error));
    }
  }

  return {
    setHidden,
    isPending: mutation.isPending,
    /** The value being saved right now, for instant feedback on the switch. */
    pendingValue: mutation.isPending ? mutation.variables?.hidden : undefined,
  };
}
