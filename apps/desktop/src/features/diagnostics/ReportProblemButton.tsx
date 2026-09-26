import { ExternalLink as ExternalLinkIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { brand } from "@/brand";
import { useOpenExternal } from "@/components/common/useOpenExternal";
import { Button } from "@/components/ui/button";
import { isHttpUrl } from "@/lib/url";

/**
 * Opens the brand's "report a problem" page (a new-issue form) in the browser. The URL is fixed:
 * report data is never put in it — the student pastes the report themselves. Renders nothing
 * when the brand has no such link.
 */
export function ReportProblemButton({ size = "sm" }: { size?: "sm" | "default" }) {
  const { t } = useTranslation();
  const openExternal = useOpenExternal();
  const href = brand.links.issues;
  if (!isHttpUrl(href)) return null;
  const onGitHub = new URL(href).hostname === "github.com";
  return (
    <Button asChild variant="outline" size={size}>
      <a
        href={href}
        target="_blank"
        rel="noreferrer noopener"
        onClick={(event) => {
          // The desktop webview never navigates away; the link opens in the browser.
          event.preventDefault();
          openExternal(href);
        }}
      >
        <ExternalLinkIcon aria-hidden />
        {onGitHub ? t("diagnostics.reportProblemOnGitHub") : t("diagnostics.reportProblem")}
      </a>
    </Button>
  );
}
