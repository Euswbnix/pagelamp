import { Languages } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import type { Locale } from "@/brand";
import { BrandMark } from "@/components/common/BrandMark";
import { Button } from "@/components/ui/button";
import { focusPageHeading } from "@/lib/focus";
import { paths } from "@/lib/routes";
import { useUiStore } from "@/stores/ui";
import { ChooseSourceStep } from "./ChooseSourceStep";
import { FirstSyncStep } from "./FirstSyncStep";
import { StepIndicator, type StepName } from "./StepIndicator";
import { WelcomeStep } from "./WelcomeStep";

/**
 * First-run setup, full window (no sidebar): welcome → add a source → first sync.
 * The add-source forms and the progress panel are shared with Sources & sync.
 */
export function OnboardingPage() {
  const navigate = useNavigate();
  const setOnboardingSkipped = useUiStore((s) => s.setOnboardingSkipped);
  const [step, setStep] = useState<StepName>("welcome");
  const shownStep = useRef(step);

  // On every step change (not the first render), move focus to the new step's heading so
  // keyboard and screen-reader users start reading from the top.
  useEffect(() => {
    if (shownStep.current === step) return;
    shownStep.current = step;
    focusPageHeading();
  }, [step]);

  function skip() {
    setOnboardingSkipped(true);
    navigate(paths.courses);
  }

  return (
    <main id="main" className="mx-auto flex min-h-dvh max-w-2xl flex-col gap-8 px-6 py-10">
      <div className="flex items-center justify-between gap-4">
        <BrandMark />
        <LanguageToggle />
      </div>
      <StepIndicator current={step} />
      {step === "welcome" ? <WelcomeStep onStart={() => setStep("source")} onSkip={skip} /> : null}
      {step === "source" ? (
        <ChooseSourceStep onBack={() => setStep("welcome")} onAdded={() => setStep("sync")} />
      ) : null}
      {step === "sync" ? <FirstSyncStep onBack={() => setStep("source")} /> : null}
    </main>
  );
}

/** Onboarding has no Settings screen nearby, so offer the other language right here. */
function LanguageToggle() {
  const { t, i18n } = useTranslation();
  const setLocale = useUiStore((s) => s.setLocale);
  const other: Locale = i18n.language === "zh-CN" ? "en" : "zh-CN";
  return (
    <Button variant="ghost" size="sm" onClick={() => setLocale(other)} lang={other}>
      <Languages aria-hidden />
      {t(`language.${other}`)}
    </Button>
  );
}
