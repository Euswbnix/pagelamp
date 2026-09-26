import { Cable } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useMcpClientConfigs } from "@/api/queries";
import type { McpClientConfig } from "@/api/types";
import { AiDisclosure } from "@/components/common/AiDisclosure";
import { ErrorState } from "@/components/common/ErrorState";
import { PageHeader } from "@/components/common/PageHeader";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { MAC_QUARANTINE_HINT } from "@/lib/platform";
import { ClientConfigCard } from "./ClientConfigCard";
import { ConnectSkeleton } from "./ConnectSkeleton";
import { HowItWorks } from "./HowItWorks";
import { sortClientConfigs } from "./order";
import { QuarantineHint } from "./QuarantineHint";
import { TryItCard } from "./TryItCard";

/** "Connect your AI app": one setup card per supported AI app, built from the backend's configs. */
export function ConnectPage() {
  const { t } = useTranslation("connect");
  const configs = useMcpClientConfigs();

  return (
    <>
      <PageHeader title={t("title")} description={t("description")} />
      <div className="space-y-6">
        <AiDisclosure />
        <HowItWorks />
        {configs.isPending ? (
          <ConnectSkeleton />
        ) : configs.isError ? (
          <ErrorState
            error={configs.error}
            title={t("errorTitle")}
            onRetry={() => void configs.refetch()}
          />
        ) : configs.data.length === 0 ? (
          <NoConfigs onRetry={() => void configs.refetch()} />
        ) : (
          <ClientList configs={configs.data} />
        )}
        <TryItCard />
      </div>
    </>
  );
}

function ClientList({ configs }: { configs: McpClientConfig[] }) {
  // Every AI app launches the same binary, so macOS's quarantine hint is shown once, for it.
  const command = configs[0]?.launch.command;
  return (
    <div className="space-y-4">
      {sortClientConfigs(configs).map((config, index) => (
        <ClientConfigCard
          key={`${config.client}:${config.title}`}
          config={config}
          recommended={index === 0 && config.client === "claude_desktop"}
        />
      ))}
      {MAC_QUARANTINE_HINT && command ? <QuarantineHint command={command} /> : null}
    </div>
  );
}

function NoConfigs({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation("connect");
  const { t: tc } = useTranslation();
  return (
    <Empty className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Cable aria-hidden />
        </EmptyMedia>
        <EmptyTitle>{t("empty.title")}</EmptyTitle>
        <EmptyDescription>{t("empty.description")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button variant="outline" onClick={onRetry}>
          {tc("actions.retry")}
        </Button>
      </EmptyContent>
    </Empty>
  );
}
