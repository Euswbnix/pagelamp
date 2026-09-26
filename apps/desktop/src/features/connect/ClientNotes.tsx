import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { McpClient, McpNoteCode } from "@/api/types";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

// Codes this build has a translation for (connect.json → noteCodes). Anything else, e.g. a
// code newer than this build, is shown as the backend's English text.
const KNOWN_CODES = [
  "works_on_all_claude_plans",
  "admins_may_disable_extensions",
  "needs_paid_claude_plan",
  "codex_config_shared_with_chatgpt_desktop",
  "codex_plus_and_edu_documented",
  "free_go_undocumented",
  "restart_client_after_change",
  "custom_data_dir",
  "generic_stdio_client",
] as const satisfies readonly McpNoteCode[];
type KnownCode = (typeof KNOWN_CODES)[number];

function isKnown(code: McpNoteCode | undefined): code is KnownCode {
  return code !== undefined && (KNOWN_CODES as readonly string[]).includes(code);
}

// Shown once, prominently, at the top of the page (TemporaryLocationWarning), not per app.
const PAGE_LEVEL: readonly McpNoteCode[] = ["run_from_temporary_location"];

// Notes an app's numbered steps already say, in words that fit that app (InstallSteps). Claude
// Desktop's steps say to quit BEFORE editing (quit_before_editing; an older backend's "quit and
// reopen after changing" would contradict them); Claude Code has no config file to "change"
// (new sessions pick it up); Codex can also just start a new session; the generic card's steps
// are the generic_stdio_client advice.
const COVERED_BY_STEPS: Partial<Record<McpClient, readonly McpNoteCode[]>> = {
  claude_desktop: ["restart_client_after_change", "quit_before_editing"],
  claude_code: ["restart_client_after_change"],
  codex: ["restart_client_after_change"],
  generic: ["generic_stdio_client"],
};

/** Whether a note with this code appears in `client`'s card (not the page, not its steps). */
export function showsOnCard(client: McpClient, code: McpNoteCode): boolean {
  return !PAGE_LEVEL.includes(code) && !(COVERED_BY_STEPS[client] ?? []).includes(code);
}

/**
 * The backend's notes for one AI app (plan availability, restart hints…), shown prominently.
 * Each note is localised from its code (`note_codes[i]` belongs to `notes[i]`); a note whose
 * code is missing or unknown to this build is shown as the backend's English text.
 */
export function ClientNotes({
  client,
  notes,
  codes,
}: {
  client: McpClient;
  notes: readonly string[];
  codes: readonly McpNoteCode[];
}) {
  const { t } = useTranslation("connect");
  const paired = codes.length === notes.length;
  const shown = notes.flatMap((note, index) => {
    const code = paired ? codes[index] : undefined;
    return code && !showsOnCard(client, code) ? [] : [{ note, code }];
  });
  if (shown.length === 0) return null;
  return (
    // role="note" instead of Alert's default role="alert": this is static advice, not an
    // urgent message, so screen readers shouldn't interrupt with it on page load.
    <Alert role="note" className="border-info/30 bg-info/5 px-3 py-2.5 *:[svg]:text-info">
      <Info aria-hidden />
      <AlertTitle>{t("notes.title")}</AlertTitle>
      <AlertDescription className="text-foreground">
        <ul className="mt-1 list-disc space-y-1 pl-4">
          {shown.map(({ note, code }) => {
            const key = `${code ?? ""}:${note}`;
            return isKnown(code) ? (
              <li key={key}>{t(`noteCodes.${code}`)}</li>
            ) : (
              <li key={key} lang="en">
                {note}
              </li>
            );
          })}
        </ul>
      </AlertDescription>
    </Alert>
  );
}
