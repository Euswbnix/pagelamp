import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/api/context";
import { brand } from "@/brand";
import { ExternalLink } from "@/components/common/ExternalLink";
import { DiagnosticReportButton } from "@/features/diagnostics/DiagnosticReportButton";
import { ReportProblemButton } from "@/features/diagnostics/ReportProblemButton";
import { isHttpUrl } from "@/lib/url";
import { RevealFolderButton } from "./RevealFolderButton";
import { SettingsSection } from "./SettingsSection";

/** Help & feedback: the diagnostic report, the logs folder and where to report a problem. */
export function HelpSection() {
  const { t } = useTranslation("settings");
  const api = useApi();
  const { help, issues } = brand.links;
  return (
    <SettingsSection title={t("help.title")} description={t("help.description")}>
      <ul className="space-y-4">
        <HelpRow hint={t("help.reportHint")}>
          <DiagnosticReportButton />
        </HelpRow>
        <HelpRow hint={t("help.logsHint")}>
          <RevealFolderButton label={t("help.openLogs")} reveal={() => api.revealLogsDir()} />
        </HelpRow>
        {isHttpUrl(issues) ? (
          <HelpRow hint={t("help.issueHint")}>
            <ReportProblemButton />
          </HelpRow>
        ) : null}
      </ul>
      {isHttpUrl(help) ? (
        <p className="text-sm">
          <ExternalLink href={help}>{t("help.helpLink")}</ExternalLink>
        </p>
      ) : null}
    </SettingsSection>
  );
}

function HelpRow({ hint, children }: { hint: string; children: ReactNode }) {
  return (
    <li className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
      <p className="min-w-0 flex-1 basis-64 text-sm text-muted-foreground">{hint}</p>
      <div className="shrink-0">{children}</div>
    </li>
  );
}
