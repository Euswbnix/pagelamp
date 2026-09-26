import type { Brand } from "../types";

/** The plain brand that ships on GitHub. Distributions copy this file; see ../types.ts. */
const brand: Brand = {
  id: "default",
  productName: "StudentOS",
  tagline: {
    en: "Your courses, on your computer, ready for the AI app you already use.",
    "zh-CN": "课程资料存在你自己的电脑上，随时交给你正在用的 AI 应用。",
  },
  defaultLocale: "en",
  colors: {
    // A quiet slate blue: readable, not "techy".
    accent: "oklch(0.47 0.08 252)",
    accentForeground: "oklch(0.985 0 0)",
    accentDark: "oklch(0.78 0.07 252)",
    accentDarkForeground: "oklch(0.2 0.02 252)",
  },
  logo: null,
  links: {
    homepage: "https://github.com/Euswbnix/studentos",
    issues: "https://github.com/Euswbnix/studentos/issues",
  },
  aiPolicyHint: {
    en: "Many universities (including U of T) don't allow generative AI in a course unless the instructor permits it — check your syllabus.",
    "zh-CN":
      "很多大学（包括多伦多大学）规定：除非任课老师明确允许，否则课程中不得使用生成式 AI。请以课程大纲为准。",
  },
};

export default brand;
