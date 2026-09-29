import { Download } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useCourseCalendar,
  useDownloadMaterialFiles,
  useSetCalendarSources,
} from "@/api/proposalQueries";
import type { CalendarCandidate, Course } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { useApiErrorText } from "@/lib/useApiErrorText";

/**
 * "Materials to read dates from" (calendar design §7.1, D46): the candidates the facade picked,
 * why each one, whether it has text, the student's include/exclude, and an explicit, disclosed
 * download for an outline file that isn't downloaded. (Why AI reading can't run is said next to
 * the button, in ReadSyllabus.)
 */
export function SyllabusSources({ course }: { course: Course }) {
  const { t } = useTranslation("proposals");
  const headingId = useId();
  const view = useCourseCalendar(course.id);
  const data = view.data;
  if (!data) return null;

  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <div className="space-y-1">
        <h3 id={headingId} className="text-sm font-medium">
          {t("sources.title")}
        </h3>
        <p className="text-sm text-muted-foreground">{t("sources.description")}</p>
      </div>
      {data.candidates.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("sources.none")}</p>
      ) : (
        <ul aria-label={t("sources.listLabel")} className="divide-y rounded-lg border">
          {data.candidates.map((candidate) => (
            <CandidateRow key={candidate.material_id} course={course} candidate={candidate} />
          ))}
        </ul>
      )}
    </section>
  );
}

function CandidateRow({ course, candidate }: { course: Course; candidate: CalendarCandidate }) {
  const { t } = useTranslation("proposals");
  const errorText = useApiErrorText();
  const setSources = useSetCalendarSources();
  const download = useDownloadMaterialFiles();
  const [pendingChoice, setPendingChoice] = useState<boolean | null>(null);
  const id = useId();
  const status = candidate.left_out
    ? t(`sources.leftOut.${candidate.left_out}`)
    : candidate.included
      ? t("sources.included")
      : null;
  // Only a material with text can be read; the others wait for a download.
  const canToggle = candidate.has_text;
  const checked = pendingChoice ?? candidate.included;

  async function toggle(include: boolean) {
    if (setSources.isPending) return;
    setPendingChoice(include);
    try {
      await setSources.mutateAsync({
        courseId: course.id,
        include: include ? [candidate.material_id] : [],
        exclude: include ? [] : [candidate.material_id],
      });
      toast.success(
        include
          ? t("sources.readded", { title: candidate.title })
          : t("sources.excluded", { title: candidate.title }),
      );
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setPendingChoice(null);
    }
  }

  async function downloadIt() {
    if (download.isPending) return;
    try {
      await download.mutateAsync({ courseId: course.id, materialIds: [candidate.material_id] });
      toast.success(t("sources.downloaded", { title: candidate.title }));
    } catch (error) {
      toast.error(errorText(error));
    }
  }

  return (
    <li className="flex flex-wrap items-start justify-between gap-3 p-3 text-sm">
      <div className="flex min-w-0 items-start gap-3">
        <Checkbox
          id={id}
          checked={checked}
          disabled={!canToggle}
          onCheckedChange={(value) => void toggle(value === true)}
          aria-describedby={`${id}-why`}
          className="mt-0.5"
        />
        <div className="min-w-0 space-y-0.5">
          <Label htmlFor={id} className="font-medium">
            {candidate.title}
          </Label>
          <p id={`${id}-why`} className="text-xs text-muted-foreground">
            {t(`sources.reason.${candidate.reason}`)}
            {status ? ` · ${status}` : null}
          </p>
        </div>
      </div>
      {candidate.downloadable ? (
        <Button
          size="sm"
          variant="outline"
          aria-label={t("sources.downloadLabel", { title: candidate.title })}
          aria-disabled={download.isPending || undefined}
          aria-busy={download.isPending || undefined}
          onClick={() => void downloadIt()}
        >
          {download.isPending ? <Spinner aria-hidden /> : <Download aria-hidden />}
          {download.isPending ? t("sources.downloading") : t("sources.download")}
        </Button>
      ) : null}
    </li>
  );
}
