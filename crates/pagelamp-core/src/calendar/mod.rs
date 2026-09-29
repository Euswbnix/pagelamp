//! Course calendars: the dates a student accepted (or typed) for a course, as one validated
//! structure (docs/design/v0.3-course-calendar.md §3.3, §7). Pure functions only.
//!
//! alpha.2 (B6) brings the rest: the dates form, the validator and the resolver using an
//! accepted calendar as its first anchor. This module starts with the types and
//! `legacy_calendar`, which the schema-v4 migration calls for PageLamp 0.1 term overrides.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dates::days_between;
use crate::term::{CalendarBreak, DateSpan, MAX_YEAR_TERM_DAYS, MIN_TERM_DAYS, TeachingSegment};

/// An accepted or proposed course calendar (built by core, stored as `calendar_json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CourseCalendar {
    /// One for a one-term course, two for a full-year course.
    pub segments: Vec<TeachingSegment>,
    pub breaks: Vec<CalendarBreak>,
    pub exam_period: Option<DateSpan>,
    /// A fixed label only, never requirements (rule 4).
    pub final_exam_on: Option<NaiveDate>,
    /// Optional per-week rows from a schedule table.
    pub weeks: Vec<CalendarWeek>,
}

/// One row of a schedule table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarWeek {
    pub number: u32,
    pub starts_on: NaiveDate,
    /// Plain text from the course material (rule 8: material text), at most 120 characters.
    pub topic: Option<String>,
}

/// The calendar for a term override set in PageLamp 0.1 (`user_term_start`, `user_term_end`)
/// that survived the v3 clean-up (design §3.2): the start becomes the first class, and the
/// end a last day of classes ("end only", §6.6).
///
/// The start is always kept (the student typed it). The end is dropped when start → end
/// isn't a plausible teaching span: it is before the start, shorter than `MIN_TERM_DAYS`, or
/// longer than `MAX_YEAR_TERM_DAYS` (the student's own dates are allowed full-year length;
/// the 0.1 prefill of a whole enrollment window is longer and was cleared by v3 anyway).
pub fn legacy_calendar(start: NaiveDate, end: Option<NaiveDate>) -> CourseCalendar {
    let end = end.filter(|end| {
        let days = days_between(start, *end) + 1;
        (MIN_TERM_DAYS..=MAX_YEAR_TERM_DAYS).contains(&days)
    });
    CourseCalendar {
        segments: vec![TeachingSegment {
            first_class: start,
            last_class: end,
            first_week_number: 1,
        }],
        breaks: Vec::new(),
        exam_period: None,
        final_exam_on: None,
        weeks: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn legacy_calendar_keeps_the_start_and_a_plausible_end() {
        let calendar = legacy_calendar(date(2026, 9, 8), Some(date(2026, 12, 8)));
        assert_eq!(
            calendar.segments,
            [TeachingSegment {
                first_class: date(2026, 9, 8),
                last_class: Some(date(2026, 12, 8)),
                first_week_number: 1,
            }]
        );
        assert!(calendar.breaks.is_empty() && calendar.weeks.is_empty());
        assert_eq!((calendar.exam_period, calendar.final_exam_on), (None, None));
        // A full-year span typed by the student is kept.
        let year = legacy_calendar(date(2026, 9, 8), Some(date(2027, 4, 9)));
        assert_eq!(year.segments[0].last_class, Some(date(2027, 4, 9)));
    }

    #[test]
    fn legacy_calendar_drops_an_implausible_end() {
        for end in [
            date(2026, 9, 1),  // before the start
            date(2026, 9, 20), // two weeks
            date(2027, 8, 31), // a year
        ] {
            let calendar = legacy_calendar(date(2026, 9, 8), Some(end));
            assert_eq!(calendar.segments[0].first_class, date(2026, 9, 8));
            assert_eq!(calendar.segments[0].last_class, None, "{end}");
        }
        assert_eq!(
            legacy_calendar(date(2026, 9, 8), None).segments[0].last_class,
            None
        );
    }

    #[test]
    fn calendar_json_round_trips() {
        let calendar = legacy_calendar(date(2026, 9, 8), Some(date(2026, 12, 8)));
        let json = serde_json::to_string(&calendar).unwrap();
        assert_eq!(
            json,
            r#"{"segments":[{"first_class":"2026-09-08","last_class":"2026-12-08","first_week_number":1}],"breaks":[],"exam_period":null,"final_exam_on":null,"weeks":[]}"#
        );
        let back: CourseCalendar = serde_json::from_str(&json).unwrap();
        assert_eq!(back, calendar);
    }
}
