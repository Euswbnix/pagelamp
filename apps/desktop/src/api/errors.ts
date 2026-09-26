import type { AppError, AppErrorKind } from "./types";

const KINDS: readonly AppErrorKind[] = [
  "auth",
  "network",
  "invalid",
  "not_found",
  "ambiguous",
  "busy",
  "internal",
];

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
    typeof v.message === "string" &&
    typeof v.kind === "string" &&
    KINDS.includes(v.kind as AppErrorKind)
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
