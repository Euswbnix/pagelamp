// UI preferences (theme, language, list toggles). Persisted to localStorage because they are
// harmless conveniences. NEVER put tokens, feed URLs or course content in this store.

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { Locale } from "@/brand";

export type ThemePreference = "system" | "light" | "dark";

interface UiState {
  theme: ThemePreference;
  /** null = the brand's default language. */
  locale: Locale | null;
  showHiddenCourses: boolean;
  /** The student dismissed onboarding without adding a source. */
  onboardingSkipped: boolean;
  /** When the student confirmed the AI disclosure (ISO instant), null = not yet. */
  aiDisclosureAcknowledgedAt: string | null;
  setTheme: (theme: ThemePreference) => void;
  setLocale: (locale: Locale) => void;
  setShowHiddenCourses: (show: boolean) => void;
  setOnboardingSkipped: (skipped: boolean) => void;
  acknowledgeAiDisclosure: () => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      theme: "system",
      locale: null,
      showHiddenCourses: false,
      onboardingSkipped: false,
      aiDisclosureAcknowledgedAt: null,
      setTheme: (theme) => set({ theme }),
      setLocale: (locale) => set({ locale }),
      setShowHiddenCourses: (showHiddenCourses) => set({ showHiddenCourses }),
      setOnboardingSkipped: (onboardingSkipped) => set({ onboardingSkipped }),
      acknowledgeAiDisclosure: () => set({ aiDisclosureAcknowledgedAt: new Date().toISOString() }),
    }),
    {
      name: "studentos.ui",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      // Explicit allow-list: only these keys are ever written to disk.
      partialize: (s) => ({
        theme: s.theme,
        locale: s.locale,
        showHiddenCourses: s.showHiddenCourses,
        onboardingSkipped: s.onboardingSkipped,
        aiDisclosureAcknowledgedAt: s.aiDisclosureAcknowledgedAt,
      }),
    },
  ),
);
