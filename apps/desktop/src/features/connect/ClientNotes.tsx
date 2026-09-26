import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

/**
 * The backend's notes for one AI app (plan availability, restart hints…), shown prominently.
 * They arrive in English and are shown verbatim — never translated or reworded here.
 */
export function ClientNotes({ notes }: { notes: readonly string[] }) {
  const { t } = useTranslation("connect");
  if (notes.length === 0) return null;
  return (
    // role="note" instead of Alert's default role="alert": this is static advice, not an
    // urgent message, so screen readers shouldn't interrupt with it on page load.
    <Alert role="note" className="border-info/30 bg-info/5 px-3 py-2.5 *:[svg]:text-info">
      <Info aria-hidden />
      <AlertTitle>{t("notes.title")}</AlertTitle>
      <AlertDescription className="text-foreground">
        <ul lang="en" className="mt-1 list-disc space-y-1 pl-4">
          {notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      </AlertDescription>
    </Alert>
  );
}
