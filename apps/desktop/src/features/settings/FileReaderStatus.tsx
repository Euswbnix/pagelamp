import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useDoctor } from "@/api/queries";
import type { ExtractWorkerStatus, UnreadableFiles } from "@/api/types";
import { Alert, AlertDescription } from "@/components/ui/alert";

/**
 * The file reader (the extraction worker, v0.3 M0.5): says nothing while it works, warns when
 * it can't run (often security software), and counts the files that couldn't be read by why.
 */
export function FileReaderStatus() {
  const doctor = useDoctor();
  if (!doctor.data) return null;
  const { status } = doctor.data.extract_worker;
  const unreadable = doctor.data.unreadable_files.filter((entry) => entry.count > 0);
  if (status === "ok" && unreadable.length === 0) return null;
  return (
    <div className="space-y-2">
      {status === "ok" ? null : <ReaderWarning status={status} />}
      {unreadable.length > 0 ? <UnreadableLine entries={unreadable} /> : null}
    </div>
  );
}

function ReaderWarning({ status }: { status: ExtractWorkerStatus }) {
  const { t } = useTranslation("settings");
  return (
    <Alert role="note" className="*:[svg]:text-warning">
      <TriangleAlert aria-hidden />
      <AlertDescription className="text-foreground">
        {status === "protocol_mismatch"
          ? t("help.fileReader.mismatch")
          : t("help.fileReader.blocked")}
      </AlertDescription>
    </Alert>
  );
}

function UnreadableLine({ entries }: { entries: UnreadableFiles[] }) {
  const { t } = useTranslation("settings");
  const list = entries
    .map(({ kind, count }) => t(`help.fileReader.kinds.${kind}`, { count }))
    .join(t("help.fileReader.separator"));
  return (
    <p className="text-sm text-muted-foreground">{t("help.fileReader.unreadable", { list })}</p>
  );
}
