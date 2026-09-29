import { Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useAiStatus } from "@/api/ai-queries";
import { type BackendRef, backendKey } from "@/api/provisional/ai";
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
  const [disclosureFor, setDisclosureFor] = useState<BackendRef | null>(null);

  const backends = status.data?.backends ?? [];
  const disclosed = disclosureFor
    ? backends.find((b) => backendKey(b.backend) === backendKey(disclosureFor))
    : undefined;
  const showDisclosure = (backend: BackendRef) => setDisclosureFor(backend);

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
            <ul aria-label={t("settings.backendsLabel")} className="space-y-3">
              {backends.map((backend) => (
                <BackendRow
                  key={backendKey(backend.backend)}
                  status={backend}
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
            backends={backends}
            onAdded={(record) =>
              showDisclosure({ kind: "provider", provider_id: record.provider_id })
            }
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
        onAdded={(record) => showDisclosure({ kind: "provider", provider_id: record.provider_id })}
      />
      {disclosed ? (
        <DisclosureDialog
          status={disclosed}
          open
          onOpenChange={(open) => {
            if (!open) setDisclosureFor(null);
          }}
        />
      ) : null}
    </SettingsSection>
  );
}
