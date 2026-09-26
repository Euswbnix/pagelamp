import { useTranslation } from "react-i18next";
import { formatDateTime, formatRelative } from "@/lib/format";

/** "3 hours ago" with the exact date in a tooltip and the machine-readable <time>. */
export function RelativeTime({ iso, now }: { iso: string; now?: Date }) {
  const { i18n } = useTranslation();
  return (
    <time dateTime={iso} title={formatDateTime(iso, i18n.language)}>
      {formatRelative(iso, i18n.language, now)}
    </time>
  );
}
