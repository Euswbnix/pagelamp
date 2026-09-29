//! What a reader of the syllabus hands back (docs/design/v0.3-course-calendar.md §7.4): the
//! model's structured output, the student's AI app over MCP (`material_id` as the source), or
//! the deterministic scan. Only "date + verbatim quote + where"; core does every piece of
//! arithmetic (years, weeks, assembly) and checks every date against its quote
//! (`validate`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalendarExtraction {
    /// The term the materials say they are for ("Fall 2026").
    pub stated_term: StatedTerm,
    pub claims: Vec<ExtractedClaim>,
    /// Rows of a schedule table.
    pub weeks: Vec<ExtractedWeek>,
    /// What the materials don't state.
    pub not_found: Vec<NotFound>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatedTerm {
    pub text: Option<String>,
    pub quote: Option<String>,
    pub source: Option<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    FirstClass,
    LastClass,
    Break,
    ExamPeriod,
    FinalExam,
    TermStart,
    TermEnd,
}

/// One dated statement: "Classes begin Tuesday, September 8".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractedClaim {
    pub kind: ClaimKind,
    /// "YYYY-MM-DD" or "MM-DD" as the reader copied it; core decides the year.
    pub date: String,
    pub end_date: Option<String>,
    /// A short label in the material's language (material text, rule 8).
    pub label: String,
    /// The exact words the date comes from.
    pub quote: String,
    /// The material: a context handle (model) or a material id (MCP, scan).
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeekKind {
    Teaching,
    Break,
    Exam,
}

/// One row of a schedule table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractedWeek {
    pub week: i64,
    pub starts_on: Option<String>,
    pub kind: WeekKind,
    pub topic: Option<String>,
    pub quote: String,
    /// The table's header line when the row itself has only a bare number ("1  Basics").
    pub header_quote: Option<String>,
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotFound {
    FirstClass,
    LastClass,
    Breaks,
    ExamPeriod,
    FinalExam,
    Weeks,
}
