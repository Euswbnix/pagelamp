import { type LucideIcon, Monitor, Moon, Sun } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { brand, SUPPORTED_LOCALES } from "@/brand";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { isSupportedLocale } from "@/i18n";
import { type ThemePreference, useUiStore } from "@/stores/ui";
import { SettingsSection } from "./SettingsSection";
import { TransparencySettings } from "./TransparencySettings";

const THEMES: { value: ThemePreference; icon: LucideIcon }[] = [
  { value: "system", icon: Monitor },
  { value: "light", icon: Sun },
  { value: "dark", icon: Moon },
];

function isTheme(value: string): value is ThemePreference {
  return THEMES.some((theme) => theme.value === value);
}

/** Theme and language. Both are stored in useUiStore; usePreferences applies them app-wide. */
export function AppearanceSection() {
  const { t } = useTranslation("settings");
  const { t: tc } = useTranslation();
  const theme = useUiStore((s) => s.theme);
  const setTheme = useUiStore((s) => s.setTheme);
  const locale = useUiStore((s) => s.locale) ?? brand.defaultLocale;
  const setLocale = useUiStore((s) => s.setLocale);
  const themeLabelId = useId();
  const themeHintId = useId();
  const languageId = useId();

  return (
    <SettingsSection title={t("appearance.title")}>
      <div className="space-y-2">
        <div id={themeLabelId} className="text-sm font-medium">
          {t("appearance.theme")}
        </div>
        <ToggleGroup
          type="single"
          variant="outline"
          spacing={0}
          value={theme}
          // A single ToggleGroup reports "" when the active item is clicked again; ignore that.
          onValueChange={(value) => {
            if (isTheme(value)) setTheme(value);
          }}
          aria-labelledby={themeLabelId}
          aria-describedby={themeHintId}
        >
          {THEMES.map(({ value, icon: Icon }) => (
            <ToggleGroupItem key={value} value={value} className="px-3">
              <Icon aria-hidden />
              {t(`appearance.themeOption.${value}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <p id={themeHintId} className="text-sm text-muted-foreground">
          {t("appearance.themeHint")}
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor={languageId}>{t("appearance.language")}</Label>
        <Select
          value={locale}
          onValueChange={(value) => {
            if (isSupportedLocale(value)) setLocale(value);
          }}
        >
          <SelectTrigger id={languageId} className="w-48">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SUPPORTED_LOCALES.map((option) => (
              // Each language is named in itself, so `lang` helps screen readers pronounce it.
              // It sits on the inner span so the closed trigger (SelectValue) keeps it too.
              <SelectItem key={option} value={option}>
                <span lang={option}>{tc(`language.${option}`)}</span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <TransparencySettings />
    </SettingsSection>
  );
}
