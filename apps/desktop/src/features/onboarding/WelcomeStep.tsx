import { ArrowRight } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { AiDisclosureAcknowledgement } from "@/components/common/AiDisclosureAcknowledgement";
import { PageHeader } from "@/components/common/PageHeader";
import { Button } from "@/components/ui/button";
import { useUiStore } from "@/stores/ui";

/**
 * Step 1: what the product does (and doesn't), the AI disclosure the student must acknowledge
 * (docs/ARCHITECTURE.md §3 rule 8), and a way out. Skipping needs no acknowledgement: nothing is
 * shared until a source is added — and adding one later asks again.
 */
export function WelcomeStep({ onStart, onSkip }: { onStart: () => void; onSkip: () => void }) {
  const { t } = useTranslation("onboarding");
  const acknowledged = useUiStore((s) => s.aiDisclosureAcknowledgedAt !== null);
  const [nudge, setNudge] = useState(false);
  const hintId = useId();

  function start() {
    // aria-disabled (not disabled) keeps the button focusable, so we can explain instead.
    if (acknowledged) onStart();
    else setNudge(true);
  }

  return (
    <div>
      <PageHeader title={t("welcome.title")} />
      <div className="space-y-6">
        <p className="text-base leading-relaxed text-pretty">{t("welcome.body")}</p>
        <AiDisclosureAcknowledgement />
        <div className="space-y-2">
          <div className="flex flex-wrap items-center gap-3">
            <Button
              size="lg"
              onClick={start}
              aria-disabled={!acknowledged}
              aria-describedby={nudge && !acknowledged ? hintId : undefined}
              className="aria-disabled:opacity-50"
            >
              {t("welcome.start")}
              <ArrowRight aria-hidden />
            </Button>
            <Button size="lg" variant="ghost" className="text-muted-foreground" onClick={onSkip}>
              {t("welcome.skip")}
            </Button>
          </div>
          {nudge && !acknowledged ? (
            <p id={hintId} role="alert" className="text-sm text-destructive">
              {t("welcome.acknowledgeFirst")}
            </p>
          ) : null}
        </div>
      </div>
    </div>
  );
}
