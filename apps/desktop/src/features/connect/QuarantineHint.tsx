import { ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { brand } from "@/brand";
import { CodeBlock } from "@/components/common/CodeBlock";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

/**
 * The beta isn't signed: if macOS blocks the bundled `studentos` when an AI app starts it,
 * removing the quarantine flag fixes it. Shown only in production macOS builds
 * (see lib/platform.ts).
 */
export function QuarantineHint() {
  const { t } = useTranslation("connect");
  const command = `xattr -dr com.apple.quarantine "/Applications/${brand.productName}.app"`;
  return (
    <Alert role="note">
      <ShieldAlert aria-hidden />
      <AlertTitle>{t("quarantine.title")}</AlertTitle>
      <AlertDescription className="space-y-2">
        <p>{t("quarantine.body")}</p>
        <CodeBlock code={command} copyLabel={t("quarantine.copy")} />
      </AlertDescription>
    </Alert>
  );
}
