// Interpolated values are data, never template: a course, material or deadline title comes from
// the LMS (instructor-controlled), and i18next would otherwise act on markup inside it. Its
// interpolator replaces the FIRST occurrence of each placeholder's text, so a title containing
// "{{when}}" received the date meant for the template's own {{when}} — and `skipOnVariables`
// doesn't prevent that. So every value is neutralised on the way in (i18next's `escape` hook,
// called for each value) and restored after interpolation and nesting (a post-processor):
// "{{" and "$t(" become private-use characters in between, which nothing in a template matches.
// A marker's character inside a value is replaced too: <SentenceWithTime> and
// <SentenceWithCode> split the sentence on it (SlotMarker below).
// HTML needs no escaping here: React escapes text, and no translation is rendered as HTML.

import type { PostProcessorModule } from "i18next";

const OPEN = ""; // stands in for "{{"
const NEST = ""; // stands in for "$t("

/**
 * A slot a component fills in a translated sentence (<SentenceWithTime>'s time,
 * <SentenceWithCode>'s code): passed to t() as a value, it puts its character in the sentence,
 * where the component splits it. Only a marker can: the same character inside any other value,
 * or as the whole of one (a course code from the LMS can be anything), becomes U+FFFD. With
 * `useRawValueToEscape` the escape hook sees the marker itself, not its text.
 */
export class SlotMarker {
  constructor(readonly char: string) {}
  toString(): string {
    return this.char;
  }
}

/** Every character a marker stands for (WHEN, WHEN_2). */
const SLOT_CHARS = ["\u0000", "\u0001"];

/** i18next `interpolation.escape`: applied to every interpolated value. */
export function neutralize(value: unknown): string {
  if (value instanceof SlotMarker) return value.char;
  const text = SLOT_CHARS.reduce((t, char) => t.replaceAll(char, "\uFFFD"), String(value));
  return text.replaceAll("{{", OPEN).replaceAll("$t(", NEST);
}

/** Puts the value's own "{{" and "$t(" back once the template is done. */
export const restoreValues: PostProcessorModule = {
  type: "postProcessor",
  name: "restoreValues",
  process: (value: string) => value.replaceAll(OPEN, "{{").replaceAll(NEST, "$t("),
};
