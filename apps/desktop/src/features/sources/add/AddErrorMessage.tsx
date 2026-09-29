import { useTranslation } from "react-i18next";
import type { ApiError, ErrorKind } from "@/api/errors";
import { FieldError } from "@/components/ui/field";

/**
 * What a failed add/replace was about; picks the most helpful wording.
 * `canvas` = address + token (add form), `token` = the token alone (replace dialog).
 */
export type AddTarget = "folder" | "feed" | "canvas" | "token";

// The kinds an add/replace call can realistically fail with. Anything else (busy, internal…)
// falls back to the shared wording in common.json.
const FIELD_KINDS = ["invalid", "not_found", "network", "auth"] as const;
type FieldKind = (typeof FIELD_KINDS)[number];

function isFieldKind(kind: ErrorKind): kind is FieldKind {
  return (FIELD_KINDS as readonly ErrorKind[]).includes(kind);
}

/** Explanation of a failed add/replace, chosen by `error.kind` (never by the message). */
export function useAddErrorText() {
  const { t } = useTranslation("sources");
  const { t: tc } = useTranslation();
  return (target: AddTarget, error: ApiError): string =>
    isFieldKind(error.kind) ? t(`addErrors.${target}.${error.kind}`) : tc(`errors.${error.kind}`);
}

/**
 * Error under a field: our explanation first, the backend's message (never a secret) as detail.
 * Renders nothing without an error. `role="alert"` (from FieldError) announces it.
 */
export function AddErrorMessage({
  id,
  target,
  error,
}: {
  id: string;
  target: AddTarget;
  error: ApiError | null;
}) {
  const errorText = useAddErrorText();
  if (!error) return null;
  return (
    <FieldError id={id}>
      <p>{errorText(target, error)}</p>
      {error.message ? <p className="mt-0.5 text-xs opacity-80">{error.message}</p> : null}
    </FieldError>
  );
}

/** Joins the ids of the hints/errors that describe an input (skipping empty ones). */
export function describedBy(...ids: Array<string | false | null | undefined>): string | undefined {
  const joined = ids.filter(Boolean).join(" ");
  return joined === "" ? undefined : joined;
}
