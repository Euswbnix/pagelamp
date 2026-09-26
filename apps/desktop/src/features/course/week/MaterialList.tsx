import { BookMarked, FileText, Link2, type LucideIcon, Megaphone, StickyNote } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { AiMaterialsState, MaterialKind, MaterialView } from "@/api/types";
import { AiMaterialsStatus } from "@/components/common/AiMaterialsStatus";
import { ExternalLink } from "@/components/common/ExternalLink";
import { formatDay } from "@/lib/format";
import { isHttpUrl } from "@/lib/url";
import { TextStatusLabel } from "./TextStatusLabel";

const MATERIAL_ICON: Record<MaterialKind, LucideIcon> = {
  file: FileText,
  page: StickyNote,
  syllabus: BookMarked,
  announcement: Megaphone,
  external_link: Link2,
};

/** Materials of one week, with a "4 of 6 materials readable by your AI app" line. */
export function MaterialList({
  materials,
  aiMaterials,
}: {
  materials: MaterialView[];
  aiMaterials: AiMaterialsState;
}) {
  const { t } = useTranslation("course");
  const headingId = useId();
  const readable = materials.filter((m) => m.text_status === "ok").length;
  return (
    <section aria-labelledby={headingId} className="space-y-2">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 id={headingId} className="text-sm font-medium text-muted-foreground">
          {t("week.materials")}
        </h3>
        <AiMaterialsStatus
          state={aiMaterials}
          indexed={readable}
          total={materials.length}
          className="text-sm text-muted-foreground"
        />
      </div>
      <ul className="divide-y rounded-lg border bg-card">
        {materials.map((material) => (
          <MaterialRow
            key={material.id}
            material={material}
            aiReadable={aiMaterials === "readable"}
          />
        ))}
      </ul>
    </section>
  );
}

function MaterialRow({ material, aiReadable }: { material: MaterialView; aiReadable: boolean }) {
  const { t, i18n } = useTranslation("course");
  const { t: tc } = useTranslation();
  const Icon = MATERIAL_ICON[material.kind];
  const meta = [
    tc(`materialKind.${material.kind}`),
    material.module_name,
    material.published_at
      ? t("week.published", { date: formatDay(material.published_at, i18n.language) })
      : null,
  ].filter(Boolean);

  return (
    <li className="flex items-start gap-3 px-4 py-3">
      <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
      <div className="min-w-0 flex-1 space-y-0.5">
        {/* Only http(s) links open; file:// URLs from course folders stay plain text. */}
        {isHttpUrl(material.url) ? (
          <ExternalLink href={material.url} className="font-medium">
            {material.title}
          </ExternalLink>
        ) : (
          <span className="font-medium">{material.title}</span>
        )}
        <p className="text-xs text-muted-foreground">{meta.join(" · ")}</p>
      </div>
      <div className="flex max-w-[45%] shrink-0 flex-col items-end gap-0.5 text-right">
        <TextStatusLabel
          status={material.text_status}
          chunks={material.chunk_count}
          aiReadable={aiReadable}
        />
        {material.text_error ? (
          <p lang="en" className="text-xs text-muted-foreground">
            {material.text_error}
          </p>
        ) : null}
      </div>
    </li>
  );
}
