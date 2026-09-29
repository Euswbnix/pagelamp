// Type-checks translation keys against the English files: `t("nav.courses")` compiles,
// `t("nav.typo")` does not. Add a line here when you add a new namespace file.

import "i18next";
import type ai from "./locales/en/ai.json";
import type common from "./locales/en/common.json";
import type connect from "./locales/en/connect.json";
import type course from "./locales/en/course.json";
import type courses from "./locales/en/courses.json";
import type onboarding from "./locales/en/onboarding.json";
import type settings from "./locales/en/settings.json";
import type sources from "./locales/en/sources.json";
import type updates from "./locales/en/updates.json";

declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "common";
    resources: {
      ai: typeof ai;
      common: typeof common;
      connect: typeof connect;
      course: typeof course;
      courses: typeof courses;
      onboarding: typeof onboarding;
      settings: typeof settings;
      sources: typeof sources;
      updates: typeof updates;
    };
    returnNull: false;
  }
}
