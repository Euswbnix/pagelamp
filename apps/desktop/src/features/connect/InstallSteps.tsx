import type { TFunction } from "i18next";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { McpClientConfig } from "@/api/types";
import { CodeBlock } from "@/components/common/CodeBlock";
import { CopyButton } from "@/components/common/CopyButton";
import { mcpServerEntry } from "./snippet";

interface Step {
  key: string;
  text: string;
  /** Shown under the step text: a file path or the snippet to copy. */
  extra?: ReactNode;
}

/** Numbered setup steps. They depend only on `install_kind`, so new clients need no UI change. */
export function InstallSteps({ config }: { config: McpClientConfig }) {
  const { t } = useTranslation("connect");
  const steps = stepsFor(config, t);
  return (
    <ol aria-label={t("steps.label", { app: config.title })} className="space-y-4">
      {steps.map((step, index) => (
        <li key={step.key} className="flex gap-3">
          <span
            aria-hidden
            className="grid size-6 shrink-0 place-items-center rounded-full border bg-background text-xs font-medium tabular-nums text-muted-foreground"
          >
            {index + 1}
          </span>
          <div className="min-w-0 flex-1 space-y-2 pt-0.5">
            <p>{step.text}</p>
            {step.extra}
          </div>
        </li>
      ))}
    </ol>
  );
}

function stepsFor(config: McpClientConfig, t: TFunction<"connect">): Step[] {
  const snippet = (
    <CodeBlock
      code={config.content}
      copyLabel={t(config.install_kind === "shell_command" ? "copyCommand" : "copyConfig", {
        app: config.title,
      })}
    />
  );
  const path = config.config_path_hint ? (
    <ConfigPath path={config.config_path_hint} app={config.title} />
  ) : null;

  switch (config.install_kind) {
    case "json_snippet":
      // Quit first: apps like Claude Desktop write this file when they quit, overwriting an
      // edit made while they were running.
      return [
        { key: "quit", text: t("steps.quitFirst", { app: config.title }) },
        { key: "open", text: path ? t("steps.openJson") : t("steps.openJsonNoPath"), extra: path },
        {
          key: "paste",
          text: t("steps.pasteJson"),
          extra: (
            <>
              {snippet}
              <EntryOnly config={config} />
            </>
          ),
        },
        { key: "reopen", text: t("steps.saveAndReopen", { app: config.title }) },
      ];
    case "toml_snippet":
      return [
        { key: "open", text: path ? t("steps.openToml") : t("steps.openTomlNoPath"), extra: path },
        { key: "paste", text: t("steps.pasteToml"), extra: snippet },
        { key: "restart", text: t("steps.restart") },
      ];
    case "shell_command":
      return [
        { key: "terminal", text: t("steps.openTerminal") },
        { key: "run", text: t("steps.runCommand"), extra: snippet },
      ];
  }
}

/** For a file that already has an `mcpServers` section: just our entry, to add inside it. */
function EntryOnly({ config }: { config: McpClientConfig }) {
  const { t } = useTranslation("connect");
  const entry = mcpServerEntry(config.content);
  if (!entry) return null;
  return (
    <div className="space-y-2 pt-1">
      <p>{t("steps.pasteJsonEntry")}</p>
      <CodeBlock code={entry} copyLabel={t("copyEntry", { app: config.title })} />
    </div>
  );
}

/** The config file location in monospace with its own copy button. */
function ConfigPath({ path, app }: { path: string; app: string }) {
  const { t } = useTranslation("connect");
  return (
    <div className="flex items-center gap-2 rounded-lg border bg-muted/50 py-1.5 pr-1.5 pl-3">
      <code className="min-w-0 flex-1 font-mono text-[13px] break-all">{path}</code>
      <CopyButton text={path} label={t("copyPath", { app })} variant="ghost" />
    </div>
  );
}
