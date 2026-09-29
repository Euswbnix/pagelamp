// Interpolated values are data, never template: a course, material or deadline title comes from
// the LMS (instructor-controlled), and i18next would otherwise act on markup inside it. Its
// interpolator replaces the FIRST occurrence of each placeholder's text, so a title containing
// "{{when}}" received the date meant for the template's own {{when}} — and `skipOnVariables`
// doesn't prevent that. So every value is neutralised on the way in (i18next's `escape` hook,
// called for each value) and restored after interpolation and nesting (a post-processor):
// "{{" and "$t(" become private-use characters in between, which nothing in a template matches.
// A NUL inside a value is replaced too: <SentenceWithTime> splits the sentence on it.
// HTML needs no escaping here: React escapes text, and no translation is rendered as HTML.

import type { PostProcessorModule } from "i18next";

const OPEN = ""; // stands in for "{{"
const NEST = ""; // stands in for "$t("

/** <SentenceWithTime>'s WHEN marker: passed on as-is, but never accepted inside other text. */
const MARKER = "\u0000";

/** i18next `interpolation.escape`: applied to every interpolated value. */
export function neutralize(value: unknown): string {
  const text = String(value);
  if (text === MARKER) return text;
  return text.replaceAll(MARKER, "\uFFFD").replaceAll("{{", OPEN).replaceAll("$t(", NEST);
}

/** Puts the value's own "{{" and "$t(" back once the template is done. */
export const restoreValues: PostProcessorModule = {
  type: "postProcessor",
  name: "restoreValues",
  process: (value: string) => value.replaceAll(OPEN, "{{").replaceAll(NEST, "$t("),
};
