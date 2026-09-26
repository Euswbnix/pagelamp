import { ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CodeBlock } from "@/components/common/CodeBlock";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

/**
 * What to clear the quarantine flag on, given the binary AI apps launch: the whole `.app` when
 * the binary is inside one (the usual case), else just the binary.
 */
export function quarantineTarget(command: string): { path: string; recursive: boolean } {
  const app = /^(.+?\.app)\/Contents\/MacOS\/[^/]+$/.exec(command);
  return app?.[1] ? { path: app[1], recursive: true } : { path: command, recursive: false };
}

/** A path as one POSIX shell word: single quotes, with any ' written as '\''. */
function shellQuote(path: string): string {
  return `'${path.replaceAll("'", `'\\''`)}'`;
}

/**
 * The beta is only ad-hoc signed (not notarized): macOS's own path is "Open Anyway" in
 * System Settings › Privacy & Security; removing the quarantine flag is the fallback, e.g. when
 * the bundled `pagelamp` is blocked as an AI app starts it. `command` is the binary the AI apps
 * launch (every app launches the same one), so the command works wherever the app lives. Shown
 * once on the Connect page, only in production macOS builds (see lib/platform.ts).
 */
export function QuarantineHint({ command }: { command: string }) {
  const { t } = useTranslation("connect");
  const target = quarantineTarget(command);
  const xattr = `xattr -d${target.recursive ? "r" : ""} com.apple.quarantine ${shellQuote(target.path)}`;
  return (
    <Alert role="note">
      <ShieldAlert aria-hidden />
      <AlertTitle>{t("quarantine.title")}</AlertTitle>
      <AlertDescription className="space-y-2">
        <p>{t("quarantine.body")}</p>
        <p>{t("quarantine.fallback")}</p>
        <CodeBlock code={xattr} copyLabel={t("quarantine.copy")} />
      </AlertDescription>
    </Alert>
  );
}
