import type { AiErrorKind, BlockReason, ModelErrorKind } from "./provisional/ai";
import type { AppError, AppErrorKind } from "./types";

/** PROVISIONAL (M1): `AppErrorKind` once the backend's generated types add the AI kinds. */
export type ErrorKind = AppErrorKind | AiErrorKind;

/** The M1 fields of `AppError` (design §3.4), set only for their kinds. */
export interface ApiErrorDetails {
  /** kind `blocked`: why the facade refused the run. */
  blocked?: BlockReason | null;
  /** kind `model`: what went wrong talking to the model. */
  model_error?: ModelErrorKind | null;
  /** When to try again (rate limits), in seconds. */
  retry_after_secs?: number | null;
}

// Every AppErrorKind, as a Record so a kind added in Rust (and regenerated) fails to compile
// here until it is listed: an unlisted kind from Tauri would otherwise turn into `internal`.
const KINDS: Record<ErrorKind, true> = {
  auth: true,
  network: true,
  invalid: true,
  not_found: true,
  ambiguous: true,
  busy: true,
  schema_too_new: true,
  schema_too_old: true,
  internal: true,
  blocked: true,
  model: true,
  cancelled: true,
};

/** The only error type API calls reject with. UI code branches on `kind`, never on `message`. */
export class ApiError extends Error implements ApiErrorDetails {
  readonly kind: ErrorKind;
  readonly blocked: BlockReason | null;
  readonly model_error: ModelErrorKind | null;
  readonly retry_after_secs: number | null;

  constructor(kind: ErrorKind, message: string, details: ApiErrorDetails = {}) {
    super(message);
    this.name = "ApiError";
    this.kind = kind;
    this.blocked = details.blocked ?? null;
    this.model_error = details.model_error ?? null;
    this.retry_after_secs = details.retry_after_secs ?? null;
  }
}

type SerialisedError = Omit<AppError, "kind"> & { kind: ErrorKind } & ApiErrorDetails;

function isAppError(value: unknown): value is SerialisedError {
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
  if (isAppError(error)) {
    return new ApiError(error.kind, error.message, {
      blocked: error.blocked,
      model_error: error.model_error,
      retry_after_secs: error.retry_after_secs,
    });
  }
  if (error instanceof Error) return new ApiError("internal", error.message);
  if (typeof error === "string") return new ApiError("internal", error);
  return new ApiError("internal", "Unexpected error");
}

export function isApiError(error: unknown, kind?: ErrorKind): error is ApiError {
  return error instanceof ApiError && (kind === undefined || error.kind === kind);
}
