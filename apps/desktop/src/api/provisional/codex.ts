// PROVISIONAL M2 contract types (design §2.3, §3.8: the ChatGPT plan through official Codex),
// hand-written from the names agreed with the backend on 2026-09-28. M2 is the backend's after
// M1, so these may still move; when its types reach generated.ts this file goes, like the M1
// provisional file did.

/** Mode A has no money budget; a weekly run cap instead (design §2.3). */
export interface ModeAUsage {
  runs_this_week: number;
  weekly_cap?: number | null;
}

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
