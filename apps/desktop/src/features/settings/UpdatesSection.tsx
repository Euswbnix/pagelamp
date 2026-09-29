import { ArrowDownToLine, Download, LoaderCircle, RefreshCw } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AvailableUpdate, UpdaterStatus } from "@/api/client";
import {
  useEffectiveUpdateChannel,
  useLastUpdateCheck,
  useSetUpdatePrefs,
  useUpdatePrefs,
  useUpdaterStatus,
} from "@/api/queries";
import type { UpdateChannel, UpdatePrefs } from "@/api/types";
import { SentenceWithTime, WHEN } from "@/components/common/SentenceWithTime";
import { useOpenExternal } from "@/components/common/useOpenExternal";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Switch } from "@/components/ui/switch";
import { InstallUpdateDialog } from "@/features/updates/InstallUpdateDialog";
import { isHttpUrl } from "@/lib/url";
import { useCheckForUpdate, useUpdateStore } from "@/stores/updates";
import { SettingsSection } from "./SettingsSection";

const CHANNELS: UpdateChannel[] = ["stable", "beta"];

/**
 * Settings → Updates: the version, automatic checks (on by default, D2), the channel, "Check now"
 * and — only when the student asks — "Install and restart". deb/rpm installs get a download link.
 */
export function UpdatesSection() {
  const { t } = useTranslation("updates");
  const status = useUpdaterStatus();
  const prefs = useUpdatePrefs();
  const channel = useEffectiveUpdateChannel();
  return (
    <SettingsSection title={t("settings.title")} description={t("settings.description")}>
      {status.data ? (
        <dl className="grid grid-cols-[auto_1fr] gap-x-6 text-sm">
          <dt className="text-muted-foreground">{t("settings.version")}</dt>
          <dd className="font-mono">{status.data.current_version}</dd>
        </dl>
      ) : null}
      {prefs.data && channel.data ? (
        <Preferences prefs={prefs.data} effectiveChannel={channel.data} />
      ) : null}
      <CheckNow status={status.data ?? null} />
    </SettingsSection>
  );
}

function Preferences({
  prefs,
  effectiveChannel,
}: {
  prefs: UpdatePrefs;
  effectiveChannel: UpdateChannel;
}) {
  const { t } = useTranslation("updates");
  const setPrefs = useSetUpdatePrefs();
  const switchId = useId();
  const hintId = useId();
  const channelLabelId = useId();
  return (
    <div className="space-y-5">
      <div className="flex items-start gap-3">
        <Switch
          id={switchId}
          checked={prefs.auto_check}
          aria-describedby={hintId}
          onCheckedChange={(checked) =>
            setPrefs.mutate({ auto_check: checked, channel: prefs.channel ?? null })
          }
        />
        <div className="space-y-0.5">
          <Label htmlFor={switchId}>{t("settings.autoCheck")}</Label>
          <p id={hintId} className="text-xs text-muted-foreground">
            {t("settings.autoCheckHint")}
          </p>
        </div>
      </div>
      <div className="space-y-2">
        <p id={channelLabelId} className="text-sm font-medium">
          {t("settings.channel")}
        </p>
        <RadioGroup
          aria-labelledby={channelLabelId}
          value={prefs.channel ?? effectiveChannel}
          onValueChange={(value) =>
            setPrefs.mutate({ auto_check: prefs.auto_check, channel: value as UpdateChannel })
          }
          className="gap-3"
        >
          {CHANNELS.map((channel) => (
            <ChannelOption key={channel} channel={channel} />
          ))}
        </RadioGroup>
      </div>
    </div>
  );
}

function ChannelOption({ channel }: { channel: UpdateChannel }) {
  const { t } = useTranslation("updates");
  const id = useId();
  const hintId = useId();
  const label = channel === "stable" ? t("settings.channelStable") : t("settings.channelBeta");
  const hint =
    channel === "stable" ? t("settings.channelStableHint") : t("settings.channelBetaHint");
  return (
    <div className="flex items-start gap-2">
      <RadioGroupItem id={id} value={channel} aria-describedby={hintId} className="mt-0.5" />
      <div className="space-y-0.5">
        <Label htmlFor={id}>{label}</Label>
        <p id={hintId} className="text-xs text-muted-foreground">
          {hint}
        </p>
      </div>
    </div>
  );
}

function CheckNow({ status }: { status: UpdaterStatus | null }) {
  const { t } = useTranslation("updates");
  const { t: tc } = useTranslation();
  const check = useCheckForUpdate();
  const checking = useUpdateStore((s) => s.checking);
  const checked = useUpdateStore((s) => s.checked);
  const available = useUpdateStore((s) => s.available);
  const checkError = useUpdateStore((s) => s.checkError);
  const last = useLastUpdateCheck();

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={checking}
          onClick={() => void check()}
        >
          {checking ? (
            <LoaderCircle className="animate-spin" aria-hidden />
          ) : (
            <RefreshCw aria-hidden />
          )}
          {checking ? t("settings.checking") : t("settings.checkNow")}
        </Button>
        <span className="text-xs text-muted-foreground">
          {last.data ? (
            <SentenceWithTime text={t("settings.lastChecked", { when: WHEN })} iso={last.data.at} />
          ) : last.isSuccess ? (
            t("settings.neverChecked")
          ) : null}
        </span>
      </div>
      <div role="status" className="text-sm">
        {checking ? null : checkError ? (
          <div className="space-y-0.5 text-destructive">
            <p className="font-medium">{t("settings.checkFailed")}</p>
            <p>{tc(`errors.${checkError.kind}`)}</p>
          </div>
        ) : available ? (
          <Available update={available} status={status} />
        ) : checked ? (
          <p>{t("settings.upToDate")}</p>
        ) : null}
      </div>
    </div>
  );
}

function Available({ update, status }: { update: AvailableUpdate; status: UpdaterStatus | null }) {
  const { t } = useTranslation("updates");
  const openExternal = useOpenExternal();
  const [open, setOpen] = useState(false);
  const downloadOnly = status?.install === "download_only";
  return (
    <div className="space-y-3 rounded-lg border p-3">
      <p className="font-medium">{t("settings.available", { version: update.version })}</p>
      {update.notes ? (
        <details className="text-muted-foreground">
          <summary className="cursor-pointer text-foreground">{t("settings.releaseNotes")}</summary>
          <p lang="en" className="mt-1 whitespace-pre-line">
            {update.notes}
          </p>
        </details>
      ) : null}
      {downloadOnly ? (
        <div className="space-y-2">
          <p className="text-xs text-muted-foreground">{t("settings.downloadHint")}</p>
          {isHttpUrl(update.download_url) ? (
            <Button type="button" size="sm" onClick={() => openExternal(update.download_url ?? "")}>
              <Download aria-hidden />
              {t("settings.download", { version: update.version })}
            </Button>
          ) : null}
        </div>
      ) : (
        <>
          <Button type="button" size="sm" onClick={() => setOpen(true)}>
            <ArrowDownToLine aria-hidden />
            {t("settings.install")}
          </Button>
          <InstallUpdateDialog update={update} open={open} onOpenChange={setOpen} />
        </>
      )}
    </div>
  );
}
