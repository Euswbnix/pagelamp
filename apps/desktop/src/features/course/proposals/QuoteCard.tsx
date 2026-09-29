import { ExternalLink, FileText, FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi } from "@/api/context";
import type { DateEvidence } from "@/api/types";
import { useOpenExternal } from "@/components/common/useOpenExternal";
import { Button } from "@/components/ui/button";
import { isHttpUrl } from "@/lib/url";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { translateWithText } from "../timeline/evidence";

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
    <figure className="pl-concentric space-y-1 border-l-2 border-rule bg-muted px-3 py-2 [--pl-pad:1.25rem]">
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
        ) : (
          // A material on this computer (a course folder, the Canvas download cache): the shell
          // opens it, the page never sees the path.
          <LocalFileButtons materialId={evidence.material_id} title={evidence.title} />
        )}
      </figcaption>
    </figure>
  );
}

function LocalFileButtons({ materialId, title }: { materialId: string; title: string }) {
  const { t } = useTranslation("proposals");
  const api = useApi();
  const errorText = useApiErrorText();
  // The material's title is plain text from the source: never interpolated.
  const named = (key: string) => translateWithText(t, key, {}, { title });
  async function run(open: boolean) {
    try {
      const done = open ? await api.openMaterial(materialId) : await api.revealMaterial(materialId);
      if (!done) toast(named("card.fileUnavailable"));
    } catch (error) {
      toast.error(errorText(error));
    }
  }
  return (
    <>
      <Button
        variant="link"
        size="xs"
        className="h-auto p-0"
        aria-label={named("card.openFileLabel")}
        onClick={() => void run(true)}
      >
        <FileText aria-hidden />
        {t("card.openFile")}
      </Button>
      <Button
        variant="link"
        size="xs"
        className="h-auto p-0"
        aria-label={named("card.revealFileLabel")}
        onClick={() => void run(false)}
      >
        <FolderOpen aria-hidden />
        {t("card.revealFile")}
      </Button>
    </>
  );
}
