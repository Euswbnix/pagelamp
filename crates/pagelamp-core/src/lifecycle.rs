//! Course lifecycle: is the course upcoming, current, finishing or over, and should PageLamp
//! suggest removing it (docs/design/v0.3-course-calendar.md §8.1)? Computed at every read,
//! never stored; pure functions, callers pass `today`.
//!
//! The lifecycle, not the phase, decides every exclusion: Ended, Inactive and Upcoming courses
//! leave the week-based views (D43), deadlines are never filtered by it (§8.2).

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::Confidence;
use crate::term::evidence::EvidenceItem;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Upcoming,
    Current,
    /// Over by its dates, but exams, recent activity or a future event keep it going.
    Finishing,
    Ended,
    /// No dates at all and no activity for months (orientation, club and training sites).
    Inactive,
    Unknown,
}

impl LifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            LifecycleState::Upcoming => "upcoming",
            LifecycleState::Current => "current",
            LifecycleState::Finishing => "finishing",
            LifecycleState::Ended => "ended",
            LifecycleState::Inactive => "inactive",
            LifecycleState::Unknown => "unknown",
        }
    }

    /// Ended and Inactive → Past; Upcoming → Upcoming; everything else → Current.
    pub fn group(self) -> CourseGroup {
        match self {
            LifecycleState::Ended | LifecycleState::Inactive => CourseGroup::Past,
            LifecycleState::Upcoming => CourseGroup::Upcoming,
            LifecycleState::Current | LifecycleState::Finishing | LifecycleState::Unknown => {
                CourseGroup::Current
            }
        }
    }
}

/// How course lists group courses (the same in Tauri, Swift and the CLI).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CourseGroup {
    Current,
    Upcoming,
    Past,
}

impl CourseGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            CourseGroup::Current => "current",
            CourseGroup::Upcoming => "upcoming",
            CourseGroup::Past => "past",
        }
    }
}

/// The answer to a removal suggestion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnoozeKind {
    /// Don't suggest it for 14 days.
    NotNow,
    /// Never suggest it again.
    Keep,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CourseLifecycle {
    pub state: LifecycleState,
    pub group: CourseGroup,
    pub confidence: Confidence,
    /// Since when the state holds, when known (e.g. the day an Ended course ended).
    pub since: Option<NaiveDate>,
    /// When classes start (Upcoming courses: "Starts Jan 11"), when known.
    pub starts_on: Option<NaiveDate>,
    /// The latest material, announcement, event or module unlock date on or before today.
    pub last_activity: Option<NaiveDate>,
    /// The earliest course event (any kind but "other") from today to today + 30 days.
    pub next_event: Option<NaiveDate>,
    pub evidence_items: Vec<EvidenceItem>,
    /// Ended or Inactive, and the suggestion isn't snoozed. Hidden courses are suggested too.
    pub suggest_removal: bool,
    /// "I'm still taking this" until this date, when set.
    pub kept_current_until: Option<NaiveDate>,
}

impl CourseLifecycle {
    /// A course nothing is known about yet.
    pub fn unknown() -> Self {
        CourseLifecycle {
            state: LifecycleState::Unknown,
            group: LifecycleState::Unknown.group(),
            confidence: Confidence::Low,
            since: None,
            starts_on: None,
            last_activity: None,
            next_event: None,
            evidence_items: Vec::new(),
            suggest_removal: false,
            kept_current_until: None,
        }
    }
}
