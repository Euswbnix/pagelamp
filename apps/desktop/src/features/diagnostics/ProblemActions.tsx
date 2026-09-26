import { DiagnosticReportButton } from "./DiagnosticReportButton";
import { ReportProblemButton } from "./ReportProblemButton";

/** "Copy diagnostic report…" + "Report a problem", under an error that stops the app working. */
export function ProblemActions() {
  return (
    <div className="flex flex-wrap items-center justify-center gap-2">
      <DiagnosticReportButton />
      <ReportProblemButton />
    </div>
  );
}
