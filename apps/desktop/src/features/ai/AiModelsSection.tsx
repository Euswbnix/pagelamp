import { Plus } from "lucide-react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { type BackendRef, backendKey, providerOf } from "@/api/ai";
import { useAiStatus } from "@/api/ai-queries";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { SettingsSection } from "@/features/settings/SettingsSection";
import { ApiKeyDialog, type ApiKeyDialogMode } from "./ApiKeyDialog";
import { BackendRow } from "./BackendRow";
import { BudgetField } from "./BudgetField";
import { DisclosureDialog } from "./DisclosureDialog";
import { FeatureModels } from "./FeatureModels";
import { LocalServers } from "./LocalServers";
import { RemoveAllAiData } from "./RemoveAllAiData";
import { useAiErrorText } from "./useAiErrorText";

/**
 * Settings → AI models (M1: API keys and local models; design §7). Everything shown here comes
 * from `ai_status` and the facade's other answers; this screen decides nothing on its own.
 * Adding a backend goes straight on to its disclosure sheet: it isn't used until the student
 * turned it on there.
 */
export function AiModelsSection() {
  const { t } = useTranslation("ai");
  const status = useAiStatus();
  const errorText = useAiErrorText();
  const [keyDialog, setKeyDialog] = useState<ApiKeyDialogMode | null>(null);
  // The backend whose sheet is open; `added` = it was just added (focus goes to its row after).
  const [disclosureFor, setDisclosureFor] = useState<{
    backend: BackendRef;
    added: boolean;
  } | null>(null);
  const list = useRef<HTMLUListElement>(null);

  const backends = status.data?.backends ?? [];
  const disclosed = disclosureFor
    ? backends.find((b) => backendKey(b.backend) === backendKey(disclosureFor.backend))
    : undefined;
  const showDisclosure = (backend: BackendRef, added = false) =>
    setDisclosureFor({ backend, added });
  const showAdded = (providerId: string) =>
    showDisclosure({ kind: "provider", provider_id: providerId }, true);
  /** The row of a backend, e.g. to put focus on one just added. */
  const rowOf = (backend: BackendRef) =>
    [...(list.current?.querySelectorAll<HTMLElement>("li[data-backend]") ?? [])].find(
      (row) => row.dataset.backend === backendKey(backend),
    ) ?? null;

  return (
    <SettingsSection title={t("settings.title")} description={t("settings.description")}>
      {status.isPending ? (
        <Skeleton className="h-24 w-full" />
      ) : status.isError ? (
        <p role="alert" className="text-sm text-destructive">
          {t("settings.loadFailed")} {errorText(status.error)}
        </p>
      ) : (
        <>
          {backends.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("settings.empty")}</p>
          ) : (
            <ul ref={list} aria-label={t("settings.backendsLabel")} className="space-y-3">
              {backends.map((backend) => (
                <BackendRow
                  key={backendKey(backend.backend)}
                  status={backend}
                  provider={providerOf(status.data, backend)}
                  onShowDisclosure={() => showDisclosure(backend.backend)}
                  onReplaceKey={(provider) => setKeyDialog({ kind: "replace", provider })}
                />
              ))}
            </ul>
          )}
          <Button type="button" variant="outline" onClick={() => setKeyDialog({ kind: "add" })}>
            <Plus aria-hidden />
            {t("settings.addKey")}
          </Button>

          <LocalServers
            providers={status.data.providers}
            onAdded={(record) => showAdded(record.provider_id)}
          />

          {backends.length > 0 ? (
            <FeatureModels backends={backends} features={status.data.features} />
          ) : null}
          {backends.some((b) => b.kind === "api_key") ? (
            <BudgetField budget={status.data.budget} />
          ) : null}
          <RemoveAllAiData />
        </>
      )}

      <ApiKeyDialog
        mode={keyDialog}
        onClose={() => setKeyDialog(null)}
        onAdded={(record) => showAdded(record.provider_id)}
      />
      {disclosed ? (
        <DisclosureDialog
          status={disclosed}
          open
          onOpenChange={(open) => {
            if (!open) setDisclosureFor(null);
          }}
          returnFocus={disclosureFor?.added ? () => rowOf(disclosed.backend) : undefined}
        />
      ) : null}
    </SettingsSection>
  );
}
