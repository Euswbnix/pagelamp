import { CloudDownload } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSources } from "@/api/queries";
import type { Course, MaterialView } from "@/api/types";
import { Alert, AlertDescription } from "@/components/ui/alert";

/**
 * Canvas files are listed but not downloaded until the student asks, because a download
 * through Canvas can count as viewing the file (module "must view" requirements). When the
 * week on screen has such files, this says so and points to "Download files…" in the course
 * header (which downloads the whole course).
 */
export function DownloadFilesCallout({
  course,
  materials,
}: {
  course: Course;
  materials: MaterialView[];
}) {
  const { t } = useTranslation("course");
  const sources = useSources();

  const isCanvas = sources.data?.find((s) => s.id === course.source_id)?.kind === "canvas";
  // Locked or too-large files can't be downloaded by asking: don't count (and offer) them.
  const missing = materials.filter(
    (m) => m.kind === "file" && m.text_status === "not_downloaded" && !m.download_blocked,
  ).length;
  if (!isCanvas || missing === 0) return null;

  return (
    <Alert role="note">
      <CloudDownload aria-hidden />
      <AlertDescription>
        {t("download.callout", { count: missing })} {t("download.calloutHint")}
      </AlertDescription>
    </Alert>
  );
}
