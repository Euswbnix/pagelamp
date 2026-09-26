import { RelativeTime } from "./RelativeTime";

/** Pass this as `when` to t(); <SentenceWithTime> swaps it for a <RelativeTime>. */
export const WHEN = "\u0000";

/**
 * Renders a translated sentence containing {{when}} with a semantic <time> element in its
 * place, so it works in any word order ("synced 2 hours ago" / "2小时前同步").
 *
 *   <SentenceWithTime text={t("card.next", { title, when: WHEN })} iso={dueAt} />
 */
export function SentenceWithTime({ text, iso }: { text: string; iso: string }) {
  const [before, after = ""] = text.split(WHEN);
  return (
    <>
      {before}
      <RelativeTime iso={iso} />
      {after}
    </>
  );
}
