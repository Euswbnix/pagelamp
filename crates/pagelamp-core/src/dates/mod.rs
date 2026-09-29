//! Instants to course calendar dates (docs/design/v0.3-course-calendar.md §6.1), and the
//! grammar of dates as course materials write them (`parse`, §7.2).
//!
//! Every place that turns a timestamp into a date for week or phase arithmetic (material
//! publish dates, module unlocks, events, the LMS course and term dates stored at sync) goes
//! through `course_date`, so the same instant never lands on two different days in two
//! signals. The time zone is the course's own (`lms_time_zone`, an IANA name); without one the
//! UTC date is used, as v0.1 did.
//!
//! `today` is not computed here: it stays the machine's local date (`views::AsOf`), so the
//! app and `pagelamp mcp` on the same machine agree.

pub mod parse;

use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeDelta};

pub use chrono_tz::Tz;

use crate::model::Timestamp;

/// The IANA time zone called `name` ("America/Toronto"), or None for an unknown name.
pub fn time_zone(name: &str) -> Option<Tz> {
    name.trim().parse().ok()
}

/// The course-local wall-clock time of `instant` (UTC without a time zone).
pub fn course_datetime(instant: Timestamp, tz: Option<Tz>) -> NaiveDateTime {
    match tz {
        Some(tz) => instant.with_timezone(&tz).naive_local(),
        None => instant.naive_utc(),
    }
}

/// The calendar date of `instant` in the course's time zone (UTC without one).
pub fn course_date(instant: Timestamp, tz: Option<Tz>) -> NaiveDate {
    course_datetime(instant, tz).date()
}

/// The Monday of `date`'s week (weeks run Monday to Sunday).
pub fn monday_of(date: NaiveDate) -> NaiveDate {
    date - TimeDelta::days(i64::from(date.weekday().num_days_from_monday()))
}

/// The Monday of teaching week 1 for classes starting on `first_class`: that week's Monday,
/// or the next Monday for a weekend start (a term that "starts" on a Saturday teaches from
/// Monday, as a weekend post counts for the next week).
pub fn week_one_monday(first_class: NaiveDate) -> NaiveDate {
    match first_class.weekday() {
        chrono::Weekday::Sat => add_days(first_class, 2),
        chrono::Weekday::Sun => add_days(first_class, 1),
        _ => monday_of(first_class),
    }
}

/// `date` plus `days` (negative goes back), saturating at the ends of `NaiveDate`.
pub fn add_days(date: NaiveDate, days: i64) -> NaiveDate {
    let delta = TimeDelta::days(days);
    date.checked_add_signed(delta).unwrap_or(if days < 0 {
        NaiveDate::MIN
    } else {
        NaiveDate::MAX
    })
}

/// Whole days from `from` to `to` (negative when `to` is earlier).
pub fn days_between(from: NaiveDate, to: NaiveDate) -> i64 {
    (to - from).num_days()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    fn at(text: &str) -> Timestamp {
        chrono::DateTime::parse_from_rfc3339(text)
            .expect("valid instant")
            .to_utc()
    }

    #[test]
    fn course_date_uses_the_course_time_zone() {
        let toronto = time_zone("America/Toronto");
        assert!(toronto.is_some());
        // Sunday 2026-09-20 21:00 in Toronto is Monday 01:00 UTC.
        let instant = at("2026-09-21T01:00:00Z");
        assert_eq!(course_date(instant, toronto), date(2026, 9, 20));
        assert_eq!(course_date(instant, None), date(2026, 9, 21));
        // Around the end of DST (2026-11-01 02:00 EDT → 01:00 EST): 23:30 local both days.
        assert_eq!(
            course_date(at("2026-11-01T03:30:00Z"), toronto),
            date(2026, 10, 31)
        );
        assert_eq!(
            course_date(at("2026-11-02T04:30:00Z"), toronto),
            date(2026, 11, 1)
        );
    }

    #[test]
    fn unknown_time_zone_names_are_none() {
        assert!(time_zone("Mars/Olympus_Mons").is_none());
        assert!(time_zone("").is_none());
        assert_eq!(time_zone(" America/Toronto "), time_zone("America/Toronto"));
    }

    #[test]
    fn monday_of_every_weekday() {
        for day in 7..=13 {
            assert_eq!(monday_of(date(2026, 9, day)), date(2026, 9, 7), "day {day}");
        }
        assert_eq!(monday_of(date(2026, 9, 14)), date(2026, 9, 14));
    }

    #[test]
    fn weekend_starts_teach_from_the_next_monday() {
        assert_eq!(week_one_monday(date(2026, 9, 8)), date(2026, 9, 7));
        assert_eq!(week_one_monday(date(2026, 9, 11)), date(2026, 9, 7));
        assert_eq!(week_one_monday(date(2026, 9, 5)), date(2026, 9, 7));
        assert_eq!(week_one_monday(date(2026, 9, 6)), date(2026, 9, 7));
    }

    #[test]
    fn add_days_saturates() {
        assert_eq!(add_days(date(2026, 9, 7), 7), date(2026, 9, 14));
        assert_eq!(add_days(date(2026, 9, 7), -7), date(2026, 8, 31));
        assert_eq!(add_days(NaiveDate::MAX, 1), NaiveDate::MAX);
        assert_eq!(add_days(NaiveDate::MIN, -1), NaiveDate::MIN);
        assert_eq!(days_between(date(2026, 9, 7), date(2026, 9, 28)), 21);
    }
}
