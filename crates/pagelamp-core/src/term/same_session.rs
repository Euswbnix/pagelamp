//! Courses of one session teach on the same days (docs/design/v0.3-course-calendar.md §6.4,
//! the anchor between 5 and 6, and §14). A course whose own dates are confirmed or well
//! established lends its week 1, and the ends of classes and exams its own source states, to the
//! other courses with the same `session::SessionKey`, where they have nothing better.
//!
//! Who lends (a donor), from its OWN evidence only: a lent date never lends on, so there are no
//! chains and no cycles (each course's donor is worked out with nothing lent to it). Whether a
//! course lends is decided over its term, never by today's phase or today's strongest signal:
//! a course keeps lending in a break, after its last class and when nothing new was posted.
//! - `StudentDates`: the student's dates (the dates form, the CLI, an accepted calendar), not
//!   the 0.1 dates the student never confirmed. Its ends: the student's last day of classes, or
//!   the calendar's last class and end of exams.
//! - `CourseDates`: its own LMS course dates, LMS term or folder dates, not disputed by a lower
//!   source, with which its own week-numbered materials and module unlocks agree over the term
//!   (`established`). Only LMS course dates state an end it lends.
//! - `ModuleUnlocks`: a course without dates of its own whose module unlocks pin week 1: two or
//!   more unlock days inside its session's window, each naming one week, all give the same
//!   Monday (the fit's day rule: a Friday evening or a weekend counts for the next Monday), and
//!   that Monday passes the window check (§6.3). Modules copied from an earlier year keep their
//!   old unlock dates: those lie outside the window and count for nothing. Week 1 only, lent at
//!   Low.
//!
//! The strongest kind present decides week 1. Its courses must agree on it, else nothing is
//! lent (`Lend::Split`) and every course of the session says so: a course without dates of its
//! own that nothing is lent, a course with its own (the disagreeing lenders among them) which
//! lenders put week 1 elsewhere. Its courses whose weeks can be lent lend; when none can (a
//! calendar of its own that two dates can't carry), the next kind's that agree with that week
//! 1 do, and a course without dates of its own that gets nothing says so (`Lend::Unlent`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use super::fit::posting_day;
use super::phase::teaching_week_on;
use super::{CalendarOrigin, DateSpan, MAX_DAYS_BEFORE_WINDOW, TermAnchorSource};
use crate::calendar::CalendarInForce;
use crate::dates::{Tz, add_days, course_datetime, days_between, week_one_monday};
use crate::model::{Confidence, Course, CourseTermData, Module};
use crate::term::ResolvedTerm;

/// Why a course may lend its dates, strongest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DonorKind {
    StudentDates,
    CourseDates,
    ModuleUnlocks,
}

/// One course's offer to the others of its session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Donor {
    /// The course's code, or its name without one.
    pub course: String,
    /// A hidden course lends too, but a reader that never shows hidden courses (the MCP server)
    /// doesn't learn its name.
    pub hidden: bool,
    pub kind: DonorKind,
    /// The first day of classes (for `ModuleUnlocks`, the Monday of week 1).
    pub start: NaiveDate,
    /// The last day of classes, when the donor's own source states it.
    pub end: Option<NaiveDate>,
    /// The end of exams, when the donor's accepted calendar states it.
    pub exams_end: Option<NaiveDate>,
    /// Its weeks can be lent (`CourseCalendar::lendable`): a calendar that can't still takes
    /// part in the session's agreement on week 1, but lends nothing.
    pub lends: bool,
    /// The number of the week `start` falls in (a calendar's first week; else 1).
    pub first_week: u32,
}

impl Donor {
    fn monday(&self) -> NaiveDate {
        week_one_of(self.start, self.first_week)
    }
}

/// The Monday of week 1 for classes starting on `start` in week `first_week`.
pub fn week_one_of(start: NaiveDate, first_week: u32) -> NaiveDate {
    add_days(week_one_monday(start), 7 * (1 - i64::from(first_week)))
}

/// The dates a course borrows from the others of its session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LentDates {
    /// The lending course's code or name; `None` when the reader doesn't show hidden courses
    /// and the lender is hidden ("another course of the same session").
    pub course: Option<String>,
    /// The session code ("20269").
    pub session: String,
    pub kind: DonorKind,
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
    pub exams_end: Option<NaiveDate>,
    /// The number of the week `start` falls in at the lender (1 for dates it lends; a course
    /// named in a split may count from another).
    pub first_week: u32,
}

impl LentDates {
    pub fn monday(&self) -> NaiveDate {
        week_one_of(self.start, self.first_week)
    }

    /// Medium, Low from module unlocks (never raised on the borrower's side: §6.7 rule 3 only
    /// raises Medium).
    pub fn confidence(&self) -> Confidence {
        match self.kind {
            DonorKind::StudentDates | DonorKind::CourseDates => Confidence::Medium,
            DonorKind::ModuleUnlocks => Confidence::Low,
        }
    }
}

/// What the other courses of a session give one course.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lend {
    Dates(LentDates),
    /// The courses of the strongest kind disagree on week 1: nothing is lent. `lenders`: those
    /// of them other than this course, each with what it would lend (a course with dates of its
    /// own names the ones that put week 1 elsewhere).
    Split {
        session: String,
        lenders: Vec<LentDates>,
    },
    /// The strongest kind agrees on week 1, but no other course that agrees with it can lend:
    /// nothing is lent.
    Unlent {
        session: String,
    },
}

impl Lend {
    pub fn dates(&self) -> Option<&LentDates> {
        match self {
            Lend::Dates(dates) => Some(dates),
            Lend::Split { .. } | Lend::Unlent { .. } => None,
        }
    }
}

/// What one course is lent. `own` is what the course could lend itself, `others` what the other
/// courses of its session can. `show_hidden`: whether the reader may name a hidden lender.
///
/// The strongest kind among all of them decides week 1. If its courses disagree on it, it is a
/// split for every course of the session, the disagreeing lenders too. Otherwise the course
/// borrows from the others of that kind whose weeks can be lent (`Donor::lends`), or, when none
/// can, from those of the next kind that can and agree with that week 1. A course of the
/// strongest kind itself borrows nothing beyond that (its own evidence is the best there is);
/// any other course that gets nothing is told so (`Lend::Unlent`).
pub fn lend(
    session: &str,
    own: Option<&Donor>,
    others: &[Donor],
    show_hidden: bool,
) -> Option<Lend> {
    let all: Vec<&Donor> = own.into_iter().chain(others).collect();
    let best = all.iter().map(|d| d.kind).min()?;
    let top: Vec<&Donor> = all.into_iter().filter(|d| d.kind == best).collect();
    let monday = top[0].monday();
    let mut lenders: Vec<&Donor> = others.iter().filter(|d| d.kind == best).collect();
    // A fixed order, so every view names the same course.
    lenders.sort_by(|a, b| a.course.cmp(&b.course).then(a.start.cmp(&b.start)));
    let lent = |donor: &Donor| LentDates {
        course: (!donor.hidden || show_hidden).then(|| donor.course.clone()),
        session: session.to_string(),
        kind: donor.kind,
        start: donor.start,
        end: donor.end,
        exams_end: donor.exams_end,
        first_week: donor.first_week,
    };
    if top.iter().any(|d| d.monday() != monday) {
        return Some(Lend::Split {
            session: session.to_string(),
            lenders: lenders.into_iter().map(lent).collect(),
        });
    }
    // They agree on week 1: of the strongest kind with a course that can lend it, the first by
    // name that states an end lends.
    let mut kinds: Vec<DonorKind> = others.iter().map(|d| d.kind).collect();
    kinds.sort();
    kinds.dedup();
    for kind in kinds {
        let mut lenders: Vec<&Donor> = others
            .iter()
            .filter(|d| d.kind == kind && d.lends && d.monday() == monday)
            .collect();
        lenders.sort_by(|a, b| a.course.cmp(&b.course).then(a.start.cmp(&b.start)));
        if let Some(donor) = lenders.iter().find(|d| d.end.is_some()).or(lenders.first()) {
            return Some(Lend::Dates(lent(donor)));
        }
    }
    (own.is_none_or(|own| own.kind != best) && !others.is_empty()).then(|| Lend::Unlent {
        session: session.to_string(),
    })
}

/// One course's own result, worked out with nothing lent to it.
pub struct DonorInput<'a> {
    pub course: &'a Course,
    pub data: &'a CourseTermData,
    /// The course's accepted calendar, if any.
    pub calendar: Option<&'a CalendarInForce>,
    pub resolved: &'a ResolvedTerm,
    pub modules: &'a [Module],
    pub today: NaiveDate,
}

/// What the course can lend to the others of its session, if anything (module docs).
pub fn donor(input: &DonorInput<'_>) -> Option<Donor> {
    let resolved = input.resolved;
    let term = &resolved.resolution;
    // A calendar's own week numbering, and whether its weeks can be lent at all.
    let first_week = input
        .calendar
        .and_then(|in_force| in_force.calendar.segments.first())
        .map_or(1, |s| s.first_week_number);
    let lends = input
        .calendar
        .is_none_or(|in_force| in_force.calendar.lendable());
    let donor = |kind, start, end, exams_end| Donor {
        course: course_label(input.course),
        hidden: input.course.hidden,
        kind,
        start,
        end,
        exams_end,
        lends,
        first_week,
    };
    match term.anchor {
        TermAnchorSource::StudentConfirmed
            if term.anchor_origin != Some(CalendarOrigin::Legacy) =>
        {
            let start = term.teaching.first()?.first_class;
            let (end, exams_end) = match input.calendar {
                Some(in_force) => (
                    in_force.calendar.segments.last().and_then(|s| s.last_class),
                    in_force.calendar.exam_period.map(|period| period.end),
                ),
                None => (input.data.user_term_end, None),
            };
            Some(donor(DonorKind::StudentDates, start, end, exams_end))
        }
        TermAnchorSource::LmsCourseDates
        | TermAnchorSource::LmsTerm
        | TermAnchorSource::FolderConfig
            if term.anchor_confidence == Confidence::Medium && established(resolved) =>
        {
            let segment = term.teaching.first()?;
            // Only the LMS course's own end is stated by the course: a term's end is the
            // school's, and a clipped end was replaced by the session window's.
            let end = segment.last_class.filter(|_| {
                term.anchor == TermAnchorSource::LmsCourseDates && !resolved.end_clipped
            });
            Some(donor(
                DonorKind::CourseDates,
                segment.first_class,
                end,
                None,
            ))
        }
        TermAnchorSource::PublishedWeekLabels | TermAnchorSource::NoAnchor => {
            let window = resolved.session.as_ref().map(|s| s.window);
            let monday = unlock_week_one(input.modules, input.today, resolved.tz, window)?;
            // Its own fit says otherwise: the course doesn't know its week 1 well enough to lend.
            if term.anchor == TermAnchorSource::PublishedWeekLabels
                && let Some(fitted) = term.week_one_monday
                && days_between(fitted, monday).abs() >= 7
            {
                return None;
            }
            Some(donor(DonorKind::ModuleUnlocks, monday, None, None))
        }
        _ => None,
    }
}

/// The course's own dates are borne out over its term: at least two of its week-numbered
/// posting days (materials and module unlocks, `fit::observations`) fall in the week they name
/// or the week before it (posted a week ahead), and no more of them fall elsewhere.
fn established(resolved: &ResolvedTerm) -> bool {
    let term = &resolved.resolution;
    let (agree, disagree) = resolved
        .observations
        .iter()
        .filter_map(|o| {
            let expected = teaching_week_on(term, o.day)?;
            Some(o.week == expected || o.week == expected + 1)
        })
        .fold((0, 0), |(agree, disagree), ok| {
            if ok {
                (agree + 1, disagree)
            } else {
                (agree, disagree + 1)
            }
        });
    agree >= 2 && disagree <= agree
}

/// The course's name in evidence: its code, or its name without one.
pub fn course_label(course: &Course) -> String {
    course
        .code
        .as_deref()
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .unwrap_or(course.name.as_str())
        .to_string()
}

/// Week 1 by the course's module unlocks: each unlock day on or before `today` (and inside
/// `window`, the session's, when given) whose modules name exactly one week gives
/// `monday_of(day) − 7 × (week − 1)`, the day counted like a posting day of the fit (a Friday
/// evening or a weekend counts for the next Monday). Two or more such days, all giving the same
/// Monday, which itself passes the window check (§6.3: not more than 14 days before the window,
/// not after it), or nothing.
pub(crate) fn unlock_week_one(
    modules: &[Module],
    today: NaiveDate,
    tz: Option<Tz>,
    window: Option<DateSpan>,
) -> Option<NaiveDate> {
    let mut weeks_by_day: BTreeMap<NaiveDate, BTreeSet<u32>> = BTreeMap::new();
    for module in modules {
        let (Some(at), Some(week)) = (module.unlock_at, module.week_hint) else {
            continue;
        };
        let local = course_datetime(at, tz);
        if local.date() > today || window.is_some_and(|w| !w.contains(local.date())) {
            continue;
        }
        weeks_by_day
            .entry(posting_day(local))
            .or_default()
            .insert(week);
    }
    let mondays: Vec<NaiveDate> = weeks_by_day
        .iter()
        .filter_map(|(day, weeks)| match weeks.iter().collect::<Vec<_>>()[..] {
            [week] => Some(add_days(week_one_monday(*day), -7 * (i64::from(*week) - 1))),
            _ => None,
        })
        .collect();
    let monday = match mondays[..] {
        [first, ..] if mondays.len() >= 2 && mondays.iter().all(|m| *m == first) => first,
        _ => return None,
    };
    let in_window = window
        .is_none_or(|w| days_between(monday, w.start) <= MAX_DAYS_BEFORE_WINDOW && monday <= w.end);
    in_window.then_some(monday)
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDateTime, TimeZone, Utc};

    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn donor(course: &str, kind: DonorKind, start: NaiveDate, end: Option<NaiveDate>) -> Donor {
        Donor {
            course: course.into(),
            hidden: false,
            kind,
            start,
            end,
            exams_end: None,
            lends: true,
            first_week: 1,
        }
    }

    #[test]
    fn the_strongest_kind_lends_when_its_courses_agree() {
        assert_eq!(lend("20269", None, &[], true), None);
        let student = donor("DEM240H7", DonorKind::StudentDates, date(2026, 9, 8), None);
        let dates = donor("DEM205H7", DonorKind::CourseDates, date(2026, 9, 21), None);
        let unlocks = donor(
            "DEM332H7",
            DonorKind::ModuleUnlocks,
            date(2026, 8, 31),
            None,
        );
        // Student dates beat course dates beat unlocks, whatever the weaker ones say.
        let Some(Lend::Dates(lent)) = lend(
            "20269",
            None,
            &[unlocks.clone(), dates.clone(), student.clone()],
            true,
        ) else {
            panic!("the student's dates lend");
        };
        assert_eq!(
            (lent.course.as_deref(), lent.kind),
            (Some("DEM240H7"), DonorKind::StudentDates)
        );
        assert_eq!(lent.monday(), date(2026, 9, 7));
        assert_eq!(lent.confidence(), Confidence::Medium);
        let Some(Lend::Dates(lent)) = lend("20269", None, &[unlocks.clone(), dates], true) else {
            panic!("course dates lend");
        };
        assert_eq!(lent.kind, DonorKind::CourseDates);
        // Module unlocks alone lend at Low.
        let Some(Lend::Dates(lent)) = lend("20269", None, &[unlocks], true) else {
            panic!("unlocks lend");
        };
        assert_eq!(lent.confidence(), Confidence::Low);
    }

    #[test]
    fn agreeing_lenders_lend_the_one_with_an_end_and_disagreeing_ones_lend_nothing() {
        let a = donor("DEM205H7", DonorKind::CourseDates, date(2026, 9, 9), None);
        let b = donor(
            "DEM240H7",
            DonorKind::CourseDates,
            date(2026, 9, 8),
            Some(date(2026, 12, 8)),
        );
        let Some(Lend::Dates(lent)) = lend("20269", None, &[a.clone(), b], true) else {
            panic!("same week 1");
        };
        assert_eq!(
            (lent.course.as_deref(), lent.end),
            (Some("DEM240H7"), Some(date(2026, 12, 8)))
        );
        let late = donor("DEM310H7", DonorKind::CourseDates, date(2026, 9, 15), None);
        let split = |lenders: &[&Donor]| {
            Some(Lend::Split {
                session: "20269".into(),
                lenders: lenders
                    .iter()
                    .map(|d| LentDates {
                        course: Some(d.course.clone()),
                        session: "20269".into(),
                        kind: DonorKind::CourseDates,
                        start: d.start,
                        end: d.end,
                        exams_end: None,
                        first_week: 1,
                    })
                    .collect(),
            })
        };
        assert_eq!(
            lend("20269", None, &[late.clone(), a.clone()], true),
            split(&[&a, &late])
        );
        // A course that is itself one of the disagreeing lenders gets the split too, not the
        // other's date, and learns which course disagrees.
        assert_eq!(
            lend("20269", Some(&a), std::slice::from_ref(&late), true),
            split(&[&late])
        );
        // The only one of the strongest kind borrows nothing.
        let unlocks = donor("DEM332H7", DonorKind::ModuleUnlocks, date(2026, 9, 7), None);
        assert_eq!(lend("20269", Some(&late), &[unlocks], true), None);
    }

    #[test]
    fn a_hidden_lender_is_named_only_to_a_reader_that_shows_hidden_courses() {
        let mut hidden = donor("DEM240H7", DonorKind::StudentDates, date(2026, 9, 8), None);
        hidden.hidden = true;
        let named = |show| {
            lend("20269", None, std::slice::from_ref(&hidden), show)
                .and_then(|l| l.dates().cloned())
                .map(|d| d.course)
        };
        assert_eq!(named(true), Some(Some("DEM240H7".into())));
        assert_eq!(named(false), Some(None));
    }

    /// A module opening at `local` (taken as UTC: the tests pass no zone).
    fn module(week: u32, local: &str) -> Module {
        let at = NaiveDateTime::parse_from_str(local, "%Y-%m-%d %H:%M").unwrap();
        Module {
            id: format!("m{week}-{local}"),
            course_id: "c".into(),
            name: format!("Week {week}"),
            position: Some(i64::from(week)),
            unlock_at: Some(Utc.from_utc_datetime(&at)),
            week_hint: Some(week),
        }
    }

    #[test]
    fn unlocks_give_week_one_only_when_two_or_more_agree() {
        let today = date(2026, 10, 16);
        // One unlock lends nothing.
        assert_eq!(
            unlock_week_one(&[module(5, "2026-10-05 09:00")], today, None, None),
            None
        );
        // Week 5 on Monday Oct 5 and week 6 on Sunday Oct 11 (counts for Monday Oct 12).
        let agreeing = [module(5, "2026-10-05 09:00"), module(6, "2026-10-11 20:00")];
        assert_eq!(
            unlock_week_one(&agreeing, today, None, None),
            Some(date(2026, 9, 7))
        );
        // One early unlock makes them disagree.
        let early = [module(5, "2026-10-05 09:00"), module(7, "2026-10-12 09:00")];
        assert_eq!(unlock_week_one(&early, today, None, None), None);
        // A day naming two weeks says nothing; a future unlock doesn't count.
        let batch = [
            module(5, "2026-10-05 09:00"),
            module(2, "2026-09-14 09:00"),
            module(3, "2026-09-14 09:00"),
            module(7, "2026-10-19 09:00"),
        ];
        assert_eq!(unlock_week_one(&batch, today, None, None), None);
    }

    /// Only unlocks inside the session's window count, and the Monday they give must pass the
    /// window check: modules copied from last year keep their old dates and lend nothing.
    #[test]
    fn unlocks_outside_the_session_window_count_for_nothing() {
        let today = date(2026, 10, 16);
        let fall = Some(DateSpan {
            start: date(2026, 9, 1),
            end: date(2026, 12, 31),
        });
        let last_year = [module(2, "2025-09-15 09:00"), module(3, "2025-09-22 09:00")];
        assert_eq!(
            unlock_week_one(&last_year, today, None, None),
            Some(date(2025, 9, 8))
        );
        assert_eq!(unlock_week_one(&last_year, today, None, fall), None);
        // Last year's modules kept next to this year's: only this year's count.
        let both = [
            last_year.to_vec(),
            vec![module(4, "2026-09-28 09:00"), module(5, "2026-10-05 09:00")],
        ]
        .concat();
        assert_eq!(unlock_week_one(&both, today, None, None), None);
        assert_eq!(
            unlock_week_one(&both, today, None, fall),
            Some(date(2026, 9, 7))
        );
        // Inside the window, but naming weeks that put week 1 in early August: not this session's.
        let early = [
            module(9, "2026-09-28 09:00"),
            module(10, "2026-10-05 09:00"),
        ];
        assert_eq!(unlock_week_one(&early, today, None, fall), None);
        let sound = [module(4, "2026-09-28 09:00"), module(5, "2026-10-05 09:00")];
        assert_eq!(
            unlock_week_one(&sound, today, None, fall),
            Some(date(2026, 9, 7))
        );
    }

    /// The fit's day rule: a module opening on a Friday evening counts for the next Monday, one
    /// opening on a Thursday for its own week, exactly as the fit counts a posting.
    #[test]
    fn unlocks_count_their_day_like_the_fit() {
        let today = date(2026, 10, 16);
        let friday_evening = [module(5, "2026-10-05 09:00"), module(6, "2026-10-09 19:00")];
        assert_eq!(
            unlock_week_one(&friday_evening, today, None, None),
            Some(date(2026, 9, 7))
        );
        // Friday morning is still its own week: week 6 then puts week 1 a week earlier, so the
        // two unlocks disagree.
        let friday_morning = [module(5, "2026-10-05 09:00"), module(6, "2026-10-09 09:00")];
        assert_eq!(unlock_week_one(&friday_morning, today, None, None), None);
        let thursday = [module(5, "2026-10-01 09:00"), module(6, "2026-10-08 09:00")];
        assert_eq!(
            unlock_week_one(&thursday, today, None, None),
            Some(date(2026, 8, 31))
        );
        // The fit puts the same days in the same weeks.
        for days in [&friday_evening[..], &thursday[..]] {
            let expected = unlock_week_one(days, today, None, None).unwrap();
            for o in super::super::fit::observations(days, &[], None, today) {
                assert_eq!(
                    add_days(week_one_monday(o.day), -7 * (i64::from(o.week) - 1)),
                    expected
                );
            }
        }
    }
}
