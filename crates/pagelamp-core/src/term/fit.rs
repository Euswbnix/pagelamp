//! Week 1 from the professor's week-numbered materials (docs/design/v0.3-course-calendar.md
//! §6.5): how "the notes the professor publishes" enter without a model.
//!
//! 1. Observations: materials with a `week_hint` published on or before today, and modules
//!    with a `week_hint` unlocked on or before today.
//! 2. The posting day is the course-local date; a post on Friday from 18:00, on Saturday or on
//!    Sunday counts for the next Monday (a Sunday-evening post is next week's material).
//! 3. Bulk days are dropped: a day with more than `MAX_WEEKS_PER_DAY` distinct weeks.
//! 4. One observation per day: that day's largest week ("posted up to week N by this day").
//! 5. One observation per week: a week's observations are split into clusters by gaps of more
//!    than `CLUSTER_GAP_DAYS`, and the earliest observation of the latest cluster is kept, so
//!    a late solutions file or an edited handout doesn't move the week, while a January
//!    renumbering (a new cluster) does.
//! 6. Each kept observation proposes `monday_of(day) − 7 × (week − 1)` as week 1's Monday.
//! 7. Only the latest `FIT_WEEKS` observations count; the result is their median. Candidates
//!    more than `MAX_SPREAD_DAYS` from it, or outside the outer frame, are dropped.
//! 8. Confidence: 3 weeks with a median absolute deviation ≤ 7 days → Medium; 2 weeks, or a
//!    deviation ≤ 14 days → Low; otherwise no fit.
//!
//! Known bias: a professor who posts next week's slides early makes the fit a week early.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate, Timelike, Weekday};

use super::DateSpan;
use crate::dates::{Tz, add_days, course_datetime, days_between, monday_of};
use crate::model::{Confidence, Material, Module};

/// More distinct weeks than this on one posting day means a bulk upload.
pub(crate) const MAX_WEEKS_PER_DAY: usize = 4;
/// A week's observations further apart than this start a new cluster.
const CLUSTER_GAP_DAYS: i64 = 56;
/// How many of the latest weeks the fit uses.
const FIT_WEEKS: usize = 3;
/// Candidates further than this from the median are dropped.
const MAX_SPREAD_DAYS: i64 = 10;
/// From 18:00 on a Friday a post counts for the next Monday.
const FRIDAY_EVENING_HOUR: u32 = 18;

/// "Posted up to week `week` by `day`": one per posting day, after dropping bulk days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Observation {
    pub day: NaiveDate,
    pub week: u32,
}

/// The fitted week 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fit {
    pub monday: NaiveDate,
    /// How many weeks' candidates agree (2 or 3).
    pub weeks: usize,
    pub confidence: Confidence,
}

/// Steps 1–4: one observation per posting day, bulk days dropped, sorted by day.
pub(crate) fn observations(
    modules: &[Module],
    materials: &[Material],
    tz: Option<Tz>,
    today: NaiveDate,
) -> Vec<Observation> {
    let dated = materials
        .iter()
        .filter_map(|m| Some((m.published_at?, m.week_hint?)))
        .chain(
            modules
                .iter()
                .filter_map(|m| Some((m.unlock_at?, m.week_hint?))),
        );
    let mut by_day: BTreeMap<NaiveDate, BTreeSet<u32>> = BTreeMap::new();
    for (instant, week) in dated {
        let local = course_datetime(instant, tz);
        if local.date() > today {
            continue;
        }
        by_day.entry(posting_day(local)).or_default().insert(week);
    }
    by_day
        .into_iter()
        .filter(|(_, weeks)| weeks.len() <= MAX_WEEKS_PER_DAY)
        .filter_map(|(day, weeks)| {
            Some(Observation {
                day,
                week: *weeks.last()?,
            })
        })
        .collect()
}

/// Step 2: Friday evening and the weekend count for the next Monday.
fn posting_day(local: chrono::NaiveDateTime) -> NaiveDate {
    let date = local.date();
    match date.weekday() {
        Weekday::Fri if local.hour() >= FRIDAY_EVENING_HOUR => add_days(date, 3),
        Weekday::Sat => add_days(date, 2),
        Weekday::Sun => add_days(date, 1),
        _ => date,
    }
}

/// Steps 5–8. `frame` bounds plausible week-1 Mondays (the outer frame, or today ± 240 days).
pub(crate) fn fit(observations: &[Observation], frame: DateSpan) -> Option<Fit> {
    let kept = one_per_week(observations);
    // Latest weeks first.
    let mut latest: Vec<(NaiveDate, NaiveDate)> =
        kept.iter().map(|o| (o.day, week_one_monday(*o))).collect();
    latest.sort_by_key(|(day, _)| std::cmp::Reverse(*day));
    latest.truncate(FIT_WEEKS);
    let first = median(&latest)?;
    let close: Vec<(NaiveDate, NaiveDate)> = latest
        .into_iter()
        .filter(|(_, monday)| days_between(first, *monday).abs() <= MAX_SPREAD_DAYS)
        .filter(|(_, monday)| frame.contains(*monday))
        .collect();
    let monday = median(&close)?;
    let mut deviations: Vec<i64> = close
        .iter()
        .map(|(_, m)| days_between(monday, *m).abs())
        .collect();
    deviations.sort_unstable();
    let deviation = deviations[deviations.len() / 2];
    let confidence = match close.len() {
        n if n >= FIT_WEEKS && deviation <= 7 => Confidence::Medium,
        n if n >= 2 && (n == 2 || deviation <= 14) => Confidence::Low,
        _ => return None,
    };
    Some(Fit {
        monday,
        weeks: close.len(),
        confidence,
    })
}

/// Step 5: per week, the earliest observation of its latest cluster.
fn one_per_week(observations: &[Observation]) -> Vec<Observation> {
    let mut by_week: BTreeMap<u32, Vec<NaiveDate>> = BTreeMap::new();
    for o in observations {
        by_week.entry(o.week).or_default().push(o.day);
    }
    by_week
        .into_iter()
        .filter_map(|(week, mut days)| {
            days.sort_unstable();
            // Start of the latest cluster: after the last gap longer than CLUSTER_GAP_DAYS.
            let start = days
                .windows(2)
                .rposition(|pair| days_between(pair[0], pair[1]) > CLUSTER_GAP_DAYS)
                .map_or(0, |i| i + 1);
            Some(Observation {
                day: *days.get(start)?,
                week,
            })
        })
        .collect()
}

/// Step 6.
fn week_one_monday(o: Observation) -> NaiveDate {
    add_days(monday_of(o.day), -7 * (i64::from(o.week) - 1))
}

/// The median candidate; with an even count, the one proposed by the more recent observation.
/// `(observation day, candidate)` pairs.
fn median(candidates: &[(NaiveDate, NaiveDate)]) -> Option<NaiveDate> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by_key(|(day, monday)| (*monday, *day));
    let n = sorted.len();
    match n {
        0 => None,
        n if n % 2 == 1 => Some(sorted[n / 2].1),
        n => {
            let (a, b) = (sorted[n / 2 - 1], sorted[n / 2]);
            Some(if a.0 > b.0 { a.1 } else { b.1 })
        }
    }
}

/// How far the professor's materials have got: the highest week among the observations of
/// the 28 days up to the latest one (so a January renumbering is followed).
pub(crate) fn notes_week(observations: &[Observation]) -> Option<u32> {
    let latest = observations.iter().map(|o| o.day).max()?;
    observations
        .iter()
        .filter(|o| days_between(o.day, latest) <= 28)
        .map(|o| o.week)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timestamp;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(text: &str) -> Timestamp {
        chrono::DateTime::parse_from_rfc3339(text).unwrap().to_utc()
    }

    fn obs(day: NaiveDate, week: u32) -> Observation {
        Observation { day, week }
    }

    fn wide_frame() -> DateSpan {
        DateSpan {
            start: date(2026, 1, 1),
            end: date(2027, 12, 31),
        }
    }

    #[test]
    fn weekend_and_friday_evening_posts_count_for_next_monday() {
        let local =
            |text: &str| chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap();
        assert_eq!(posting_day(local("2026-09-18 17:59")), date(2026, 9, 18));
        assert_eq!(posting_day(local("2026-09-18 18:00")), date(2026, 9, 21));
        assert_eq!(posting_day(local("2026-09-19 09:00")), date(2026, 9, 21));
        assert_eq!(posting_day(local("2026-09-20 21:00")), date(2026, 9, 21));
        assert_eq!(posting_day(local("2026-09-21 08:00")), date(2026, 9, 21));
        // The shifted day uses the course's time zone.
        let toronto = crate::dates::time_zone("America/Toronto");
        // Friday 21:00 in Toronto (Saturday 01:00 UTC): next Monday either way.
        assert_eq!(
            posting_day(course_datetime(at("2026-09-19T01:00:00Z"), toronto)),
            date(2026, 9, 21)
        );
        // Friday 16:00 in Toronto is 20:00 UTC: only the course time zone keeps it Friday.
        assert_eq!(
            posting_day(course_datetime(at("2026-09-18T20:00:00Z"), toronto)),
            date(2026, 9, 18)
        );
        assert_eq!(
            posting_day(course_datetime(at("2026-09-18T20:00:00Z"), None)),
            date(2026, 9, 21)
        );
    }

    #[test]
    fn three_weekly_posts_fit_week_one() {
        let observations = [
            obs(date(2026, 9, 8), 1),
            obs(date(2026, 9, 15), 2),
            obs(date(2026, 9, 22), 3),
        ];
        let fit = fit(&observations, wide_frame()).unwrap();
        assert_eq!(fit.monday, date(2026, 9, 7));
        assert_eq!((fit.weeks, fit.confidence), (3, Confidence::Medium));
    }

    #[test]
    fn two_weeks_give_low_and_one_gives_nothing() {
        let two = [obs(date(2026, 9, 15), 2), obs(date(2026, 9, 22), 3)];
        let fit = fit(&two, wide_frame()).unwrap();
        assert_eq!(
            (fit.monday, fit.confidence),
            (date(2026, 9, 7), Confidence::Low)
        );
        assert_eq!(super::fit(&two[..1], wide_frame()), None);
    }

    #[test]
    fn late_copies_keep_the_earliest_observation() {
        // The week 3 handout fixed in week 6 is a new observation of week 3; the earliest of
        // week 3's cluster still counts.
        let observations = [
            obs(date(2026, 9, 8), 1),
            obs(date(2026, 9, 22), 3),
            obs(date(2026, 9, 29), 4),
            obs(date(2026, 10, 6), 5),
            obs(date(2026, 10, 13), 3),
        ];
        let fit = fit(&observations, wide_frame()).unwrap();
        assert_eq!(fit.monday, date(2026, 9, 7));
        assert_eq!(fit.confidence, Confidence::Medium);
    }

    #[test]
    fn a_drifted_date_is_an_outlier() {
        // Week 2's only material is a page whose updated_at moved to week 5 (Wednesday 10-07):
        // it proposes 09-28, three weeks off the others, and is dropped.
        let observations = [
            obs(date(2026, 9, 8), 1),
            obs(date(2026, 9, 22), 3),
            obs(date(2026, 9, 29), 4),
            obs(date(2026, 10, 6), 5),
            obs(date(2026, 10, 7), 2),
        ];
        let fit = fit(&observations, wide_frame()).unwrap();
        assert_eq!(fit.monday, date(2026, 9, 7));
        assert_eq!(fit.weeks, 2);
    }

    #[test]
    fn renumbering_after_a_long_gap_starts_a_new_cluster() {
        // A Y course: weeks 1-3 in September, renumbered 1-3 from January 11.
        let observations = [
            obs(date(2026, 9, 8), 1),
            obs(date(2026, 9, 15), 2),
            obs(date(2026, 9, 22), 3),
            obs(date(2027, 1, 12), 1),
            obs(date(2027, 1, 19), 2),
            obs(date(2027, 1, 26), 3),
        ];
        let fit = fit(&observations, wide_frame()).unwrap();
        assert_eq!(fit.monday, date(2027, 1, 11));
        assert_eq!(notes_week(&observations), Some(3));
    }

    #[test]
    fn candidates_outside_the_frame_are_dropped() {
        let observations = [
            obs(date(2026, 9, 8), 1),
            obs(date(2026, 9, 15), 2),
            obs(date(2026, 9, 22), 3),
        ];
        let frame = DateSpan {
            start: date(2026, 9, 14),
            end: date(2026, 12, 31),
        };
        assert_eq!(fit(&observations, frame), None);
    }

    #[test]
    fn bulk_days_are_dropped_and_each_day_keeps_its_largest_week() {
        use crate::model::{MaterialKind, TextStatus};
        let material = |id: &str, week: u32, when: &str| Material {
            id: id.into(),
            course_id: "c".into(),
            module_id: None,
            kind: MaterialKind::File,
            title: format!("Week {week}"),
            url: None,
            local_path: None,
            mime: None,
            published_at: Some(at(when)),
            week_hint: Some(week),
            content_hash: None,
            text_status: TextStatus::Pending,
            text_error: None,
            text_error_kind: None,
            text_error_fingerprint: None,
            download_blocked: None,
            updated_at: at(when),
        };
        let mut materials: Vec<Material> = (1..=12)
            .map(|w| material(&format!("bulk{w}"), w, "2026-09-02T12:00:00Z"))
            .collect();
        materials.push(material("a", 1, "2026-09-08T12:00:00Z"));
        materials.push(material("b", 2, "2026-09-15T12:00:00Z"));
        materials.push(material("c", 1, "2026-09-15T13:00:00Z"));
        materials.push(material("future", 9, "2026-10-30T12:00:00Z"));
        let observations = observations(&[], &materials, None, date(2026, 9, 28));
        assert_eq!(
            observations,
            [obs(date(2026, 9, 8), 1), obs(date(2026, 9, 15), 2)]
        );
    }
}
