import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useMcpClientConfigs } from "@/api/queries";
import type { McpClientConfig, TemporaryLocation } from "@/api/types";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

/**
 * Where the binary AI apps would launch runs from, when that place won't last: the mounted
 * disk image, a macOS App Translocation copy (the app wasn't moved out of Downloads) or an
 * AppImage mount. A config copied now breaks later, so say so before anything else. Every
 * config carries the same launch, so the first one that says so decides.
 */
export function temporaryLocation(configs: readonly McpClientConfig[]): TemporaryLocation | null {
  return configs.find((c) => c.launch.temporary_location)?.launch.temporary_location ?? null;
}

/** Prominent warning for the Connect page and onboarding's last step; nothing when all is well. */
export function TemporaryLocationWarning() {
  const { t } = useTranslation("connect");
  const configs = useMcpClientConfigs();
  const where = temporaryLocation(configs.data ?? []);
  if (!where) return null;
  return (
    <Alert role="note" className="border-warning/50 bg-warning/10 px-4 py-3 *:[svg]:text-warning">
      <TriangleAlert aria-hidden />
      <AlertTitle>{t(`temporaryLocation.${where}.title`)}</AlertTitle>
      <AlertDescription className="text-foreground">
        {t(`temporaryLocation.${where}.body`)}
      </AlertDescription>
    </Alert>
  );
}
