import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useAcknowledgeWhatsNew,
  useSetSyncPrefs,
  useSetUpdatePrefs,
  useStartupTasks,
  useSyncPrefs,
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
import { useSyncStore } from "@/stores/sync";
import { shownTopics, TOPIC_ICON } from "./whatsNewTopics";

/**
 * One-time "What's new" for upgraders: the topics introduced since their version (all of them
 * from 0.1, which never saw onboarding). It explains the automatic update check and the
 * automatic sync BEFORE the first one runs, each with its switch right there. Closing it any way
 * counts as read; the facade then decides whether a check or a sync is due. Each topic is a
 * heading; the focus starts on the title so the sheet is read from the top, and a long list
 * scrolls inside the window.
 *
 * The update check waits for the acknowledgement, so a topic this build has no copy or icon for
 * is left out, and with none left the sheet is acknowledged without being shown.
 */
export function WhatsNewSheet() {
  const tasks = useStartupTasks();
  const { i18n } = useTranslation("updates");
  const whatsNew = tasks.data?.whats_new;
  if (!whatsNew) return null;
  const topics = shownTopics(whatsNew.topics, (key) => i18n.exists(key, { ns: "updates" }));
  if (topics.length === 0) return <AcknowledgeUnshown />;
  return <Sheet since={whatsNew.since ?? null} topics={topics} />;
}

/** Nothing to show: still count What's new as read, once, so the update check isn't held. */
function AcknowledgeUnshown() {
  const acknowledge = useAcknowledgeWhatsNew();
  const sent = useRef(false);
  useEffect(() => {
    if (sent.current) return;
    sent.current = true;
    acknowledge.mutate();
  }, [acknowledge]);
  return null;
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
  const titleRef = useRef<HTMLHeadingElement>(null);
  const current = autoCheck ?? prefs.data?.auto_check ?? true;
  const syncPrefs = useSyncPrefs();
  const setSyncPrefs = useSetSyncPrefs();
  // The same for automatic sync: null = untouched. How often is chosen on Sources & sync.
  const [autoSync, setAutoSync] = useState<boolean | null>(null);
  const syncSwitchId = useId();
  const syncWasOn = syncPrefs.data?.auto_sync !== "off";
  const syncOn = autoSync ?? syncWasOn;

  async function done() {
    setOpen(false);
    // Closing the sheet is the student's action: a sync that becomes due with it is attended.
    useSyncStore.getState().noteStudentAction();
    if (autoCheck !== null && prefs.data && autoCheck !== prefs.data.auto_check) {
      await setPrefs.mutateAsync({ auto_check: autoCheck, channel: prefs.data.channel ?? null });
    }
    // Saved before the acknowledgement: once that is in, a sync may be due.
    // Only a real change is saved. When the setting couldn't be read it counts as on (the
    // default), so "off" is still saved, and off-then-on writes nothing over a stored choice.
    if (autoSync !== null && autoSync !== syncWasOn) {
      await setSyncPrefs.mutateAsync({
        ...syncPrefs.data,
        auto_sync: autoSync ? "twice_daily" : "off",
      });
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
      <DialogContent
        className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-lg"
        showCloseButton={false}
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          titleRef.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle ref={titleRef} tabIndex={-1} className="outline-none">
            {t("whatsNew.title")}
          </DialogTitle>
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
                  <h3 className="font-medium">{t(`whatsNew.topics.${topic}.title`)}</h3>
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
                  {topic === "auto_sync" ? (
                    <div className="flex items-center gap-2 pt-1">
                      <Switch
                        id={syncSwitchId}
                        checked={syncOn}
                        onCheckedChange={(checked) => setAutoSync(checked)}
                      />
                      <Label htmlFor={syncSwitchId}>{t("whatsNew.autoSync")}</Label>
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
