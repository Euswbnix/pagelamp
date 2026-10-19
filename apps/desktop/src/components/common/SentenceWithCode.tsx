import { WHEN } from "./SentenceWithTime";

/**
 * Pass this as the code's value to t(); <SentenceWithCode> swaps it for a <code> element. The
 * same marker as <SentenceWithTime>'s: the interpolation lets it through as a whole value only,
 * never inside one (i18n/safeValues.ts), so nothing a value holds can stand in for it.
 */
export const CODE = WHEN;

/**
 * Renders a translated sentence containing a command (or any code) in code style, in any word
 * order. The sentence stays text: no part of it, and none of its values, is read as markup.
 *
 *   <SentenceWithCode text={t("sameSession.sourceCalendar", { command: CODE })} code="pagelamp course copy-dates" />
 */
export function SentenceWithCode({ text, code }: { text: string; code: string }) {
  const [before, ...after] = text.split(CODE);
  return (
    <>
      {before}
      {after.length > 0 ? <code className="font-mono text-[0.9em]">{code}</code> : null}
      {after.join("")}
    </>
  );
}
