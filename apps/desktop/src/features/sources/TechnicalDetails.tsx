import { ChevronDown } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CopyButton } from "@/components/common/CopyButton";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { translateWithText } from "@/features/course/timeline/evidence";

/**
 * The backend's own text behind a translated error or warning (usually English, in every
 * language): collapsed under "Technical details", selectable, and copyable for a bug report. The
 * line above it (the error's kind, "1 warning") is what the student reads.
 */
export function TechnicalDetails({
  lines,
  subject,
}: {
  lines: string[];
  /** What the details are about (a source's label): names the buttons apart on a page. */
  subject: string;
}) {
  const { t } = useTranslation("sources");
  const unique = [...new Set(lines)];
  if (unique.length === 0) return null;
  const text = unique.join("\n");
  return (
    <Collapsible>
      <CollapsibleTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          className="group -ml-2 text-muted-foreground"
          aria-label={translateWithText(t, "technical.titleFor", {}, { subject })}
        >
          {t("technical.title")}
          <ChevronDown
            className="transition-transform group-aria-expanded:rotate-180 motion-reduce:transition-none"
            aria-hidden
          />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-1.5 pt-1">
        <p className="text-xs text-muted-foreground">{t("technical.note")}</p>
        <div
          lang="en"
          className="rounded-md bg-muted px-2.5 py-2 font-mono text-xs break-words select-text [overflow-wrap:anywhere]"
        >
          {unique.length === 1 ? (
            <p>{unique[0]}</p>
          ) : (
            <ul className="list-disc space-y-1 pl-4">
              {unique.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          )}
        </div>
        <CopyButton
          text={text}
          label={translateWithText(t, "technical.copyFor", {}, { subject })}
          variant="ghost"
        />
      </CollapsibleContent>
    </Collapsible>
  );
}
