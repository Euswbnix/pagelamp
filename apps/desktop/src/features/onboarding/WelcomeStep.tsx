import { ArrowRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AiDisclosure } from "@/components/common/AiDisclosure";
import { PageHeader } from "@/components/common/PageHeader";
import { Button } from "@/components/ui/button";

/** Step 1: what the product does (and doesn't), the AI disclosure, and a way out. */
export function WelcomeStep({ onStart, onSkip }: { onStart: () => void; onSkip: () => void }) {
  const { t } = useTranslation("onboarding");
  return (
    <div>
      <PageHeader title={t("welcome.title")} />
      <div className="space-y-6">
        <p className="text-base leading-relaxed text-pretty">{t("welcome.body")}</p>
        <AiDisclosure />
        <div className="flex flex-wrap items-center gap-3">
          <Button size="lg" onClick={onStart}>
            {t("welcome.start")}
            <ArrowRight aria-hidden />
          </Button>
          <Button size="lg" variant="ghost" className="text-muted-foreground" onClick={onSkip}>
            {t("welcome.skip")}
          </Button>
        </div>
      </div>
    </div>
  );
}
