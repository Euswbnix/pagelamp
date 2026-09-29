import { BellRing, X } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useApi } from "@/api/context";
import { useStartupTasks } from "@/api/queries";
import type { Reminder } from "@/api/reminders";
import { Alert, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { paths } from "@/lib/routes";
import { REMINDERS_UI } from "./availability";
import { useReminderSettings } from "./queries";
import { reminderText } from "./reminderText";

/**
 * Reminders off ("Not now"): what came due since the last launch shows here instead of as
 * notifications (design §5.3, the leader's decision 6b): the course code, title and due time.
 * Opening one, or dismissing the card, marks them shown, so they don't come back.
 */
export function RemindersCatchUp() {
  const { t, i18n } = useTranslation("reminders");
  const api = useApi();
  const tasks = useStartupTasks();
  const settings = useReminderSettings();
  const [seen, setSeen] = useState<ReadonlySet<string>>(new Set());
  const titleId = useId();
  if (!REMINDERS_UI || settings.data?.run_in_background !== false) return null;
  const due = (tasks.data?.due_reminders ?? []).filter((r) => !seen.has(r.id));
  if (due.length === 0) return null;

  function markShown(ids: string[]) {
    setSeen((before) => new Set([...before, ...ids]));
    api.markRemindersShown(ids).catch(() => {});
  }

  return (
    <Alert role="region" aria-labelledby={titleId} className="mb-6 px-4 py-3">
      <BellRing aria-hidden />
      <AlertTitle id={titleId}>{t("catchUp.title")}</AlertTitle>
      <div className="col-start-2 space-y-3">
        <ul className="divide-y text-sm">
          {due.map((reminder) => {
            const text = reminderText(reminder, t, i18n.language);
            return (
              <li key={reminder.id} className="flex items-baseline justify-between gap-3 py-1.5">
                <span className="min-w-0">
                  <span className="font-medium">{text.title}</span>
                  {text.body ? <span className="text-muted-foreground"> · {text.body}</span> : null}
                </span>
                <Button asChild variant="ghost" size="sm" className="shrink-0">
                  <Link to={destination(reminder)} onClick={() => markShown([reminder.id])}>
                    {t("catchUp.open")}
                  </Link>
                </Button>
              </li>
            );
          })}
        </ul>
        <div className="flex flex-wrap items-center gap-3">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => markShown(due.map((r) => r.id))}
          >
            <X aria-hidden />
            {t("catchUp.dismiss")}
          </Button>
          <Link to={paths.settings} className="text-sm underline underline-offset-2">
            {t("catchUp.turnOn")}
          </Link>
        </div>
      </div>
    </Alert>
  );
}

/** A deadline opens its course; the week and today's plan open Courses (the plan is there). */
function destination(reminder: Reminder): string {
  return reminder.kind === "deadline_soon" && reminder.course_id
    ? paths.course(reminder.course_id)
    : paths.courses;
}
