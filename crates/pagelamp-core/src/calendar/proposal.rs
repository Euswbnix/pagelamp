//! Proposals and the accepted calendar, as the facade shows them (docs/design/
//! v0.3-course-calendar.md §4, §7.10). A proposal has no effect until the student accepts it;
//! an accepted calendar is in force until the student's next accept or dates form.
//!
//! Both carry material text (labels, topics, quotes): local display only, never in structure
//! outputs (§7.11).

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::CourseCalendar;
use super::assemble::{Assembled, CalendarChange, CalendarConflict, ProposedDate};
use super::validate::DropCount;
use crate::ai_gate::ManifestEntry;
use crate::model::Timestamp;
use crate::term::{AiLabel, CalendarOrigin, CoursePhase};

/// A calendar proposed from the course's materials (scan, AI reading or the student's AI app).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarProposal {
    /// The `course_calendars` row; accept or dismiss by it.
    pub id: i64,
    pub course_id: String,
    pub origin: CalendarOrigin,
    pub calendar: CourseCalendar,
    pub dates: Vec<ProposedDate>,
    /// Choices between two for the student (V5, V7, V8).
    pub conflicts: Vec<CalendarConflict>,
    /// What the checks dropped, by reason.
    pub dropped: Vec<DropCount>,
    /// Many claims dropped, the week table dropped, or the stated term from another year.
    pub low_quality: bool,
    /// No conflicts and not low quality: "accept all that pass" may take it.
    pub passing: bool,
    /// Set for origins `ai` and `ai_app`.
    pub ai_label: Option<AiLabel>,
    /// Show the one-time question (b) reminder with this proposal (D37 option 2, D49).
    pub sharing_reminder: bool,
    /// Today's week once accepted.
    pub resulting_week_today: Option<u32>,
    pub resulting_phase: CoursePhase,
    /// What accepting would change compared with the calendar in force.
    pub changes: Vec<CalendarChange>,
    pub created_at: Timestamp,
}

/// The calendar in force for a course, with where its dates came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AcceptedCalendar {
    pub id: i64,
    pub origin: CalendarOrigin,
    pub calendar: CourseCalendar,
    /// The dates with their quotes (empty for the student's own dates).
    pub dates: Vec<ProposedDate>,
    pub ai_label: Option<AiLabel>,
    pub accepted_at: Timestamp,
    /// A quoted material changed and a quote is no longer in it (§7.8). Still in force.
    pub stale: bool,
    /// When the first such change was seen.
    pub stale_since: Option<NaiveDate>,
    /// Ids of the quoted materials that changed.
    pub changed_materials: Vec<String>,
}

/// A proposal checked and assembled, before storage gives it an id (schema v4
/// `course_calendars`, state `proposed`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewProposal {
    pub course_id: String,
    pub origin: CalendarOrigin,
    pub assembled: Assembled,
    /// Set for origins `ai` and `ai_app`.
    pub ai_label: Option<AiLabel>,
    pub sharing_reminder: bool,
    /// The materials read: content hash and chunks (`manifest_json`).
    pub manifest: Vec<ManifestEntry>,
    /// The candidate set's fingerprint (`ReadingInputs::fingerprint`).
    pub fingerprint: String,
}

impl NewProposal {
    /// The proposal as the facade shows it, once stored as row `id`.
    pub fn into_proposal(self, id: i64, created_at: Timestamp) -> CalendarProposal {
        let assembled = self.assembled;
        CalendarProposal {
            id,
            course_id: self.course_id,
            origin: self.origin,
            calendar: assembled.calendar,
            dates: assembled.dates,
            conflicts: assembled.conflicts,
            dropped: assembled.dropped,
            low_quality: assembled.low_quality,
            passing: assembled.passing,
            ai_label: self.ai_label,
            sharing_reminder: self.sharing_reminder,
            resulting_week_today: assembled.resulting_week_today,
            resulting_phase: assembled.resulting_phase,
            changes: assembled.changes,
            created_at,
        }
    }
}
