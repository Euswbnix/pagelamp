import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { McpNoteCode } from "@/api/types";
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

/**
 * The backend's notes for one AI app (plan availability, restart hints…), shown prominently.
 * Each note is localised from its code (`note_codes[i]` belongs to `notes[i]`); a note whose
 * code is missing or unknown to this build is shown as the backend's English text.
 */
export function ClientNotes({
  notes,
  codes,
}: {
  notes: readonly string[];
  codes: readonly McpNoteCode[];
}) {
  const { t } = useTranslation("connect");
  const paired = codes.length === notes.length;
  const shown = notes.flatMap((note, index) => {
    const code = paired ? codes[index] : undefined;
    return code && PAGE_LEVEL.includes(code) ? [] : [{ note, code }];
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
