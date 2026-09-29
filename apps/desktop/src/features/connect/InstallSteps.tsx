import type { TFunction } from "i18next";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { McpClientConfig } from "@/api/types";
import { CodeBlock } from "@/components/common/CodeBlock";
import { CopyButton } from "@/components/common/CopyButton";
import {
  claudeMcpRemove,
  mcpServerEntry,
  mcpServerName,
  mcpServersKey,
  tomlTable,
} from "./snippet";

interface Step {
  key: string;
  text: string;
  /** Shown under the step text: a file path or the snippet to copy. */
  extra?: ReactNode;
}

/**
 * Numbered setup steps, by `install_kind`, with a few app-specific ones (Claude Desktop's
 * quit-first order, Claude Code's "new sessions", Codex's "new session", the generic card's
 * neutral wording). A new client with a known install kind still needs no UI change.
 */
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
      if (config.client === "claude_desktop") {
        // Quit first: Claude Desktop writes this file when it quits, overwriting an edit made
        // while it was running.
        return [
          { key: "quit", text: t("steps.quitFirst", { app: config.title }) },
          {
            key: "open",
            text: path ? t("steps.openJson") : t("steps.openJsonNoPath"),
            extra: path,
          },
          {
            key: "paste",
            text: t("steps.pasteJson"),
            extra: (
              <>
                {snippet}
                <ExistingFile config={config} />
              </>
            ),
          },
          { key: "reopen", text: t("steps.saveAndReopen", { app: config.title }) },
        ];
      }
      if (config.client === "generic") {
        // A bare server definition: every client has its own config format.
        return [
          { key: "open", text: t("steps.genericOpen") },
          { key: "add", text: t("steps.genericAdd"), extra: snippet },
        ];
      }
      return [
        { key: "open", text: path ? t("steps.openJson") : t("steps.openJsonNoPath"), extra: path },
        { key: "paste", text: t("steps.pasteJsonMerge"), extra: snippet },
        { key: "restart", text: t("steps.restart") },
      ];
    case "toml_snippet":
      return [
        { key: "open", text: path ? t("steps.openToml") : t("steps.openTomlNoPath"), extra: path },
        {
          key: "paste",
          text: t("steps.pasteToml"),
          extra: (
            <>
              {snippet}
              <ReplaceTomlTable content={config.content} />
            </>
          ),
        },
        {
          key: "restart",
          text: config.client === "codex" ? t("steps.restartCodex") : t("steps.restart"),
        },
      ];
    case "shell_command":
      return [
        { key: "terminal", text: t("steps.openTerminal") },
        {
          key: "run",
          text: t("steps.runCommand"),
          extra: (
            <>
              {snippet}
              <RemoveFirst config={config} />
            </>
          ),
        },
        ...(config.client === "claude_code"
          ? [{ key: "after", text: t("steps.claudeCodeAfter") }]
          : []),
      ];
  }
}

/**
 * For a config file that isn't new: with an `mcpServers` section, just our entry goes inside it;
 * with other settings but no `mcpServers` (Claude Desktop writes "preferences" itself), the
 * whole `"mcpServers": { … }` key goes next to them. The full snippet fits neither.
 */
function ExistingFile({ config }: { config: McpClientConfig }) {
  const { t } = useTranslation("connect");
  const entry = mcpServerEntry(config.content);
  const key = mcpServersKey(config.content);
  const name = mcpServerName(config.content);
  return (
    <>
      {entry ? (
        <div className="space-y-2 pt-1">
          <p>{t("steps.pasteJsonEntry", { name })}</p>
          <CodeBlock code={entry} copyLabel={t("copyEntry", { app: config.title })} />
        </div>
      ) : null}
      {key ? (
        <div className="space-y-2 pt-1">
          <p>{t("steps.pasteJsonKey")}</p>
          <CodeBlock code={key} copyLabel={t("copyKey", { app: config.title })} />
        </div>
      ) : null}
    </>
  );
}

/**
 * Set up once already (e.g. before moving the app)? Adding the lines again would define the
 * table twice, which is invalid TOML: replace the existing lines instead.
 */
function ReplaceTomlTable({ content }: { content: string }) {
  const { t } = useTranslation("connect");
  const table = tomlTable(content);
  return table ? <p className="pt-1">{t("steps.replaceToml", { table })}</p> : null;
}

/** `claude mcp add` fails when the server exists already: remove it first, then add again. */
function RemoveFirst({ config }: { config: McpClientConfig }) {
  const { t } = useTranslation("connect");
  const remove = claudeMcpRemove(config.content);
  if (!remove) return null;
  return (
    <div className="space-y-2 pt-1">
      <p>{t("steps.removeFirst")}</p>
      <CodeBlock code={remove} copyLabel={t("copyRemoveCommand", { app: config.title })} />
    </div>
  );
}

/** The config file location in monospace with its own copy button. */
function ConfigPath({ path, app }: { path: string; app: string }) {
  const { t } = useTranslation("connect");
  return (
    <div className="pl-concentric flex items-center gap-2 bg-muted py-1.5 pr-1.5 pl-3 [--pl-pad:1rem]">
      <code className="min-w-0 flex-1 font-mono text-[13px] break-all">{path}</code>
      <CopyButton text={path} label={t("copyPath", { app })} variant="ghost" />
    </div>
  );
}
