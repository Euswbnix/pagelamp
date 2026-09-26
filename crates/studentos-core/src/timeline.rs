//! Course-timeline inference: "which teaching week is this course in, and why".
//!
//! Pure functions only (no DB, no clock) so they are easy to unit-test; callers pass `today`.
//!
//! Signals, strongest first:
//! 1. Modules with `unlock_at <= today` → the most recently unlocked module(s) are current.
//!    If that module has a `week_hint`, that is the week (High confidence).
//! 2. Term start known → week = floor((today - term_start) / 7) + 1 (Medium; High when it
//!    agrees with signal 1). Reading week / breaks are not modelled in v0.1 — say so in evidence.
//! 3. Materials with `week_hint` and `published_at <= today` → max week_hint among materials
//!    published in the last 10 days (Medium).
//! 4. Otherwise the latest-published material's week_hint (Low), else `current_week = None`.
//!
//! Evidence strings must name the concrete module/material/date used.
//!
//! Details and deliberate simplifications (v0.1):
//! - **Dates.** An instant (`Timestamp`) becomes a calendar date via `.date_naive()` of the
//!   *UTC* instant, not the student's local time zone. A module unlocking at 21:00 in Toronto
//!   (01:00 UTC the next day) therefore counts as unlocked on the next day. That is good enough
//!   for "which week" and keeps these functions free of time-zone settings.
//! - **Signal 1** takes the latest unlock *date* on or before `today`; every module unlocked
//!   on that date is "most recently unlocked". Modules without `unlock_at` are ignored, so when
//!   no module has one (common on Canvas) signal 1 is simply absent. The group names a week
//!   only when its modules carry exactly one distinct `week_hint`. When they carry several
//!   (e.g. the instructor released every module on the first day of term) unlock dates say
//!   nothing about the current week: that is noted in the evidence and the week comes from the
//!   weaker signals.
//! - **Signal 2** needs `today >= term_start`. It keeps counting after `term_end`;
//!   `outside_term` and the evidence tell the reader. "High when it agrees with signal 1"
//!   needs no extra code: whenever signal 1 names a week it wins with High confidence anyway.
//! - **Signal 3** looks at the 10 calendar days ending today (today and the 9 days before).
//!   Signal 4 is consulted only when signal 3 finds nothing. Both ignore materials without a
//!   `week_hint` or `published_at`, and materials published after `today`.
//! - **Combining.** The strongest signal that names a week decides `current_week` and
//!   `confidence`. Every weaker signal is still listed in the evidence, marked as agreeing or
//!   disagreeing, so a student can see (and correct) a wrong inference.
//! - Callers pass the modules/materials of this one course; they are not filtered by course id.

use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;

use crate::model::{Confidence, Course, CourseTimeline, Event, Material, Module, Timestamp};

/// Largest week number `parse_week_hint` accepts. A term has ~12–15 teaching weeks and a
/// year-long course ~26; anything bigger is almost certainly a year or some other number.
const MAX_WEEK: u32 = 30;

/// Signal 3 window: materials published within this many calendar days, today included.
const RECENT_MATERIAL_DAYS: i64 = 10;

/// How many module names an evidence string lists before summarising as "and N more".
const MAX_NAMES_IN_EVIDENCE: usize = 3;

/// Added whenever the term-calendar signal (2) is part of the evidence.
const READING_WEEK_NOTE: &str = "calendar weeks count every 7 days since term start: reading \
     weeks and breaks are not modelled, so the calendar week can run ahead of the teaching week";

/// Added when no signal gives a week number.
const NO_SIGNAL_NOTE: &str = "current week unknown: no unlocked module with a week number, no \
     term start on or before today, and no published material with a week number";

/// Week-number patterns, tried in this order; the first match whose number is in
/// `1..=MAX_WEEK` wins. Names are matched after replacing `_` with a space, because the regex
/// word boundary `\b` counts `_` as a letter ("W3_slides" must still match).
static WEEK_PATTERNS: LazyLock<[Regex; 3]> = LazyLock::new(|| {
    [
        // Chinese: "第3周", "第 3 周", and the traditional "第3週". ASCII digits only
        // ("第三周" is not recognised). "第3章" (chapter) and "第3讲" (lecture) do not match.
        Regex::new(r"第\s*([0-9]+)\s*[周週]").expect("valid regex"),
        // The full word: "Week 3", "week03", "WEEK-3", "Week #3", "Weeks 3-4" (first number).
        // `\b` in front rejects "midweek 3". Anything may follow the number ("week3slides"),
        // because the word "week" alone makes the meaning clear.
        Regex::new(r"(?i)\bweeks?\s*[-.:#]?\s*([0-9]+)").expect("valid regex"),
        // Abbreviations: "W3", "w03", "Wk 3", "wk.3". Both ends must be word boundaries, so
        // "HW3" (homework), "2026W" and letter-glued codes such as "W3C" or "W3D1" are NOT
        // weeks (too easily a standard, room or product code). A bare "W" must touch the
        // number: "W 3" is not a week.
        Regex::new(r"(?i)\b(?:wk\s*[-.:]?\s*|w)([0-9]+)\b").expect("valid regex"),
    ]
});

/// Parse a teaching-week number from a module/folder/file name.
/// Accepts e.g. "Week 3", "week03", "W3", "Wk 3", "Week 3 - Backprop", "03 - Week 3",
/// "第3周". Returns None for "Lecture 3"/"Lab 3" (lecture numbers are not weeks) and for
/// numbers > 30.
///
/// More precisely (see `WEEK_PATTERNS` for the exact rules):
/// - Ranges give their first week: "Weeks 3-4" and "Week 3 & 4" → 3.
/// - "Week 0" → None: weeks are 1-based, and an orientation week is not teaching week 1.
/// - "HW3", "Quiz 3", "Tutorial 3", "2026W", "W3D", "Week three" → None.
/// - When a name contains several week-like tokens, the spelled-out forms ("第3周", then
///   "Week 3") win over abbreviations ("W3"); within one form the leftmost valid one wins.
pub fn parse_week_hint(name: &str) -> Option<u32> {
    let normalized = name.replace('_', " ");
    WEEK_PATTERNS
        .iter()
        .flat_map(|pattern| pattern.captures_iter(&normalized))
        .filter_map(|captures| captures.get(1)?.as_str().parse::<u32>().ok())
        .find(|week| (1..=MAX_WEEK).contains(week))
}

/// Teaching week of `date` relative to `term_start` (1-based). None if date < term_start.
///
/// The term-start day and the 6 days after it are week 1; `term_start + 7 days` is week 2.
pub fn week_of(term_start: NaiveDate, date: NaiveDate) -> Option<u32> {
    let days = (date - term_start).num_days();
    if days < 0 {
        return None;
    }
    u32::try_from(days / 7 + 1).ok()
}

/// Infer where `course` is on `today`, using the four signals described in the module docs.
///
/// `modules` and `materials` must belong to `course`. `_events` is accepted so callers do not
/// have to change when event-based signals arrive, but v0.1 ignores it: deadline dates say
/// little about which teaching week a course is in.
pub fn infer_timeline(
    course: &Course,
    modules: &[Module],
    materials: &[Material],
    _events: &[Event],
    today: NaiveDate,
) -> CourseTimeline {
    let mut evidence = Vec::new();

    // Collect the signals that name a week, strongest first.
    let unlock = latest_unlock(modules, today);
    let mut signals = Vec::new();
    if let Some(unlock) = &unlock {
        match unlock_signal(unlock) {
            Some(signal) => signals.push(signal),
            None => evidence.push(unlock_without_week_note(unlock)),
        }
    }
    let calendar = calendar_signal(course, today);
    let uses_calendar = calendar.is_some();
    signals.extend(calendar);
    signals.extend(
        recent_material_signal(materials, today)
            .or_else(|| latest_material_signal(materials, today)),
    );

    // The strongest signal decides; the weaker ones are listed as agreeing or disagreeing.
    let (current_week, confidence) = match signals.split_first() {
        Some((chosen, weaker)) => {
            evidence.push(chosen.evidence.clone());
            evidence.extend(weaker.iter().map(|signal| compare_note(signal, chosen)));
            (Some(chosen.week), chosen.confidence)
        }
        None => (None, Confidence::Low),
    };

    if uses_calendar {
        evidence.push(READING_WEEK_NOTE.to_string());
    }
    let outside_note = outside_term_note(course, today);
    let outside_term = outside_note.is_some();
    evidence.extend(outside_note);
    if current_week.is_none() {
        evidence.push(NO_SIGNAL_NOTE.to_string());
    }

    let current_module_ids = match &unlock {
        // Signal 1 found the most recently unlocked module(s) → those are current. A batch
        // release spanning several weeks says nothing about "current", so it is skipped.
        Some(unlock) if unlock.weeks().len() <= 1 => {
            unlock.modules.iter().map(|m| m.id.clone()).collect()
        }
        _ => modules
            .iter()
            .filter(|m| current_week.is_some() && m.week_hint == current_week)
            .map(|m| m.id.clone())
            .collect(),
    };

    CourseTimeline {
        as_of: today,
        current_week,
        confidence,
        evidence,
        current_module_ids,
        outside_term,
    }
}

/// A signal that names a week, with the sentence that justifies it.
struct WeekSignal {
    week: u32,
    confidence: Confidence,
    /// Short name used in agree/disagree notes, e.g. "module unlock dates".
    source: &'static str,
    /// Human-readable reason naming the concrete module/material and date.
    evidence: String,
}

/// Raw data for signal 1: the module(s) unlocked most recently on or before `today`.
struct LatestUnlock<'a> {
    date: NaiveDate,
    /// Every module whose unlock date is `date`, in input order.
    modules: Vec<&'a Module>,
}

impl LatestUnlock<'_> {
    /// The distinct week hints of the group, ascending (empty when no module has one).
    fn weeks(&self) -> Vec<u32> {
        let mut weeks: Vec<u32> = self.modules.iter().filter_map(|m| m.week_hint).collect();
        weeks.sort_unstable();
        weeks.dedup();
        weeks
    }
}

/// Calendar date of an instant, taken in UTC (see the module docs for why).
fn utc_date(instant: Timestamp) -> NaiveDate {
    instant.date_naive()
}

/// Signal 1 input: None when no module has an unlock date on or before `today`.
fn latest_unlock(modules: &[Module], today: NaiveDate) -> Option<LatestUnlock<'_>> {
    let unlock_date = |m: &Module| m.unlock_at.map(utc_date);
    let date = modules
        .iter()
        .filter_map(unlock_date)
        .filter(|date| *date <= today)
        .max()?;
    let modules = modules
        .iter()
        .filter(|m| unlock_date(m) == Some(date))
        .collect();
    Some(LatestUnlock { date, modules })
}

/// Signal 1: a week only when the most recently unlocked modules agree on exactly one.
fn unlock_signal(unlock: &LatestUnlock<'_>) -> Option<WeekSignal> {
    let [week] = unlock.weeks()[..] else {
        return None;
    };
    Some(WeekSignal {
        week,
        confidence: Confidence::High,
        source: "module unlock dates",
        evidence: format!(
            "{} unlocked {} (most recent unlock), so week {week}",
            describe_modules(&unlock.modules),
            unlock.date
        ),
    })
}

/// Evidence line for a most-recent-unlock group that does not name exactly one week.
fn unlock_without_week_note(unlock: &LatestUnlock<'_>) -> String {
    let modules = describe_modules(&unlock.modules);
    let weeks = unlock.weeks();
    match (weeks.first(), weeks.last()) {
        (Some(first), Some(last)) => format!(
            "{modules} all unlocked {} (weeks {first}-{last}), so unlock dates do not pin down \
             the current week",
            unlock.date
        ),
        _ => format!(
            "{modules} unlocked {} (most recent unlock) without a week number in the name",
            unlock.date
        ),
    }
}

/// "module 'A'", "modules 'A', 'B'" or "modules 'A', 'B', 'C' and 2 more".
fn describe_modules(modules: &[&Module]) -> String {
    let noun = if modules.len() == 1 {
        "module"
    } else {
        "modules"
    };
    let shown: Vec<String> = modules
        .iter()
        .take(MAX_NAMES_IN_EVIDENCE)
        .map(|m| format!("'{}'", m.name))
        .collect();
    let hidden = modules.len().saturating_sub(MAX_NAMES_IN_EVIDENCE);
    let more = if hidden > 0 {
        format!(" and {hidden} more")
    } else {
        String::new()
    };
    format!("{noun} {}{more}", shown.join(", "))
}

/// Signal 2: calendar week counted from the term start.
fn calendar_signal(course: &Course, today: NaiveDate) -> Option<WeekSignal> {
    let start = course.term_start?;
    let week = week_of(start, today)?;
    Some(WeekSignal {
        week,
        confidence: Confidence::Medium,
        source: "the term calendar",
        evidence: format!("term started {start}, so {today} is calendar week {week}"),
    })
}

/// A material usable by signals 3 and 4, with its publish date and week.
struct DatedMaterial<'a> {
    material: &'a Material,
    date: NaiveDate,
    week: u32,
}

/// Materials with a week hint that were published on or before `today`.
fn dated_week_materials(
    materials: &[Material],
    today: NaiveDate,
) -> impl Iterator<Item = DatedMaterial<'_>> {
    materials.iter().filter_map(move |material| {
        let week = material.week_hint?;
        let date = utc_date(material.published_at?);
        (date <= today).then_some(DatedMaterial {
            material,
            date,
            week,
        })
    })
}

/// Signal 3: highest week among materials published in the last `RECENT_MATERIAL_DAYS` days.
fn recent_material_signal(materials: &[Material], today: NaiveDate) -> Option<WeekSignal> {
    let best = dated_week_materials(materials, today)
        .filter(|m| (today - m.date).num_days() < RECENT_MATERIAL_DAYS)
        // Highest week; among equal weeks the most recently published one is named.
        .max_by_key(|m| (m.week, m.material.published_at))?;
    Some(WeekSignal {
        week: best.week,
        confidence: Confidence::Medium,
        source: "recent materials",
        evidence: format!(
            "material '{}' published {} has the highest week number ({}) among materials \
             published in the last {RECENT_MATERIAL_DAYS} days",
            best.material.title, best.date, best.week
        ),
    })
}

/// Signal 4: week of the most recently published week-numbered material.
fn latest_material_signal(materials: &[Material], today: NaiveDate) -> Option<WeekSignal> {
    let latest = dated_week_materials(materials, today)
        // Latest instant; on an exact tie the higher week is named.
        .max_by_key(|m| (m.material.published_at, m.week))?;
    Some(WeekSignal {
        week: latest.week,
        confidence: Confidence::Low,
        source: "the latest material",
        evidence: format!(
            "latest week-numbered material '{}' was published {} (week {}); nothing \
             week-numbered was published in the last {RECENT_MATERIAL_DAYS} days",
            latest.material.title, latest.date, latest.week
        ),
    })
}

/// Evidence line for a weaker signal, saying whether it agrees with the chosen one.
fn compare_note(weaker: &WeekSignal, chosen: &WeekSignal) -> String {
    if weaker.week == chosen.week {
        format!("{} (agrees)", weaker.evidence)
    } else {
        format!(
            "{} (disagrees with week {} from {}; the stronger signal wins)",
            weaker.evidence, chosen.week, chosen.source
        )
    }
}

/// Some(note) when `today` is before the known term start or after the known term end.
fn outside_term_note(course: &Course, today: NaiveDate) -> Option<String> {
    match (course.term_start, course.term_end) {
        (Some(start), _) if today < start => Some(format!(
            "outside term: today ({today}) is before the term starts ({start})"
        )),
        (_, Some(end)) if today > end => Some(format!(
            "outside term: today ({today}) is after the term ended ({end})"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AiPolicy, EventKind, MaterialKind, TextStatus};

    // ----- helpers (synthetic data only) ------------------------------------------------------

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    /// An instant at `hour`:00 UTC on `day`.
    fn at_hour(day: NaiveDate, hour: u32) -> Timestamp {
        day.and_hms_opt(hour, 0, 0).expect("valid time").and_utc()
    }

    fn noon(day: NaiveDate) -> Timestamp {
        at_hour(day, 12)
    }

    /// Synthetic term used by most tests: Monday 2026-09-07 .. 2026-12-18.
    /// Calendar weeks: 09-07 → 1, 09-14 → 2, 09-21 → 3, 09-28 → 4, 10-05 → 5.
    fn term_start() -> NaiveDate {
        date(2026, 9, 7)
    }

    fn term_end() -> NaiveDate {
        date(2026, 12, 18)
    }

    fn course(term_start: Option<NaiveDate>, term_end: Option<NaiveDate>) -> Course {
        Course {
            id: "demo:local/course/demo101".into(),
            source_id: "demo:local".into(),
            external_id: "demo101".into(),
            code: Some("DEMO101".into()),
            name: "Intro to Demo Studies".into(),
            term_start,
            term_end,
            url: None,
            ai_policy: AiPolicy::Unknown,
            ai_policy_note: None,
            hidden: false,
            updated_at: noon(date(2026, 9, 1)),
        }
    }

    fn course_without_term() -> Course {
        course(None, None)
    }

    fn course_with_term() -> Course {
        course(Some(term_start()), Some(term_end()))
    }

    /// A module whose week_hint is parsed from its name, as the sources do.
    fn module(id: &str, name: &str, unlock: Option<NaiveDate>) -> Module {
        Module {
            id: id.into(),
            course_id: "demo:local/course/demo101".into(),
            name: name.into(),
            position: None,
            unlock_at: unlock.map(noon),
            week_hint: parse_week_hint(name),
        }
    }

    /// A material whose week_hint is parsed from its title, as the sources do.
    fn material(id: &str, title: &str, published: Option<NaiveDate>) -> Material {
        Material {
            id: id.into(),
            course_id: "demo:local/course/demo101".into(),
            module_id: None,
            kind: MaterialKind::File,
            title: title.into(),
            url: None,
            local_path: None,
            mime: None,
            published_at: published.map(noon),
            week_hint: parse_week_hint(title),
            content_hash: None,
            text_status: TextStatus::Pending,
            text_error: None,
            updated_at: noon(date(2026, 9, 1)),
        }
    }

    /// Weekly modules "Week 1" .. "Week 5", unlocking on each Monday of the synthetic term.
    fn weekly_modules() -> Vec<Module> {
        vec![
            module("m1", "Week 1: Welcome", Some(date(2026, 9, 7))),
            module("m2", "Week 2: Basics", Some(date(2026, 9, 14))),
            module("m3", "Week 3: Gradients", Some(date(2026, 9, 21))),
            module("m4", "Week 4: Backprop", Some(date(2026, 9, 28))),
            module("m5", "Week 5: Review", Some(date(2026, 10, 5))),
        ]
    }

    fn infer(
        course: &Course,
        modules: &[Module],
        materials: &[Material],
        today: NaiveDate,
    ) -> CourseTimeline {
        infer_timeline(course, modules, materials, &[], today)
    }

    fn has_evidence(timeline: &CourseTimeline, needle: &str) -> bool {
        timeline.evidence.iter().any(|line| line.contains(needle))
    }

    #[track_caller]
    fn assert_evidence(timeline: &CourseTimeline, needle: &str) {
        assert!(
            has_evidence(timeline, needle),
            "no evidence line contains {needle:?}; evidence: {:#?}",
            timeline.evidence
        );
    }

    // ----- parse_week_hint ------------------------------------------------------------------

    #[test]
    fn parse_week_hint_accepts_week_names() {
        let cases = [
            ("Week 3", 3),
            ("week03", 3),
            ("WEEK 3", 3),
            ("W3", 3),
            ("w03", 3),
            ("Wk 3", 3),
            ("wk.3", 3),
            ("WK03", 3),
            ("Wk-3", 3),
            ("Week 3 - Backprop", 3),
            ("03 - Week 3", 3),
            ("Week-3", 3),
            ("Week_3", 3),
            ("Week.3", 3),
            ("Week #3", 3),
            ("Week: 3", 3),
            ("(Week 3)", 3),
            ("week3slides.pdf", 3),
            ("W3.pdf", 3),
            ("W3 slides", 3),
            ("DEMO101_W05_slides.pdf", 5),
            ("2026 Fall Week 2", 2),
            ("Lecture 7 - Week 4", 4),
            ("Week 12: Review", 12),
            ("Week 1", 1),
            ("Week 30", 30),
            ("第3周", 3),
            ("第 3 周", 3),
            ("第3週", 3),
            ("第12周 复习", 12),
        ];
        for (name, expected) in cases {
            assert_eq!(parse_week_hint(name), Some(expected), "name: {name:?}");
        }
    }

    #[test]
    fn parse_week_hint_takes_first_week_of_a_range() {
        let cases = [
            ("Weeks 3-4", 3),
            ("Week 3 & 4", 3),
            ("Week 3 and 4", 3),
            ("Weeks 3–4", 3),
            ("W3-4", 3),
            ("Weeks 10 - 11: Projects", 10),
        ];
        for (name, expected) in cases {
            assert_eq!(parse_week_hint(name), Some(expected), "name: {name:?}");
        }
    }

    #[test]
    fn parse_week_hint_prefers_spelled_out_week_over_abbreviation() {
        assert_eq!(parse_week_hint("W2 recap - Week 3"), Some(3));
        assert_eq!(parse_week_hint("第4周 Week 3"), Some(4));
    }

    #[test]
    fn parse_week_hint_rejects_non_week_names() {
        let cases = [
            "",
            "Syllabus",
            "Lecture 3",
            "Lab 3",
            "Tutorial 3",
            "Assignment 3",
            "Homework 3",
            "HW3",
            "hw3",
            "Quiz 3",
            "2026W",
            "2026W3",
            "Midweek 3",
            "Weekly Quiz 3",
            "Weekend 2",
            "Workshop 3",
            "Week",
            "Week three",
            // A bare "W" must touch the number.
            "W 3",
            // Letters glued to the number: codes, not weeks.
            "W3D",
            "W3D1",
            "W3C Standards",
            "Wk3D",
            // Chinese: numerals, chapters and lectures are not weeks.
            "第三周",
            "第3章",
            "第3讲",
        ];
        for name in cases {
            assert_eq!(parse_week_hint(name), None, "name: {name:?}");
        }
    }

    #[test]
    fn parse_week_hint_rejects_out_of_range_numbers() {
        let cases = [
            "Week 0",
            "W0",
            "第0周",
            "Week 31",
            "Week 99",
            "W45",
            "Week 2026",
            "Week 99999999999999999999",
        ];
        for name in cases {
            assert_eq!(parse_week_hint(name), None, "name: {name:?}");
        }
    }

    // ----- week_of --------------------------------------------------------------------------

    #[test]
    fn week_of_counts_seven_day_blocks_from_term_start() {
        let start = term_start();
        let cases = [
            (0, Some(1)),
            (1, Some(1)),
            (6, Some(1)),
            (7, Some(2)),
            (13, Some(2)),
            (14, Some(3)),
            (70, Some(11)),
        ];
        for (offset, expected) in cases {
            let day = start + chrono::Days::new(offset);
            assert_eq!(week_of(start, day), expected, "offset {offset} days");
        }
    }

    #[test]
    fn week_of_is_none_before_term_start() {
        let start = term_start();
        assert_eq!(week_of(start, date(2026, 9, 6)), None);
        assert_eq!(week_of(start, date(2025, 9, 7)), None);
    }

    // ----- infer_timeline: no data ----------------------------------------------------------

    #[test]
    fn no_data_gives_unknown_week_with_low_confidence() {
        let today = date(2026, 9, 30);
        let t = infer(&course_without_term(), &[], &[], today);
        assert_eq!(t.as_of, today);
        assert_eq!(t.current_week, None);
        assert_eq!(t.confidence, Confidence::Low);
        assert!(t.current_module_ids.is_empty());
        assert!(!t.outside_term);
        assert_eq!(t.evidence, vec![NO_SIGNAL_NOTE.to_string()]);
    }

    #[test]
    fn events_are_ignored() {
        let today = date(2026, 9, 30);
        let event = Event {
            id: "demo:local/event/1".into(),
            source_id: "demo:local".into(),
            course_id: Some("demo:local/course/demo101".into()),
            kind: EventKind::AssignmentDue,
            title: "Week 9 problem set".into(),
            starts_at: None,
            ends_at: None,
            due_at: Some(noon(date(2026, 9, 29))),
            url: None,
            updated_at: noon(date(2026, 9, 1)),
        };
        let t = infer_timeline(&course_without_term(), &[], &[], &[event], today);
        assert_eq!(t.current_week, None);
        assert_eq!(t.confidence, Confidence::Low);
    }

    // ----- signal 1: module unlocks ---------------------------------------------------------

    #[test]
    fn signal1_alone_uses_most_recently_unlocked_module() {
        // 2026-09-30: "Week 4" unlocked 09-28, "Week 5" unlocks 10-05 (future, ignored).
        let t = infer(
            &course_without_term(),
            &weekly_modules(),
            &[],
            date(2026, 9, 30),
        );
        assert_eq!(t.current_week, Some(4));
        assert_eq!(t.confidence, Confidence::High);
        assert_eq!(t.current_module_ids, vec!["m4".to_string()]);
        assert!(!t.outside_term);
        assert!(
            t.evidence[0].starts_with("module 'Week 4: Backprop' unlocked 2026-09-28"),
            "{:#?}",
            t.evidence
        );
        // No term start → no calendar signal → no reading-week note.
        assert!(!has_evidence(&t, "reading weeks"));
    }

    #[test]
    fn signal1_ignores_future_unlock_dates() {
        let modules = vec![
            module("m1", "Week 1: Welcome", Some(date(2026, 9, 7))),
            module("m9", "Week 9: Later", Some(date(2026, 11, 2))),
        ];
        let t = infer(&course_without_term(), &modules, &[], date(2026, 9, 10));
        assert_eq!(t.current_week, Some(1));
        assert_eq!(t.current_module_ids, vec!["m1".to_string()]);
    }

    #[test]
    fn signal1_is_absent_when_every_unlock_is_in_the_future() {
        let modules = vec![module("m1", "Week 1: Welcome", Some(date(2026, 9, 7)))];
        let t = infer(&course_without_term(), &modules, &[], date(2026, 9, 1));
        assert_eq!(t.current_week, None);
        assert_eq!(t.confidence, Confidence::Low);
        assert!(t.current_module_ids.is_empty());
    }

    #[test]
    fn signal1_counts_a_module_unlocked_today() {
        let t = infer(
            &course_without_term(),
            &weekly_modules(),
            &[],
            date(2026, 10, 5),
        );
        assert_eq!(t.current_week, Some(5));
        assert_eq!(t.current_module_ids, vec!["m5".to_string()]);
    }

    #[test]
    fn signal1_uses_the_utc_date_of_the_unlock_instant() {
        // 01:00 UTC on 09-29 is still the evening of 09-28 in the Americas, but we use UTC.
        let mut early = module("m4", "Week 4: Backprop", None);
        early.unlock_at = Some(at_hour(date(2026, 9, 29), 1));
        let modules = vec![
            module("m3", "Week 3: Gradients", Some(date(2026, 9, 21))),
            early,
        ];
        let t = infer(&course_without_term(), &modules, &[], date(2026, 9, 28));
        assert_eq!(t.current_week, Some(3));
        let t = infer(&course_without_term(), &modules, &[], date(2026, 9, 29));
        assert_eq!(t.current_week, Some(4));
    }

    #[test]
    fn signal1_groups_modules_unlocked_on_the_same_day() {
        let mut lecture = module("m4a", "Week 4: Lecture", None);
        lecture.unlock_at = Some(at_hour(date(2026, 9, 28), 8));
        let mut tutorial = module("m4b", "Week 4: Tutorial", None);
        tutorial.unlock_at = Some(at_hour(date(2026, 9, 28), 17));
        let modules = vec![
            module("m3", "Week 3: Gradients", Some(date(2026, 9, 21))),
            lecture,
            tutorial,
        ];
        let t = infer(&course_without_term(), &modules, &[], date(2026, 9, 30));
        assert_eq!(t.current_week, Some(4));
        assert_eq!(t.confidence, Confidence::High);
        assert_eq!(
            t.current_module_ids,
            vec!["m4a".to_string(), "m4b".to_string()]
        );
        assert_evidence(
            &t,
            "modules 'Week 4: Lecture', 'Week 4: Tutorial' unlocked 2026-09-28",
        );
    }

    #[test]
    fn signal1_batch_release_of_many_weeks_does_not_pick_a_week() {
        // Every module released on the first day of term: unlock dates say nothing.
        let modules: Vec<Module> = (1..=12)
            .map(|w| {
                module(
                    &format!("m{w}"),
                    &format!("Week {w}"),
                    Some(date(2026, 9, 7)),
                )
            })
            .collect();
        let t = infer(&course_with_term(), &modules, &[], date(2026, 10, 1));
        assert_eq!(t.current_week, Some(4), "falls back to the calendar");
        assert_eq!(t.confidence, Confidence::Medium);
        assert_eq!(t.current_module_ids, vec!["m4".to_string()]);
        assert_evidence(
            &t,
            "modules 'Week 1', 'Week 2', 'Week 3' and 9 more all unlocked 2026-09-07 (weeks 1-12)",
        );
    }

    #[test]
    fn signal1_module_without_week_number_is_current_but_week_comes_from_calendar() {
        let modules = vec![
            module("m1", "Week 1: Welcome", Some(date(2026, 9, 7))),
            module("mid", "Midterm Review", Some(date(2026, 9, 29))),
            module("m4", "Week 4: Backprop", None),
        ];
        let t = infer(&course_with_term(), &modules, &[], date(2026, 10, 1));
        assert_eq!(t.current_week, Some(4));
        assert_eq!(t.confidence, Confidence::Medium);
        assert_eq!(t.current_module_ids, vec!["mid".to_string()]);
        assert_evidence(&t, "module 'Midterm Review' unlocked 2026-09-29");
        assert_evidence(&t, "without a week number");
    }

    #[test]
    fn modules_without_unlock_dates_leave_signal1_absent() {
        let modules: Vec<Module> = weekly_modules()
            .into_iter()
            .map(|mut m| {
                m.unlock_at = None;
                m
            })
            .collect();
        let t = infer(&course_with_term(), &modules, &[], date(2026, 10, 1));
        assert_eq!(t.current_week, Some(4));
        assert_eq!(t.confidence, Confidence::Medium);
        assert_eq!(t.current_module_ids, vec!["m4".to_string()]);
        assert!(!has_evidence(&t, "unlocked"), "{:#?}", t.evidence);
    }

    // ----- signal 2: term calendar ----------------------------------------------------------

    #[test]
    fn signal2_alone_uses_calendar_week_with_reading_week_note() {
        let t = infer(&course_with_term(), &[], &[], date(2026, 10, 1));
        assert_eq!(t.current_week, Some(4));
        assert_eq!(t.confidence, Confidence::Medium);
        assert!(t.current_module_ids.is_empty());
        assert!(!t.outside_term);
        assert_eq!(
            t.evidence,
            vec![
                "term started 2026-09-07, so 2026-10-01 is calendar week 4".to_string(),
                READING_WEEK_NOTE.to_string(),
            ]
        );
    }

    #[test]
    fn signal1_and_signal2_agree_gives_high_confidence() {
        let t = infer(
            &course_with_term(),
            &weekly_modules(),
            &[],
            date(2026, 9, 30),
        );
        assert_eq!(t.current_week, Some(4));
        assert_eq!(t.confidence, Confidence::High);
        assert_eq!(t.current_module_ids, vec!["m4".to_string()]);
        assert_evidence(&t, "module 'Week 4: Backprop' unlocked 2026-09-28");
        assert_evidence(&t, "calendar week 4 (agrees)");
        assert_evidence(&t, "reading weeks and breaks are not modelled");
    }

    #[test]
    fn signal1_wins_over_disagreeing_calendar() {
        // Week 4's module is delayed to 10-05, so on 10-01 (calendar week 4) the latest
        // unlocked module is Week 3.
        let modules = vec![
            module("m1", "Week 1: Welcome", Some(date(2026, 9, 7))),
            module("m2", "Week 2: Basics", Some(date(2026, 9, 14))),
            module("m3", "Week 3: Gradients", Some(date(2026, 9, 21))),
            module("m4", "Week 4: Backprop", Some(date(2026, 10, 5))),
        ];
        let t = infer(&course_with_term(), &modules, &[], date(2026, 10, 1));
        assert_eq!(t.current_week, Some(3));
        assert_eq!(t.confidence, Confidence::High);
        assert_eq!(t.current_module_ids, vec!["m3".to_string()]);
        assert_evidence(
            &t,
            "calendar week 4 (disagrees with week 3 from module unlock dates",
        );
        assert_evidence(&t, "reading weeks and breaks are not modelled");
    }

    // ----- signals 3 and 4: materials -------------------------------------------------------

    #[test]
    fn signal3_alone_uses_max_week_of_recent_materials() {
        let materials = vec![
            material("a", "Week 2 slides.pdf", Some(date(2026, 9, 20))), // 10 days old: too old
            material("b", "Week 3 slides.pdf", Some(date(2026, 9, 26))),
            material("c", "Week 1 recap.pdf", Some(date(2026, 9, 29))),
            material("d", "Course policies.pdf", Some(date(2026, 9, 29))), // no week number
            material("e", "Week 4 notes.pdf", Some(date(2026, 10, 5))),    // future: ignored
            material("f", "Week 5 notes.pdf", None),                       // undated: ignored
        ];
        let t = infer(&course_without_term(), &[], &materials, date(2026, 9, 30));
        assert_eq!(t.current_week, Some(3));
        assert_eq!(t.confidence, Confidence::Medium);
        assert_evidence(&t, "material 'Week 3 slides.pdf' published 2026-09-26");
        assert_evidence(&t, "last 10 days");
    }

    #[test]
    fn signal3_window_is_today_and_the_nine_days_before() {
        let today = date(2026, 9, 30);
        let nine_days_ago = vec![material("a", "Week 3 slides.pdf", Some(date(2026, 9, 21)))];
        let t = infer(&course_without_term(), &[], &nine_days_ago, today);
        assert_eq!(
            (t.current_week, t.confidence),
            (Some(3), Confidence::Medium)
        );

        let ten_days_ago = vec![material("a", "Week 3 slides.pdf", Some(date(2026, 9, 20)))];
        let t = infer(&course_without_term(), &[], &ten_days_ago, today);
        assert_eq!(
            (t.current_week, t.confidence),
            (Some(3), Confidence::Low),
            "falls to signal 4"
        );
    }

    #[test]
    fn signal4_alone_uses_latest_week_numbered_material() {
        let materials = vec![
            material("a", "Week 1 intro.pdf", Some(date(2026, 9, 8))),
            material("b", "Week 2 notes.pdf", Some(date(2026, 9, 12))),
            material("c", "Course policies.pdf", Some(date(2026, 9, 15))), // no week number
        ];
        let t = infer(&course_without_term(), &[], &materials, date(2026, 10, 1));
        assert_eq!(t.current_week, Some(2));
        assert_eq!(t.confidence, Confidence::Low);
        assert!(t.current_module_ids.is_empty());
        assert_evidence(
            &t,
            "latest week-numbered material 'Week 2 notes.pdf' was published 2026-09-12",
        );
    }

    #[test]
    fn signal4_ignores_materials_published_after_today() {
        let materials = vec![
            material("a", "Week 1 intro.pdf", Some(date(2026, 9, 8))),
            material("b", "Week 6 notes.pdf", Some(date(2026, 10, 12))),
        ];
        let t = infer(&course_without_term(), &[], &materials, date(2026, 9, 25));
        assert_eq!(t.current_week, Some(1));
        assert_eq!(t.confidence, Confidence::Low);
    }

    #[test]
    fn calendar_wins_over_disagreeing_materials() {
        let materials = vec![material("a", "Week 2 recap.pdf", Some(date(2026, 9, 29)))];
        let t = infer(&course_with_term(), &[], &materials, date(2026, 10, 1));
        assert_eq!(t.current_week, Some(4));
        assert_eq!(t.confidence, Confidence::Medium);
        assert_evidence(
            &t,
            "material 'Week 2 recap.pdf' published 2026-09-29 has the highest week number (2)",
        );
        assert_evidence(
            &t,
            "(disagrees with week 4 from the term calendar; the stronger signal wins)",
        );
    }

    #[test]
    fn current_modules_follow_material_week_when_modules_have_no_unlock_dates() {
        let modules = vec![
            module("m2", "Week 2: Basics", None),
            module("m3", "Week 3: Gradients", None),
        ];
        let materials = vec![material("a", "W3 slides.pdf", Some(date(2026, 9, 28)))];
        let t = infer(
            &course_without_term(),
            &modules,
            &materials,
            date(2026, 9, 30),
        );
        assert_eq!(t.current_week, Some(3));
        assert_eq!(t.current_module_ids, vec!["m3".to_string()]);
    }

    #[test]
    fn all_signals_agree() {
        let materials = vec![material("a", "Week 4 slides.pdf", Some(date(2026, 9, 28)))];
        let t = infer(
            &course_with_term(),
            &weekly_modules(),
            &materials,
            date(2026, 9, 30),
        );
        assert_eq!(t.current_week, Some(4));
        assert_eq!(t.confidence, Confidence::High);
        assert_evidence(&t, "calendar week 4 (agrees)");
        assert_evidence(&t, "materials published in the last 10 days (agrees)");
    }

    // ----- outside term ---------------------------------------------------------------------

    #[test]
    fn before_term_start_is_outside_term_without_calendar_week() {
        let t = infer(&course_with_term(), &[], &[], date(2026, 9, 1));
        assert!(t.outside_term);
        assert_eq!(t.current_week, None);
        assert_eq!(t.confidence, Confidence::Low);
        assert_evidence(
            &t,
            "outside term: today (2026-09-01) is before the term starts (2026-09-07)",
        );
        assert!(!has_evidence(&t, "reading weeks"));
    }

    #[test]
    fn before_term_start_an_early_unlocked_module_still_counts() {
        let modules = vec![module("m1", "Week 1: Welcome", Some(date(2026, 8, 31)))];
        let t = infer(&course_with_term(), &modules, &[], date(2026, 9, 1));
        assert!(t.outside_term);
        assert_eq!(t.current_week, Some(1));
        assert_eq!(t.confidence, Confidence::High);
        assert_evidence(&t, "module 'Week 1: Welcome' unlocked 2026-08-31");
        assert_evidence(&t, "before the term starts");
    }

    #[test]
    fn after_term_end_is_outside_term_and_calendar_keeps_counting() {
        let t = infer(&course_with_term(), &[], &[], date(2027, 1, 10));
        assert!(t.outside_term);
        assert_eq!(t.current_week, Some(18));
        assert_evidence(
            &t,
            "outside term: today (2027-01-10) is after the term ended (2026-12-18)",
        );
    }

    #[test]
    fn term_end_day_is_inside_term_and_unknown_end_is_never_outside() {
        let t = infer(&course_with_term(), &[], &[], term_end());
        assert!(!t.outside_term);
        let open_ended = course(Some(term_start()), None);
        let t = infer(&open_ended, &[], &[], date(2027, 6, 1));
        assert!(!t.outside_term);
    }
}
