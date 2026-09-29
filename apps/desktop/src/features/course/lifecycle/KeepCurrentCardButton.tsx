import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useKeepCourseCurrent } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { useApiErrorText } from "@/lib/useApiErrorText";

/**
 * "I'm still taking this" on a Past course's card (the facade's default date). Sits above the
 * card's stretched link (z-10) as a sibling, never inside it. The card then moves to Current,
 * so focus follows it there (its link carries `data-course-id`).
 */
export function KeepCurrentCardButton({
  courseId,
  courseName,
}: {
  courseId: string;
  courseName: string;
}) {
  const { t } = useTranslation("calendar");
  const errorText = useApiErrorText();
  const mutation = useKeepCourseCurrent();

  async function keep() {
    if (mutation.isPending) return;
    try {
      await mutation.mutateAsync({ courseId, until: null });
      toast.success(t("keep.keptCourse", { course: courseName }));
      requestAnimationFrame(() =>
        document.querySelector<HTMLElement>(`a[data-course-id="${CSS.escape(courseId)}"]`)?.focus(),
      );
    } catch (error) {
      toast.error(errorText(error));
    }
  }

  return (
    <Button
      type="button"
      size="xs"
      variant="outline"
      className="relative z-10 aria-disabled:opacity-50"
      aria-disabled={mutation.isPending || undefined}
      onClick={() => void keep()}
    >
      {t("keep.button")}
    </Button>
  );
}
