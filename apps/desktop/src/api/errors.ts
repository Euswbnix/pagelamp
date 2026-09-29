import type { AppError, AppErrorKind } from "./types";

// Every AppErrorKind, as a Record so a kind added in Rust (and regenerated) fails to compile
// here until it is listed: an unlisted kind from Tauri would otherwise turn into `internal`.
const KINDS: Record<AppErrorKind, true> = {
  auth: true,
  network: true,
  invalid: true,
  not_found: true,
  ambiguous: true,
  busy: true,
  schema_too_new: true,
  schema_too_old: true,
  internal: true,
};

/** The only error type API calls reject with. UI code branches on `kind`, never on `message`. */
export class ApiError extends Error implements AppError {
  readonly kind: AppErrorKind;

  constructor(kind: AppErrorKind, message: string) {
    super(message);
    this.name = "ApiError";
    this.kind = kind;
  }
}

function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.message === "string" && typeof v.kind === "string" && Object.hasOwn(KINDS, v.kind)
  );
}

/**
 * Normalise anything thrown by `invoke` (or the mock) into an ApiError.
 * Tauri rejects with the command's serialised `AppError` object; a plain string means Tauri
 * itself failed (unknown command, bad arguments) — treat that as internal.
 */
export function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  if (isAppError(error)) return new ApiError(error.kind, error.message);
  if (error instanceof Error) return new ApiError("internal", error.message);
  if (typeof error === "string") return new ApiError("internal", error);
  return new ApiError("internal", "Unexpected error");
}

export function isApiError(error: unknown, kind?: AppErrorKind): error is ApiError {
  return error instanceof ApiError && (kind === undefined || error.kind === kind);
}
