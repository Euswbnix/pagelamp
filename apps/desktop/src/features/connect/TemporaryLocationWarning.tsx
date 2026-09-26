import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useMcpClientConfigs } from "@/api/queries";
import type { McpClientConfig } from "@/api/types";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

/**
 * The backend adds `run_from_temporary_location` (first, to every config) when the binary AI
 * apps would launch lives somewhere that disappears: the mounted disk image, a macOS App
 * Translocation copy, or an AppImage mount. A config copied now breaks later, so say so before
 * anything else. Which case it is follows from the path: macOS paths are inside a `.app`.
 */
export function temporaryLocation(configs: readonly McpClientConfig[]): "mac" | "appimage" | null {
  const config = configs.find((c) => c.note_codes.includes("run_from_temporary_location"));
  if (!config) return null;
  return config.launch.command.includes(".app/") ? "mac" : "appimage";
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
