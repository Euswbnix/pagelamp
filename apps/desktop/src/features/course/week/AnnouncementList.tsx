import { Megaphone } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { MaterialView } from "@/api/types";
import { ExternalLink } from "@/components/common/ExternalLink";
import { formatDay } from "@/lib/format";
import { isHttpUrl } from "@/lib/url";

/** Titles and dates of announcements from the last two weeks (newest first, from the backend). */
export function AnnouncementList({ announcements }: { announcements: MaterialView[] }) {
  const { t, i18n } = useTranslation("course");
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-2">
      <h2 id={headingId} className="font-heading text-base font-semibold tracking-tight">
        {t("week.announcements.title")}
      </h2>
      {announcements.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("week.announcements.empty")}</p>
      ) : (
        <ul className="divide-y border-t">
          {announcements.map((item) => (
            <li key={item.id} className="flex items-start gap-3 py-2.5">
              <Megaphone className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
              <div className="min-w-0 flex-1">
                {isHttpUrl(item.url) ? (
                  <ExternalLink href={item.url} className="font-medium">
                    {item.title}
                  </ExternalLink>
                ) : (
                  <span className="font-medium">{item.title}</span>
                )}
              </div>
              {item.published_at ? (
                <time
                  dateTime={item.published_at}
                  className="shrink-0 text-xs text-muted-foreground"
                >
                  {formatDay(item.published_at, i18n.language)}
                </time>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
