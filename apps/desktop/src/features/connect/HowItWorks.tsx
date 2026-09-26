import { HardDrive, type LucideIcon, Power, Repeat } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";

const POINTS: { key: "background" | "local" | "once"; icon: LucideIcon }[] = [
  { key: "background", icon: Power },
  { key: "local", icon: HardDrive },
  { key: "once", icon: Repeat },
];

/** Three short facts so students know there's no server or window to keep running. */
export function HowItWorks() {
  const { t } = useTranslation("connect");
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <h2 id={headingId} className="font-heading text-base font-medium">
        {t("howItWorks.title")}
      </h2>
      <ul className="grid gap-3 sm:grid-cols-3">
        {POINTS.map(({ key, icon: Icon }) => (
          <li key={key} className="flex gap-2.5 text-sm text-muted-foreground">
            <Icon className="mt-0.5 size-4 shrink-0 text-foreground" aria-hidden />
            <span>{t(`howItWorks.${key}`)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
