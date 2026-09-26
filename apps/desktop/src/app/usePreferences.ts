import { useEffect } from "react";
import { brand } from "@/brand";
import i18n from "@/i18n";
import { useUiStore } from "@/stores/ui";

/** Keeps <html class="dark">, <html lang> and i18next in sync with the stored preferences. */
export function usePreferences() {
  const theme = useUiStore((s) => s.theme);
  const locale = useUiStore((s) => s.locale) ?? brand.defaultLocale;

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && media.matches);
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.style.colorScheme = dark ? "dark" : "light";
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);

  useEffect(() => {
    document.documentElement.lang = locale;
    if (i18n.language !== locale) void i18n.changeLanguage(locale);
  }, [locale]);
}
