import { Eye } from "lucide-react";
import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSetCourseHidden } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { useApiErrorText } from "@/lib/useApiErrorText";

/**
 * Un-hide a course straight from its card, so un-hiding never depends on opening the course.
 * Sits above the card's stretched link (z-10) as a sibling, never inside it. After it succeeds
 * the button disappears, so focus moves to the card's link.
 */
export function ShowInListButton({
  courseId,
  courseName,
  linkRef,
}: {
  courseId: string;
  courseName: string;
  linkRef: RefObject<HTMLAnchorElement | null>;
}) {
  const { t } = useTranslation("courses");
  const errorText = useApiErrorText();
  const mutation = useSetCourseHidden();

  async function show() {
    if (mutation.isPending) return;
    try {
      await mutation.mutateAsync({ courseId, hidden: false });
      toast.success(t("card.shownAgain", { course: courseName }));
      linkRef.current?.focus();
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
      onClick={() => void show()}
    >
      <Eye aria-hidden />
      {t("card.showInList")}
    </Button>
  );
}
