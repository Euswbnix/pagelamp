import { ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { DateEvidence } from "@/api/types";
import { useOpenExternal } from "@/components/common/useOpenExternal";
import { Button } from "@/components/ui/button";
import { isHttpUrl } from "@/lib/url";

/**
 * Where a date comes from: the material's own words (plain text, never Markdown or links),
 * its title and place ("p. 2"), and a way to open it (calendar design §7.10).
 */
export function QuoteCard({ evidence }: { evidence: DateEvidence }) {
  const { t } = useTranslation("proposals");
  const openExternal = useOpenExternal();
  const source = evidence.locator
    ? t("card.sourceAt", { title: evidence.title, locator: evidence.locator })
    : t("card.source", { title: evidence.title });
  return (
    <figure className="space-y-1 rounded-md border-l-2 border-primary/40 bg-muted/40 px-3 py-2">
      {evidence.quote ? (
        <blockquote className="text-sm whitespace-pre-wrap">“{evidence.quote}”</blockquote>
      ) : null}
      <figcaption className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
        <cite className="not-italic">{source}</cite>
        {evidence.derived ? <span>· {t("card.derived")}</span> : null}
        {isHttpUrl(evidence.url) ? (
          <Button
            variant="link"
            size="xs"
            className="h-auto p-0"
            onClick={() => openExternal(evidence.url ?? "")}
          >
            <ExternalLink aria-hidden />
            {t("card.open", { title: evidence.title })}
          </Button>
        ) : null}
      </figcaption>
    </figure>
  );
}
