import { CircleCheck, CircleDashed, TriangleAlert } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { type AiBackendStatus, backendKey, type ModelProviderRecord } from "@/api/ai";
import { useModelProviderPresets, useRemoveModelProvider } from "@/api/ai-queries";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { firstKey } from "./ApiKeyDialog";
import { DataPolicyLine } from "./DataPolicyLine";
import { useAiErrorText } from "./useAiErrorText";

/**
 * One configured backend: its name, kind and state, problems, the key's last 4 characters, the
 * data-policy line, and what can be done with it. The state and problems come from the facade.
 */
export function BackendRow({
  status,
  provider,
  onShowDisclosure,
  onReplaceKey,
}: {
  status: AiBackendStatus;
  /** The record behind an API-key or local backend (ai_status lists them apart). */
  provider: ModelProviderRecord | null;
  /** Open a note: the backend's own, or (a server here) the one of its cloud models. */
  onShowDisclosure: (note: BackendNote) => void;
  onReplaceKey: (provider: ModelProviderRecord) => void;
}) {
  const { t } = useTranslation("ai");
  const headingId = useId();
  const presets = useModelProviderPresets();
  // By the server's kind, wherever it is: a key for a service that takes one (a custom server
  // here, added without one, may get one later); none for Ollama or LM Studio, even remote.
  const takesKey = !!provider && !!presets.data?.find((p) => p.id === provider.preset)?.needs_key;
  const own = hasOwnNote(provider);
  const cloud = status.cloud_disclosure ?? null;
  const ownRead = status.disclosure_acknowledged === status.disclosure.version;
  const cloudRead = !!cloud && status.cloud_disclosure_acknowledged === cloud.version;
  const needs = status.state === "needs_disclosure";
  const StateIcon =
    status.state === "ready"
      ? CircleCheck
      : status.state === "needs_disclosure"
        ? CircleDashed
        : TriangleAlert;
  return (
    // Focusable from code only: after a new backend is turned on, focus lands on its row.
    <li
      tabIndex={-1}
      data-backend={backendKey(status.backend)}
      className="space-y-2 py-4 outline-hidden focus-visible:rounded-row focus-visible:ring-3 focus-visible:ring-ring"
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <h3 id={headingId} className="font-medium">
          {status.label}
        </h3>
        <Badge variant="outline">{t(`backend.kind.${status.kind}`)}</Badge>
        <span className="inline-flex items-center gap-1 text-sm text-muted-foreground">
          <StateIcon className="size-4" aria-hidden />
          {t(`backend.state.${status.state}`)}
        </span>
      </div>
      {provider?.key_last4 ? (
        <p className="text-sm text-muted-foreground">
          {t("backend.keyEnds", { last4: provider.key_last4 })}
        </p>
      ) : null}
      <DataPolicyLine
        facts={own ? status.disclosure : (cloud ?? status.disclosure)}
        name={status.label}
      />
      {status.problems.length > 0 ? (
        <ul className="space-y-1 text-sm">
          {status.problems.map((problem) => (
            <li key={problem}>{t(`backend.problem.${problem}`)}</li>
          ))}
        </ul>
      ) : null}
      <div className="flex flex-wrap gap-2 pt-1">
        {own ? (
          <Button
            type="button"
            size="sm"
            variant={needs && !ownRead ? "default" : "outline"}
            onClick={() => onShowDisclosure("own")}
            aria-describedby={headingId}
          >
            {needs && !ownRead ? t("backend.turnOn") : t("backend.whatsShared")}
          </Button>
        ) : null}
        {cloud ? (
          <Button
            type="button"
            size="sm"
            variant={needs && !cloudRead ? "default" : "outline"}
            onClick={() => onShowDisclosure("cloud")}
            aria-describedby={headingId}
          >
            {needs && !cloudRead ? t("backend.turnOnCloud") : t("backend.cloudShared")}
          </Button>
        ) : null}
        {provider && takesKey ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => onReplaceKey(provider)}
            aria-describedby={headingId}
          >
            {firstKey(provider) ? t("backend.addKey") : t("backend.replaceKey")}
          </Button>
        ) : null}
        {provider ? <RemoveBackendButton provider={provider} describedBy={headingId} /> : null}
      </div>
    </li>
  );
}

/** Which of a backend's notes: its own, or the one of a model its server here runs in the cloud. */
export type BackendNote = "own" | "cloud";

/**
 * Whether the backend's own note can apply: always off this computer and for the ChatGPT plan;
 * on it, only for a server that says where a model runs (Ollama, LM Studio). A custom server
 * here only ever needs the note of its cloud models (as the facade decides it).
 */
export function hasOwnNote(provider: ModelProviderRecord | null): boolean {
  if (!provider?.on_device) return true;
  return provider.preset === "ollama" || provider.preset === "lm_studio";
}

function RemoveBackendButton({
  provider,
  describedBy,
}: {
  provider: ModelProviderRecord;
  describedBy: string;
}) {
  const { t } = useTranslation("ai");
  const { t: tc } = useTranslation();
  const remove = useRemoveModelProvider();
  const errorText = useAiErrorText();
  const [open, setOpen] = useState(false);
  const name = provider.label;

  async function confirm() {
    try {
      await remove.mutateAsync(provider.provider_id);
      toast.success(t("backend.removed", { name }));
      setOpen(false);
    } catch {
      // Shown in the dialog (remove.error).
    }
  }

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        onClick={() => setOpen(true)}
        aria-describedby={describedBy}
      >
        {t("backend.remove")}
      </Button>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("backend.removeTitle", { name })}</AlertDialogTitle>
          <AlertDialogDescription>{t("backend.removeBody", { name })}</AlertDialogDescription>
        </AlertDialogHeader>
        {remove.error ? (
          <p role="alert" className="text-sm text-destructive">
            {errorText(remove.error)}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel>{tc("actions.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={(event) => {
              // Keep the dialog open until the removal is done (or failed).
              event.preventDefault();
              void confirm();
            }}
          >
            {t("backend.remove")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
