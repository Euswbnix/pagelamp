import { FileCode, type LucideIcon, SquareTerminal, Star } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { InstallKind, McpClientConfig } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import { ClientNotes } from "./ClientNotes";
import { InstallSteps } from "./InstallSteps";

const KIND_ICON: Record<InstallKind, LucideIcon> = {
  json_snippet: FileCode,
  toml_snippet: FileCode,
  shell_command: SquareTerminal,
};

interface ClientConfigCardProps {
  config: McpClientConfig;
  /** Marks the card as the easiest place to start (Claude Desktop). */
  recommended?: boolean;
}

/** Setup card for one AI app: how it installs, the backend's notes, then numbered steps. */
export function ClientConfigCard({ config, recommended = false }: ClientConfigCardProps) {
  const { t } = useTranslation("connect");
  const headingId = useId();
  const KindIcon = KIND_ICON[config.install_kind];
  return (
    <article aria-labelledby={headingId}>
      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-center gap-2">
            <h2 id={headingId} className="font-heading text-base leading-snug font-medium">
              {config.title}
            </h2>
            {recommended ? (
              <Badge variant="secondary">
                <Star aria-hidden />
                {t("clients.recommended")}
              </Badge>
            ) : null}
          </div>
          <CardDescription className="flex items-center gap-1.5">
            <KindIcon className="size-4" aria-hidden />
            {t(`installKind.${config.install_kind}`)}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          <ClientNotes client={config.client} notes={config.notes} codes={config.note_codes} />
          <InstallSteps config={config} />
        </CardContent>
      </Card>
    </article>
  );
}
