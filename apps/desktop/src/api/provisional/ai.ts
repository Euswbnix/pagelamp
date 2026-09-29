// PROVISIONAL M1 contract types (docs/design/v0.3-model-access.md §3.4, §3.5, §3.8), hand-written
// from the names agreed with the backend (2026-09-28) until its type-only commit lands in
// generated.ts (target 2026-10-18). Then this file is deleted and types.ts re-exports the
// generated types instead — keep shapes exactly as serde sends them (snake_case, tagged enums,
// Option<T> as `?: T | null`, money as integer micro-USD).

import type { Course, IsoDate, Timestamp } from "../types";

// ----- enums -----------------------------------------------------------------------------------

export type AiFeature = "study_plan" | "weekly_explanation" | "weekly_note" | "course_calendar";
export const AI_FEATURES: readonly AiFeature[] = [
  "study_plan",
  "weekly_explanation",
  "weekly_note",
  "course_calendar",
];

/** Which model backend (serde tag "kind"). */
export type BackendRef =
  | { kind: "codex" }
  | { kind: "claude_code" }
  | { kind: "provider"; provider_id: string };

export type Effort = "lowest" | "low" | "medium" | "high";
export const EFFORTS: readonly Effort[] = ["lowest", "low", "medium", "high"];

export interface ModelChoice {
  backend: BackendRef;
  model: string;
  effort: Effort;
}

/** Question (b): may this course's materials be shared with an AI service? (§4.1, D37) */
export type MaterialSharing = "unanswered" | "allowed" | "not_sure" | "not_allowed";
export const MATERIAL_SHARING: readonly MaterialSharing[] = [
  "unanswered",
  "allowed",
  "not_sure",
  "not_allowed",
];

/** Why the facade refuses a run (`AppError.blocked`, `CostEstimate.would_block`). */
export type BlockReason =
  | "course_policy_prohibited"
  | "course_ai_turned_off"
  | "course_hidden"
  | "no_readable_materials"
  | "material_sharing_not_allowed"
  | "coding_plan_key"
  | "disclosure_not_acknowledged"
  | "no_model_chosen"
  | "budget_reached"
  | "price_unknown_not_acknowledged"
  | "weekly_run_cap_reached"
  | "backend_disabled_in_this_build";
export const BLOCK_REASONS: readonly BlockReason[] = [
  "course_policy_prohibited",
  "course_ai_turned_off",
  "course_hidden",
  "no_readable_materials",
  "material_sharing_not_allowed",
  "coding_plan_key",
  "disclosure_not_acknowledged",
  "no_model_chosen",
  "budget_reached",
  "price_unknown_not_acknowledged",
  "weekly_run_cap_reached",
  "backend_disabled_in_this_build",
];

/** What went wrong talking to a model (`AppError.model_error`). */
export type ModelErrorKind =
  | "not_signed_in"
  | "auth_rejected"
  | "billing_or_quota"
  | "usage_limit"
  | "rate_limited"
  | "overloaded"
  | "invalid_request"
  | "model_not_found"
  | "context_too_long"
  | "refused"
  | "content_filtered"
  | "network"
  | "timeout"
  | "bad_output"
  | "runtime_missing"
  | "runtime_verify_failed"
  | "runtime_outdated"
  | "unsupported";
export const MODEL_ERROR_KINDS: readonly ModelErrorKind[] = [
  "not_signed_in",
  "auth_rejected",
  "billing_or_quota",
  "usage_limit",
  "rate_limited",
  "overloaded",
  "invalid_request",
  "model_not_found",
  "context_too_long",
  "refused",
  "content_filtered",
  "network",
  "timeout",
  "bad_output",
  "runtime_missing",
  "runtime_verify_failed",
  "runtime_outdated",
  "unsupported",
];

/** The AppErrorKinds M1 adds (next to auth, network, …, schema_too_old). */
export type AiErrorKind = "blocked" | "model" | "cancelled";

// ----- disclosure facts (codes + params; the UI translates, never the backend) -------------------

export type SentData = "structure" | "material_text";

export type TrainingFact =
  | { kind: "no_training" }
  | { kind: "may_train"; how_to_turn_off_url?: string | null }
  | { kind: "may_train_free_tier" }
  | { kind: "unknown" };

export type RetentionFact =
  | { kind: "not_stored" }
  | { kind: "stored_days"; days: number }
  | { kind: "provider_terms" }
  | { kind: "on_device" };

export type CostFact = "api_billing" | "plan_credits" | "free_on_device" | "cloud_via_local";

/**
 * Everything the disclosure sheet says about one backend (Canvas API Policy §2E, all six items).
 * The limitations-and-risks and ownership paragraphs are always shown; `version` hashes them
 * too, so any change asks the student again.
 */
export interface DisclosureFacts {
  version: number;
  sends: SentData[];
  recipient: { name: string; terms_url?: string | null };
  training: TrainingFact;
  retention: RetentionFact;
  /** Edu/Enterprise admins can see the runs (Codex). */
  admin_visibility: boolean;
  min_age?: number | null;
  /** Under 18 needs a guardian's permission (OpenAI 13+). */
  guardian_permission: boolean;
  cost: CostFact;
  on_device: boolean;
}

// ----- setup -------------------------------------------------------------------------------------

export type WireKind = "openai_responses" | "anthropic_messages" | "ollama" | "chat_completions";

export interface ProviderPreset {
  id: string;
  label: string;
  wire: WireKind;
  default_base_url?: string | null;
  needs_key: boolean;
  base_url_editable: boolean;
  /** Shown before a key exists: the same facts the disclosure sheet renders. */
  data_policy: DisclosureFacts;
}

export interface ModelProviderRecord {
  provider_id: string;
  preset: string;
  label: string;
  wire: WireKind;
  base_url: string;
  /** A loopback base URL. */
  on_device: boolean;
  /** The only part of the key the UI ever sees. */
  key_last4?: string | null;
  created_at: Timestamp;
}

export interface ModelInfo {
  id: string;
  label?: string | null;
  /** False for Ollama `-cloud` / `remote_host` models: "runs in the cloud". */
  on_device: boolean;
  price_known: boolean;
  context_window?: number | null;
  reasoning_always_on: boolean;
  suggested_for: AiFeature[];
}

export type StructuredOutputTier = "native_schema" | "json_object" | "prompt_only";

export interface ProbeReport {
  ok: boolean;
  latency_ms?: number | null;
  structured_output_tier?: StructuredOutputTier | null;
  thinking_always_on: boolean;
  error?: ModelErrorKind | null;
}

export interface LocalServer {
  kind: "ollama" | "lm_studio";
  base_url: string;
  running: boolean;
}

export type BackendKind = "api_key" | "local" | "codex" | "claude_code";
export type BackendState = "ready" | "needs_setup" | "needs_disclosure" | "unavailable";
export type BackendProblem =
  | "key_missing"
  | "server_not_running"
  | "model_missing"
  | "disclosure_changed";

export interface AiBackendStatus {
  backend: BackendRef;
  label: string;
  kind: BackendKind;
  state: BackendState;
  problems: BackendProblem[];
  /** For API-key and local backends. */
  provider?: ModelProviderRecord | null;
  disclosure: DisclosureFacts;
  /** The disclosure version the student acknowledged, if any. */
  disclosure_acknowledged?: number | null;
}

export interface FeatureRouting {
  feature: AiFeature;
  choice?: ModelChoice | null;
}

export interface BudgetStatus {
  /** The soft monthly cap for API keys (D18; default US$5). None = no cap. */
  monthly_micro_usd?: number | null;
  spent_micro_usd: number;
  warn_at_percent: number;
}

export interface AiStatus {
  /** In priority order. */
  backends: AiBackendStatus[];
  features: FeatureRouting[];
  budget: BudgetStatus;
}

// ----- estimate and usage ------------------------------------------------------------------------

/** serde tag "feature". */
export type EstimateRequest =
  | { feature: "study_plan"; courses: string[]; days: number }
  | { feature: "weekly_explanation"; course: string; week?: number | null }
  | { feature: "weekly_note" }
  | { feature: "course_calendar"; courses: string[] };

/** "≈ $x" before Generate: an upper bound, not a guess. */
export interface CostEstimate {
  /** None when the price is unknown or doesn't apply (plans). 0 on device. */
  micro_usd_upper?: number | null;
  input_tokens: number;
  max_output_tokens: number;
  reasoning_allowance: number;
  repair_possible: boolean;
  price_known: boolean;
  /** The run would be refused right now (budget, disclosure, question (b), …). */
  would_block?: BlockReason | null;
}

export interface UsageRow {
  backend_label: string;
  backend_kind: BackendKind;
  model: string;
  feature: AiFeature;
  runs: number;
  /** The total, cached ones included. */
  input_tokens: number;
  /** The total, reasoning included. */
  output_tokens: number;
  /** Of the output, how much was reasoning. */
  reasoning_tokens: number;
  /** None when the price is unknown (tokens only). 0 on device. */
  micro_usd?: number | null;
  /** At least one run's cost was estimated (a cancelled run, a price overlay). */
  estimated: boolean;
}

/** Mode A has no money budget; a weekly run cap instead (design §2.3). */
export interface ModeAUsage {
  runs_this_week: number;
  weekly_cap?: number | null;
}

export interface UsageSummary {
  /** The first day of the month. */
  month: IsoDate;
  rows: UsageRow[];
  total_micro_usd: number;
  budget: BudgetStatus;
  /** Null when Codex isn't set up. */
  mode_a?: ModeAUsage | null;
}

// ----- generated content --------------------------------------------------------------------------

/** Token counts of one run (serde `TokenUsage`). */
export interface TokenUsage {
  /** The total, cached ones included. */
  input_tokens: number;
  cached_input_tokens: number;
  /** Reasoning included. */
  output_tokens: number;
  /** Null when the provider doesn't report it. */
  reasoning_tokens?: number | null;
}

/**
 * Embedded in every result (design §3.8); the "AI-generated · …" label is built from it. The
 * context summary and prompt version arrive with the backend's type-only commit.
 */
export interface GenerationMeta {
  generation_id: string;
  feature: AiFeature;
  backend_label: string;
  model: string;
  created_at: Timestamp;
  usage: TokenUsage;
  est_cost_micro_usd?: number | null;
  /** Some numbers are estimates (true after a cancel). */
  estimated: boolean;
}

// ----- mode A: the ChatGPT plan through official Codex (M2; design §2.3) -------------------------
// Names agreed with the backend on 2026-09-28; M2 is theirs after M1, so these may still move.

export type CodexRuntimeState = "not_installed" | "installed" | "unsupported_platform";
export type CodexSource = "managed" | "system";

export interface CodexRuntime {
  state: CodexRuntimeState;
  source: CodexSource;
  installed_version?: string | null;
  pinned_version: string;
  /** The compressed download, for "≈70 MB". */
  download_bytes: number;
  /** Windows arm64: runs, but untested (D14). */
  untested_platform: boolean;
}

/** What a RuntimeOutdated error means right now; the facade decides (rule 12). */
export type CodexOutdatedAction = "none" | "install_pin" | "update_pagelamp";

export type CodexLoginState = "signed_out" | "chatgpt" | "api_key";
/** A best guess from `codex login status`; `unknown` whenever it doesn't say (until A7). */
export type ChatGptPlanType =
  | "free"
  | "go"
  | "plus"
  | "pro"
  | "business"
  | "edu"
  | "enterprise"
  | "unknown";

export interface CodexLogin {
  state: CodexLoginState;
  plan_type?: ChatGptPlanType | null;
}

export interface SystemCodex {
  version: string;
  in_tested_range: boolean;
}

export interface CodexStatus {
  runtime: CodexRuntime;
  outdated_action: CodexOutdatedAction;
  login: CodexLogin;
  /** False when `codex exec` doesn't work on this plan (Free/Go, D10; pending A7). */
  exec_available?: boolean | null;
  weekly_cap?: number | null;
  runs_this_week: number;
  /** An installed `codex` found on this computer (D12). */
  system_codex?: SystemCodex | null;
}

/** install_codex progress (serde tag "type"). A failure or cancel ends the call with an error. */
export type RuntimeEvent =
  | { type: "download_started"; total_bytes: number }
  | { type: "progress"; downloaded_bytes: number; total_bytes: number }
  | { type: "verifying" }
  | { type: "installing" }
  | { type: "done"; version: string };

export type CodexLoginMethod = "browser" | "device_code";

/** codex_login progress (serde tag "type"). */
export type LoginEvent =
  | { type: "browser_opened"; url?: string | null }
  | {
      type: "device_code";
      verification_url: string;
      user_code: string;
      expires_in_secs?: number | null;
    }
  | { type: "waiting" }
  | { type: "done" };

// ----- courses -----------------------------------------------------------------------------------

/** `material_sharing` joins `ai_policy` on the course types in schema v4. */
export type CourseWithSharing = Course & { material_sharing?: MaterialSharing | null };

export function materialSharing(course: Course): MaterialSharing {
  return (course as CourseWithSharing).material_sharing ?? "unanswered";
}

/** Same backend? (BackendRef is a tagged union, so compare by value.) */
export function sameBackend(a: BackendRef, b: BackendRef): boolean {
  if (a.kind !== b.kind) return false;
  return a.kind !== "provider" || a.provider_id === (b as { provider_id: string }).provider_id;
}

/** A stable string for a backend: a React key, a Map key, a query key part. */
export function backendKey(backend: BackendRef): string {
  return backend.kind === "provider" ? `provider:${backend.provider_id}` : backend.kind;
}
