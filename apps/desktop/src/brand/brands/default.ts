import type { Brand } from "../types";

/** The plain brand that ships on GitHub. Distributions copy this file; see ../types.ts. */
const brand: Brand = {
  id: "default",
  productName: "Weekmark",
  tagline: {
    en: "Bookmark this week of every course.",
    "zh-CN": "给每门课的这一周，夹上一枚书签。",
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
    homepage: "https://github.com/Euswbnix/weekmark",
    issues: "https://github.com/Euswbnix/weekmark/issues/new/choose",
  },
  aiPolicyHint: {
    en: "Many universities don't allow generative AI in a course unless the instructor permits it — check your syllabus.",
    "zh-CN": "很多大学规定：除非任课老师明确允许，否则课程中不得使用生成式 AI。请以课程大纲为准。",
  },
};

export default brand;
