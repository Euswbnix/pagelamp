import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { McpClient, McpNoteCode } from "@/api/types";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

const KNOWN_CODES: readonly McpNoteCode[] = [
  "works_on_all_claude_plans",
  "admins_may_disable_extensions",
  "needs_paid_claude_plan",
  "codex_config_shared_with_chatgpt_desktop",
  "codex_plus_and_edu_documented",
  "free_go_undocumented",
  "restart_client_after_change",
  "custom_data_dir",
  "generic_stdio_client",
];

// Shown once, prominently, at the top of the page (TemporaryLocationWarning), not per app.
const PAGE_LEVEL: readonly McpNoteCode[] = ["run_from_temporary_location"];

// Notes an app's numbered steps already cover (in the right order). Claude Desktop's steps
// say to quit BEFORE editing; "quit and reopen after changing" would contradict them.
// Plain strings: `quit_before_editing` may be newer than this build's types.
const COVERED_BY_STEPS: Partial<Record<McpClient, readonly string[]>> = {
  claude_desktop: ["restart_client_after_change", "quit_before_editing"],
};

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
  const covered = COVERED_BY_STEPS[client] ?? [];
  const shown = notes.flatMap((note, index) => {
    const code = paired ? codes[index] : undefined;
    const hidden = code && (PAGE_LEVEL.includes(code) || covered.includes(code));
    return hidden ? [] : [{ note, code }];
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
            return code && KNOWN_CODES.includes(code) ? (
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
