import { CloudDownload } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSources } from "@/api/queries";
import type { Course, MaterialView } from "@/api/types";
import { Alert, AlertAction, AlertDescription } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { useDownloadCourseFiles, useSyncStore } from "@/stores/sync";

// Warnings listed in the toast; the rest are summed up as "…and N more".
const SHOWN_WARNINGS = 3;

/**
 * Canvas files are listed but not downloaded until the student asks, because a download
 * through Canvas can count as viewing the file (module "must view" requirements). This offers
 * that explicit download for the course — after saying so (the dialog text is required copy).
 */
export function DownloadFilesCallout({
  course,
  materials,
}: {
  course: Course;
  materials: MaterialView[];
}) {
  const { t } = useTranslation("course");
  const { t: tc } = useTranslation();
  const errorText = useApiErrorText();
  const sources = useSources();
  const download = useDownloadCourseFiles();
  const running = useSyncStore((s) => s.running);

  const isCanvas = sources.data?.find((s) => s.id === course.source_id)?.kind === "canvas";
  const missing = materials.filter(
    (m) => m.kind === "file" && m.text_status === "not_downloaded",
  ).length;
  if (!isCanvas || missing === 0) return null;

  async function start() {
    const ran = await download(course.id);
    if (!ran) {
      toast.info(tc("sync.busy"));
      return;
    }
    const { lastSummary, runError } = useSyncStore.getState();
    const result = lastSummary?.results[0];
    // Files that were skipped (too large, locked) are only explained in the run's warnings.
    const warnings = result?.warnings ?? [];
    const description =
      warnings.length > 0 ? (
        <ul className="mt-1 space-y-0.5">
          {warnings.slice(0, SHOWN_WARNINGS).map((warning) => (
            <li key={warning} lang="en">
              {warning}
            </li>
          ))}
          {warnings.length > SHOWN_WARNINGS ? (
            <li>{t("download.moreWarnings", { count: warnings.length - SHOWN_WARNINGS })}</li>
          ) : null}
        </ul>
      ) : undefined;
    if (runError) toast.error(errorText(runError));
    else if (result && !result.ok) toast.error(tc(`sourceError.${result.error_kind ?? "other"}`));
    else if (result?.files_downloaded) {
      toast.success(t("download.done", { count: result.files_downloaded }), { description });
    } else if (warnings.length > 0) toast.warning(t("download.someSkipped"), { description });
    else toast.success(t("download.doneNone"));
  }

  return (
    <Alert role="note">
      <CloudDownload aria-hidden />
      <AlertDescription>{t("download.callout", { count: missing })}</AlertDescription>
      <AlertAction>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button
              size="xs"
              variant="outline"
              aria-disabled={running || undefined}
              className="aria-disabled:opacity-50"
            >
              {running ? t("download.running") : t("download.action")}
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("download.dialogTitle")}</AlertDialogTitle>
              <AlertDialogDescription asChild>
                <div className="space-y-2">
                  <p className="font-medium text-foreground">{t("download.viewingNotice")}</p>
                  <p>{t("download.dialogDetail")}</p>
                </div>
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{tc("actions.cancel")}</AlertDialogCancel>
              <AlertDialogAction disabled={running} onClick={() => void start()}>
                {t("download.confirm")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </AlertAction>
    </Alert>
  );
}
