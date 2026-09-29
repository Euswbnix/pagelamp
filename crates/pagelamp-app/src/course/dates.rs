//! The course dates form, version 2 (docs/design/v0.3-course-calendar.md §7.10): first and
//! last day of classes, end of exams, breaks and a second segment for full-year courses.
//!
//! The input types come first (alpha.2 contract, agreed with frontend-2);
//! `set_course_dates` arrives with the schema-v4 calendars (B6). The facade computes the week
//! numbering (a second segment continues after the first or restarts at 1), so surfaces do no
//! week arithmetic.

use chrono::NaiveDate;
use pagelamp_core::model::BreakKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CourseDatesInput {
    pub first_class: Option<NaiveDate>,
    pub last_class: Option<NaiveDate>,
    pub exams_end: Option<NaiveDate>,
    pub breaks: Vec<BreakInput>,
    /// The second half of a full-year course.
    pub second_segment: Option<SegmentInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BreakInput {
    pub kind: BreakKind,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// The break counts in the week numbering.
    pub numbered: bool,
    /// Shown to the student only (never over MCP); at most 80 characters.
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SegmentInput {
    pub first_class: NaiveDate,
    pub last_class: Option<NaiveDate>,
    /// True: this segment's first week is week 1; false: numbering continues.
    pub restart_numbering: bool,
}
