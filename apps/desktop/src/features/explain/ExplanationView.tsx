import { FileText, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApi } from "@/api/context";
import type { Citation, WeeklyExplanation } from "@/api/explain";
import { AiGeneratedLabel, aiGeneratedLabelText } from "@/components/common/AiGeneratedLabel";
import { CopyButton } from "@/components/common/CopyButton";
import { useOpenExternal } from "@/components/common/useOpenExternal";
import { Button } from "@/components/ui/button";
import { inlineMarkdown } from "@/lib/inlineMarkdown";
import { isHttpUrl } from "@/lib/url";

/**
 * One explanation (design §5.2, §7): its sections, each paragraph with its citation chips (the
 * local file, else the LMS page), the check questions, what wasn't read and why, and the
 * AI-generated line, which Copy carries too.
 */
export function ExplanationView({
  explanation,
  onIncludeLeftOut,
  actions,
}: {
  explanation: WeeklyExplanation;
  /** More controls beside Copy (Delete). */
  actions?: ReactNode;
  /** Write again with the left-out materials included (only those the student may add). */
  onIncludeLeftOut?: (materialIds: string[]) => void;
}) {
  const { t, i18n } = useTranslation("explain");
  const { t: tai } = useTranslation("ai");
  const includable = explanation.left_out.filter((m) => m.reason === "over_budget");

  return (
    <article className="space-y-5">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <AiGeneratedLabel meta={explanation.meta} />
        <div className="flex items-center gap-1">
          <CopyButton
            text={copyText(explanation, aiGeneratedLabelText(explanation.meta, tai, i18n.language))}
            label={t("result.copy")}
          />
          {actions}
        </div>
      </div>

      {explanation.sections.map((section, s) => (
        // Sections and paragraphs have no ids; an explanation never changes.
        // biome-ignore lint/suspicious/noArrayIndexKey: fixed list
        <section key={s} className="space-y-2">
          <h4 className="font-medium">{section.heading}</h4>
          {section.paragraphs.map((paragraph, p) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed list
            <div key={p} className="space-y-1.5">
              <p className="pl-prose text-sm">{inlineMarkdown(paragraph.text)}</p>
              <ul aria-label={t("result.sourcesLabel")} className="flex flex-wrap gap-1.5">
                {paragraph.citations.map((citation) => (
                  <li key={`${citation.handle}-${citation.locator ?? ""}`}>
                    <CitationChip citation={citation} />
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </section>
      ))}

      {explanation.dropped_citations > 0 ? (
        <p className="flex items-start gap-2 text-sm text-muted-foreground">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
          {t("result.droppedCitations", { count: explanation.dropped_citations })}
        </p>
      ) : null}

      {explanation.check_questions.length > 0 ? (
        <section className="space-y-2">
          <h4 className="font-medium">{t("result.questions")}</h4>
          <ol className="list-decimal space-y-1 pl-5 text-sm">
            {explanation.check_questions.map((question) => (
              <li key={question}>{inlineMarkdown(question)}</li>
            ))}
          </ol>
        </section>
      ) : null}

      {explanation.left_out.length > 0 ? (
        <section className="space-y-2">
          <h4 className="text-sm font-medium">{t("result.leftOut")}</h4>
          <ul className="space-y-1 text-sm">
            {explanation.left_out.map((material) => (
              <li key={material.material_id}>
                {material.title}
                <span className="text-muted-foreground">
                  {" "}
                  ({t(`result.leftOutReason.${material.reason}`)})
                </span>
              </li>
            ))}
          </ul>
          {onIncludeLeftOut && includable.length > 0 ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => onIncludeLeftOut(includable.map((m) => m.material_id))}
            >
              {t("result.includeAgain")}
            </Button>
          ) : null}
        </section>
      ) : null}

      {explanation.cite_ai_use ? (
        <p className="text-sm text-muted-foreground">{t("result.citeAiUse")}</p>
      ) : null}
    </article>
  );
}

/** "Title, locator": opens the material's local file, else its LMS page. */
function CitationChip({ citation }: { citation: Citation }) {
  const { t } = useTranslation("explain");
  const api = useApi();
  const openExternal = useOpenExternal();
  const label = citation.locator
    ? t("result.citation", { title: citation.title, locator: citation.locator })
    : citation.title;

  async function open() {
    const opened = await api.openMaterial(citation.material_id).catch(() => false);
    if (opened) return;
    if (citation.url && isHttpUrl(citation.url)) openExternal(citation.url);
    else toast.info(t("result.notOnComputer", { title: citation.title }));
  }

  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      className="h-auto rounded-full px-2.5 py-0.5 text-xs font-normal"
      onClick={() => void open()}
    >
      <FileText aria-hidden />
      {label}
    </Button>
  );
}

/** Plain text for the clipboard: the sections with their sources, the questions, the label. */
function copyText(explanation: WeeklyExplanation, label: string): string {
  const parts: string[] = [];
  for (const section of explanation.sections) {
    parts.push(section.heading);
    for (const paragraph of section.paragraphs) {
      const sources = paragraph.citations
        .map((c) => (c.locator ? `${c.title}, ${c.locator}` : c.title))
        .join("; ");
      parts.push(sources ? `${paragraph.text} [${sources}]` : paragraph.text);
    }
  }
  if (explanation.check_questions.length > 0) {
    parts.push(explanation.check_questions.map((q, i) => `${i + 1}. ${q}`).join("\n"));
  }
  parts.push(label);
  return parts.join("\n\n");
}
