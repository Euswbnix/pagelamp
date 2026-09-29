import {
  Archive,
  Bell,
  CalendarRange,
  FileSearch,
  type LucideIcon,
  RefreshCw,
  Sparkles,
} from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useAcknowledgeWhatsNew,
  useSetUpdatePrefs,
  useStartupTasks,
  useUpdatePrefs,
} from "@/api/queries";
import type { WhatsNewTopic } from "@/api/types";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

const TOPIC_ICON: Record<WhatsNewTopic, LucideIcon> = {
  update_check: RefreshCw,
  course_weeks: CalendarRange,
  course_removal: Archive,
  syllabus_reading: FileSearch,
  ai_writing: Sparkles,
  reminders: Bell,
};

/**
 * One-time "What's new" for upgraders (from 0.1 or an earlier alpha), who never saw
 * onboarding. It explains the automatic update check BEFORE the first one runs, with the switch
 * right there. Closing it any way counts as read; the facade then decides whether a check is due.
 */
export function WhatsNewSheet() {
  const tasks = useStartupTasks();
  const whatsNew = tasks.data?.whats_new;
  if (!whatsNew || whatsNew.topics.length === 0) return null;
  return <Sheet since={whatsNew.since ?? null} topics={whatsNew.topics} />;
}

function Sheet({ since, topics }: { since: string | null; topics: WhatsNewTopic[] }) {
  const { t } = useTranslation("updates");
  const prefs = useUpdatePrefs();
  const setPrefs = useSetUpdatePrefs();
  const acknowledge = useAcknowledgeWhatsNew();
  const [open, setOpen] = useState(true);
  // null = untouched: keep whatever the preference is.
  const [autoCheck, setAutoCheck] = useState<boolean | null>(null);
  const switchId = useId();
  const current = autoCheck ?? prefs.data?.auto_check ?? true;

  async function done() {
    setOpen(false);
    if (autoCheck !== null && prefs.data && autoCheck !== prefs.data.auto_check) {
      await setPrefs.mutateAsync({ auto_check: autoCheck, channel: prefs.data.channel ?? null });
    }
    await acknowledge.mutateAsync();
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) void done();
      }}
    >
      <DialogContent className="sm:max-w-lg" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{t("whatsNew.title")}</DialogTitle>
          {since ? (
            <DialogDescription>{t("whatsNew.since", { version: since })}</DialogDescription>
          ) : null}
        </DialogHeader>
        <ul className="space-y-4">
          {topics.map((topic) => {
            const Icon = TOPIC_ICON[topic];
            return (
              <li key={topic} className="flex gap-3">
                <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
                <div className="min-w-0 space-y-1">
                  <p className="font-medium">{t(`whatsNew.topics.${topic}.title`)}</p>
                  <p className="text-muted-foreground">{t(`whatsNew.topics.${topic}.body`)}</p>
                  {topic === "update_check" ? (
                    <div className="flex items-center gap-2 pt-1">
                      <Switch
                        id={switchId}
                        checked={current}
                        onCheckedChange={(checked) => setAutoCheck(checked)}
                      />
                      <Label htmlFor={switchId}>{t("settings.autoCheck")}</Label>
                    </div>
                  ) : null}
                </div>
              </li>
            );
          })}
        </ul>
        <DialogFooter>
          <Button type="button" onClick={() => void done()}>
            {t("whatsNew.done")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
