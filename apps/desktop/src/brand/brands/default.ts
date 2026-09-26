import type { Brand } from "../types";

/** The plain brand that ships on GitHub. Distributions copy this file; see ../types.ts. */
const brand: Brand = {
  id: "default",
  productName: "PageLamp",
  tagline: {
    en: "A reading lamp for your courses.",
    "zh-CN": "为每门课点一盏读书灯。",
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
    homepage: "https://github.com/Euswbnix/pagelamp",
    issues: "https://github.com/Euswbnix/pagelamp/issues/new/choose",
  },
  aiPolicyHint: {
    en: "Many universities don't allow generative AI in a course unless the instructor permits it — check your syllabus.",
    "zh-CN": "很多大学规定：除非任课老师明确允许，否则课程中不得使用生成式 AI。请以课程大纲为准。",
  },
};

export default brand;
