import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SourceRecord } from "@/api/types";
import { PageHeader } from "@/components/common/PageHeader";
import { Button } from "@/components/ui/button";
import { AddSource } from "@/features/sources/add/AddSource";

/** Step 2: pick folder + calendar feed (recommended) or a Canvas token, and add it. */
export function ChooseSourceStep({
  onBack,
  onAdded,
}: {
  onBack: () => void;
  onAdded: (records: SourceRecord[]) => void;
}) {
  const { t } = useTranslation("onboarding");
  const { t: tc } = useTranslation();
  return (
    <div>
      <PageHeader title={t("source.title")} description={t("source.description")} />
      <AddSource
        submitLabel={t("source.submit")}
        onAdded={onAdded}
        footerStart={
          <Button type="button" variant="ghost" onClick={onBack}>
            <ArrowLeft aria-hidden />
            {tc("actions.back")}
          </Button>
        }
      />
    </div>
  );
}
