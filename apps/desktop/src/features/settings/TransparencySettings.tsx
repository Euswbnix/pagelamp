import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useUiStore } from "@/stores/ui";

/**
 * Settings ▸ Appearance: "Reduce transparency" on every platform (WebKitGTK can't see the system
 * setting, and some students want it anyway) and, on Linux, "Increase contrast" (§8). Both add
 * to the system's own settings, which the app follows through media queries.
 */
export function TransparencySettings() {
  const { t } = useTranslation("chrome");
  const transparency = useUiStore((s) => s.transparency);
  const setTransparency = useUiStore((s) => s.setTransparency);
  const contrast = useUiStore((s) => s.contrast);
  const setContrast = useUiStore((s) => s.setContrast);
  const linux = document.documentElement.dataset.platform === "linux";

  return (
    <div className="space-y-4">
      <SwitchRow
        label={t("appearance.transparency")}
        hint={t("appearance.transparencyHint")}
        checked={transparency === "reduced"}
        onCheckedChange={(on) => setTransparency(on ? "reduced" : "auto")}
      />
      {linux ? (
        <SwitchRow
          label={t("appearance.contrast")}
          hint={t("appearance.contrastHint")}
          checked={contrast === "more"}
          onCheckedChange={(on) => setContrast(on ? "more" : "auto")}
        />
      ) : null}
    </div>
  );
}

function SwitchRow({
  label,
  hint,
  checked,
  onCheckedChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  const id = useId();
  return (
    <div className="flex items-start justify-between gap-6">
      <div className="grid gap-1">
        <Label htmlFor={id}>{label}</Label>
        <p id={`${id}-hint`} className="text-sm text-muted-foreground">
          {hint}
        </p>
      </div>
      <Switch
        id={id}
        checked={checked}
        onCheckedChange={onCheckedChange}
        aria-describedby={`${id}-hint`}
      />
    </div>
  );
}
