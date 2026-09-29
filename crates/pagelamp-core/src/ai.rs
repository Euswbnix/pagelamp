//! Plain data types of PageLamp's own model calls (v0.3 M1; docs/design/v0.3-model-access.md
//! §3.4, §3.8, §4.1). They cross the facade, so they are `Serialize + JsonSchema` with stable
//! snake_case names that the UIs translate; UIs branch on these codes, never on messages.
//!
//! The model code itself lives in `pagelamp-llm` (network) and `ai_gate` (the only producer of
//! prompt text); this module has no logic beyond names.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Why a model call was refused before anything was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    /// The course's `ai_policy` is `prohibited`.
    CoursePolicyProhibited,
    /// The student switched AI access off for the course.
    CourseAiTurnedOff,
    CourseHidden,
    NoReadableMaterials,
    /// A cloud backend, and the student answered "may not be shared" for the course.
    MaterialSharingNotAllowed,
    /// A coding-plan endpoint or key: its terms forbid use from other applications.
    CodingPlanKey,
    DisclosureNotAcknowledged,
    NoModelChosen,
    /// The run would go past the monthly budget (API keys only).
    BudgetReached,
    /// An API-key model without a known price, and the student hasn't acknowledged that the
    /// budget can't be enforced for it.
    PriceUnknownNotAcknowledged,
    /// The weekly run cap of the ChatGPT-plan mode.
    WeeklyRunCapReached,
    BackendDisabledInThisBuild,
}

impl BlockReason {
    pub const ALL: [BlockReason; 12] = [
        BlockReason::CoursePolicyProhibited,
        BlockReason::CourseAiTurnedOff,
        BlockReason::CourseHidden,
        BlockReason::NoReadableMaterials,
        BlockReason::MaterialSharingNotAllowed,
        BlockReason::CodingPlanKey,
        BlockReason::DisclosureNotAcknowledged,
        BlockReason::NoModelChosen,
        BlockReason::BudgetReached,
        BlockReason::PriceUnknownNotAcknowledged,
        BlockReason::WeeklyRunCapReached,
        BlockReason::BackendDisabledInThisBuild,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BlockReason::CoursePolicyProhibited => "course_policy_prohibited",
            BlockReason::CourseAiTurnedOff => "course_ai_turned_off",
            BlockReason::CourseHidden => "course_hidden",
            BlockReason::NoReadableMaterials => "no_readable_materials",
            BlockReason::MaterialSharingNotAllowed => "material_sharing_not_allowed",
            BlockReason::CodingPlanKey => "coding_plan_key",
            BlockReason::DisclosureNotAcknowledged => "disclosure_not_acknowledged",
            BlockReason::NoModelChosen => "no_model_chosen",
            BlockReason::BudgetReached => "budget_reached",
            BlockReason::PriceUnknownNotAcknowledged => "price_unknown_not_acknowledged",
            BlockReason::WeeklyRunCapReached => "weekly_run_cap_reached",
            BlockReason::BackendDisabledInThisBuild => "backend_disabled_in_this_build",
        }
    }
}

/// Why a model call failed (the facade's `SourceErrorKind` for models).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelErrorKind {
    /// ChatGPT / Claude plan modes: not signed in.
    NotSignedIn,
    /// The key or sign-in was refused (401/403).
    AuthRejected,
    /// Out of credit or over a spend limit (402, quota 429s): never retried.
    BillingOrQuota,
    /// A plan's usage limit (Codex / Claude Code).
    UsageLimit,
    /// Too many requests; see `retry_after_secs`.
    RateLimited,
    /// The service is overloaded or failing (5xx, 529).
    Overloaded,
    /// The service rejected the request (400).
    InvalidRequest,
    ModelNotFound,
    ContextTooLong,
    /// The model declined to answer.
    Refused,
    /// The provider's content filter stopped the answer.
    ContentFiltered,
    Network,
    Timeout,
    /// The answer could not be used (unparseable, failed validation after one repair).
    BadOutput,
    /// The local runtime (Codex) is not installed.
    RuntimeMissing,
    /// A downloaded runtime failed its checksum or signature check.
    RuntimeVerifyFailed,
    /// Codex: "requires a newer version of Codex" (design §2.3).
    RuntimeOutdated,
    /// The backend or model can't do what was asked.
    Unsupported,
}

impl ModelErrorKind {
    pub const ALL: [ModelErrorKind; 18] = [
        ModelErrorKind::NotSignedIn,
        ModelErrorKind::AuthRejected,
        ModelErrorKind::BillingOrQuota,
        ModelErrorKind::UsageLimit,
        ModelErrorKind::RateLimited,
        ModelErrorKind::Overloaded,
        ModelErrorKind::InvalidRequest,
        ModelErrorKind::ModelNotFound,
        ModelErrorKind::ContextTooLong,
        ModelErrorKind::Refused,
        ModelErrorKind::ContentFiltered,
        ModelErrorKind::Network,
        ModelErrorKind::Timeout,
        ModelErrorKind::BadOutput,
        ModelErrorKind::RuntimeMissing,
        ModelErrorKind::RuntimeVerifyFailed,
        ModelErrorKind::RuntimeOutdated,
        ModelErrorKind::Unsupported,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ModelErrorKind::NotSignedIn => "not_signed_in",
            ModelErrorKind::AuthRejected => "auth_rejected",
            ModelErrorKind::BillingOrQuota => "billing_or_quota",
            ModelErrorKind::UsageLimit => "usage_limit",
            ModelErrorKind::RateLimited => "rate_limited",
            ModelErrorKind::Overloaded => "overloaded",
            ModelErrorKind::InvalidRequest => "invalid_request",
            ModelErrorKind::ModelNotFound => "model_not_found",
            ModelErrorKind::ContextTooLong => "context_too_long",
            ModelErrorKind::Refused => "refused",
            ModelErrorKind::ContentFiltered => "content_filtered",
            ModelErrorKind::Network => "network",
            ModelErrorKind::Timeout => "timeout",
            ModelErrorKind::BadOutput => "bad_output",
            ModelErrorKind::RuntimeMissing => "runtime_missing",
            ModelErrorKind::RuntimeVerifyFailed => "runtime_verify_failed",
            ModelErrorKind::RuntimeOutdated => "runtime_outdated",
            ModelErrorKind::Unsupported => "unsupported",
        }
    }
}

/// How hard the model should think. Mapped per wire: "lowest" is the cheapest setting the
/// model allows (`none` / `minimal`, or `low` where thinking can't be turned off).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    #[default]
    Lowest,
    Low,
    Medium,
    High,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Lowest => "lowest",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_their_json_names() {
        for reason in BlockReason::ALL {
            assert_eq!(serde_json::to_value(reason).unwrap(), reason.as_str());
        }
        for kind in ModelErrorKind::ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
        }
        for effort in [Effort::Lowest, Effort::Low, Effort::Medium, Effort::High] {
            assert_eq!(serde_json::to_value(effort).unwrap(), effort.as_str());
        }
    }
}
