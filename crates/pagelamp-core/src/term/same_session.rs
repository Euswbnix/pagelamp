//! Courses of one session teach on the same days (docs/design/v0.3-course-calendar.md §6.4,
//! the anchor between 5 and 6, and §14). A course whose own dates are confirmed or well
//! established lends its week 1, and the ends of classes and exams its own source states, to the
//! other courses with the same `session::SessionKey`, where they have nothing better.
//!
//! Who lends (a donor), from its OWN evidence only: a lent date never lends on, so there are no
//! chains and no cycles (the views work every course out alone first, then lend).
//! - `StudentDates`: the student's dates (the dates form, the CLI, an accepted calendar), not
//!   the 0.1 dates the student never confirmed. Its ends: the student's last day of classes, or
//!   the calendar's last class and end of exams.
//! - `CourseDates`: its own LMS course dates, LMS term or folder dates, which the timeline raised
//!   to High because an independent signal agrees (§6.7 rule 3), not disputed by a lower
//!   source. Only LMS course dates state an end it lends.
//! - `ModuleUnlocks`: a week that is High from module unlocks alone, when two or more unlock
//!   days, each naming one week, all give the same week 1. Week 1 only, lent at Low.
//!
//! The strongest kind present decides. Its donors must agree on week 1, else nothing is lent
//! and the borrowers say the session's courses disagree (`Lend::Split`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use super::evidence::EvidenceSignal;
use super::{CalendarOrigin, TermAnchorSource};
use crate::calendar::CalendarInForce;
use crate::dates::{Tz, add_days, course_date, week_one_monday};
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
    pub kind: DonorKind,
    /// The first day of classes (for `ModuleUnlocks`, the Monday of week 1).
    pub start: NaiveDate,
    /// The last day of classes, when the donor's own source states it.
    pub end: Option<NaiveDate>,
    /// The end of exams, when the donor's accepted calendar states it.
    pub exams_end: Option<NaiveDate>,
}

impl Donor {
    fn monday(&self) -> NaiveDate {
        week_one_monday(self.start)
    }
}

/// The dates a course borrows from the others of its session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LentDates {
    /// The lending course's code, or its name without one.
    pub course: String,
    /// The session code ("20269").
    pub session: String,
    pub kind: DonorKind,
    pub start: NaiveDate,
    pub end: Option<NaiveDate>,
    pub exams_end: Option<NaiveDate>,
}

impl LentDates {
    pub fn monday(&self) -> NaiveDate {
        week_one_monday(self.start)
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
    /// The donors of the strongest kind disagree on week 1: nothing is lent.
    Split {
        session: String,
    },
}

impl Lend {
    pub fn dates(&self) -> Option<&LentDates> {
        match self {
            Lend::Dates(dates) => Some(dates),
            Lend::Split { .. } => None,
        }
    }
}

/// What `donors` (the other courses of the session, never the borrower itself) lend; `None`
/// when no course of the session can lend.
pub fn lend(session: &str, donors: &[Donor]) -> Option<Lend> {
    let best = donors.iter().map(|d| d.kind).min()?;
    let mut top: Vec<&Donor> = donors.iter().filter(|d| d.kind == best).collect();
    // A fixed order, so every view picks the same course to name.
    top.sort_by(|a, b| a.course.cmp(&b.course).then(a.start.cmp(&b.start)));
    let monday = top[0].monday();
    if top.iter().any(|d| d.monday() != monday) {
        return Some(Lend::Split {
            session: session.to_string(),
        });
    }
    // They agree on week 1: the one that states an end lends it (the first, by name).
    let donor = top.iter().find(|d| d.end.is_some()).unwrap_or(&top[0]);
    Some(Lend::Dates(LentDates {
        course: donor.course.clone(),
        session: session.to_string(),
        kind: best,
        start: donor.start,
        end: donor.end,
        exams_end: donor.exams_end,
    }))
}

/// One course's own result, worked out without anything lent (the views' first pass).
pub struct DonorInput<'a> {
    pub course: &'a Course,
    pub data: &'a CourseTermData,
    /// The course's accepted calendar, if any.
    pub calendar: Option<&'a CalendarInForce>,
    pub resolved: &'a ResolvedTerm,
    /// The timeline's week confidence and the signal that chose the week.
    pub week_confidence: Confidence,
    pub chosen: Option<EvidenceSignal>,
    pub modules: &'a [Module],
    pub today: NaiveDate,
}

/// What the course can lend to the others of its session, if anything (module docs).
pub fn donor(input: &DonorInput<'_>) -> Option<Donor> {
    let term = &input.resolved.resolution;
    let course = course_label(input.course);
    let high_from =
        |signal| input.week_confidence == Confidence::High && input.chosen == Some(signal);
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
            Some(Donor {
                course,
                kind: DonorKind::StudentDates,
                start,
                end,
                exams_end,
            })
        }
        TermAnchorSource::LmsCourseDates
        | TermAnchorSource::LmsTerm
        | TermAnchorSource::FolderConfig
            if term.anchor_confidence == Confidence::Medium && high_from(EvidenceSignal::Dates) =>
        {
            let segment = term.teaching.first()?;
            // Only the LMS course's own end is stated by the course: a term's end is the
            // school's, and a clipped end was replaced by the session window's.
            let end = segment.last_class.filter(|_| {
                term.anchor == TermAnchorSource::LmsCourseDates && !input.resolved.end_clipped
            });
            Some(Donor {
                course,
                kind: DonorKind::CourseDates,
                start: segment.first_class,
                end,
                exams_end: None,
            })
        }
        _ if high_from(EvidenceSignal::ModuleUnlock) => {
            let monday = unlock_week_one(input.modules, input.today, input.resolved.tz)?;
            Some(Donor {
                course,
                kind: DonorKind::ModuleUnlocks,
                start: monday,
                end: None,
                exams_end: None,
            })
        }
        _ => None,
    }
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

/// Week 1 by the course's module unlocks: each unlock day on or before `today` whose modules
/// name exactly one week gives `monday_of(day) − 7 × (week − 1)` (a weekend unlock counts for
/// the next Monday). Two or more such days, all giving the same Monday, or nothing.
pub(crate) fn unlock_week_one(
    modules: &[Module],
    today: NaiveDate,
    tz: Option<Tz>,
) -> Option<NaiveDate> {
    let mut weeks_by_day: BTreeMap<NaiveDate, BTreeSet<u32>> = BTreeMap::new();
    for module in modules {
        let (Some(at), Some(week)) = (module.unlock_at, module.week_hint) else {
            continue;
        };
        let day = course_date(at, tz);
        if day <= today {
            weeks_by_day.entry(day).or_default().insert(week);
        }
    }
    let mondays: Vec<NaiveDate> = weeks_by_day
        .iter()
        .filter_map(|(day, weeks)| match weeks.iter().collect::<Vec<_>>()[..] {
            [week] => Some(add_days(week_one_monday(*day), -7 * (i64::from(*week) - 1))),
            _ => None,
        })
        .collect();
    match mondays[..] {
        [first, ..] if mondays.len() >= 2 && mondays.iter().all(|m| *m == first) => Some(first),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn donor(course: &str, kind: DonorKind, start: NaiveDate, end: Option<NaiveDate>) -> Donor {
        Donor {
            course: course.into(),
            kind,
            start,
            end,
            exams_end: None,
        }
    }

    #[test]
    fn the_strongest_kind_lends_when_its_courses_agree() {
        assert_eq!(lend("20269", &[]), None);
        let student = donor("DEM240H5", DonorKind::StudentDates, date(2026, 9, 8), None);
        let unlocks = donor(
            "DEM332H5",
            DonorKind::ModuleUnlocks,
            date(2026, 8, 31),
            None,
        );
        let Some(Lend::Dates(dates)) = lend("20269", &[unlocks.clone(), student.clone()]) else {
            panic!("the student's dates lend");
        };
        assert_eq!(dates.course, "DEM240H5");
        assert_eq!(dates.kind, DonorKind::StudentDates);
        assert_eq!(dates.monday(), date(2026, 9, 7));
        assert_eq!(dates.confidence(), Confidence::Medium);
        // Module unlocks alone lend at Low.
        let Some(Lend::Dates(dates)) = lend("20269", &[unlocks]) else {
            panic!("unlocks lend");
        };
        assert_eq!(dates.confidence(), Confidence::Low);
    }

    #[test]
    fn agreeing_donors_lend_the_one_with_an_end_and_disagreeing_ones_lend_nothing() {
        let a = donor("DEM205H5", DonorKind::CourseDates, date(2026, 9, 9), None);
        let b = donor(
            "DEM240H5",
            DonorKind::CourseDates,
            date(2026, 9, 8),
            Some(date(2026, 12, 8)),
        );
        let Some(Lend::Dates(dates)) = lend("20269", &[a.clone(), b]) else {
            panic!("same week 1");
        };
        assert_eq!(
            (dates.course.as_str(), dates.end),
            ("DEM240H5", Some(date(2026, 12, 8)))
        );
        let late = donor("DEM310H5", DonorKind::CourseDates, date(2026, 9, 15), None);
        assert_eq!(
            lend("20269", &[a, late]),
            Some(Lend::Split {
                session: "20269".into()
            })
        );
    }

    fn module(week: u32, unlock: NaiveDate) -> Module {
        Module {
            id: format!("m{week}-{unlock}"),
            course_id: "c".into(),
            name: format!("Week {week}"),
            position: Some(i64::from(week)),
            unlock_at: Some(unlock.and_hms_opt(12, 0, 0).unwrap().and_utc()),
            week_hint: Some(week),
        }
    }

    #[test]
    fn unlocks_give_week_one_only_when_two_or_more_agree() {
        let today = date(2026, 10, 16);
        // One unlock lends nothing.
        assert_eq!(
            unlock_week_one(&[module(5, date(2026, 10, 5))], today, None),
            None
        );
        // Week 5 on Oct 5 and week 6 on Sunday Oct 11 (counts for Monday Oct 12): Sept 7.
        let agreeing = [module(5, date(2026, 10, 5)), module(6, date(2026, 10, 11))];
        assert_eq!(
            unlock_week_one(&agreeing, today, None),
            Some(date(2026, 9, 7))
        );
        // One early unlock makes them disagree.
        let early = [module(5, date(2026, 10, 5)), module(7, date(2026, 10, 12))];
        assert_eq!(unlock_week_one(&early, today, None), None);
        // A day naming two weeks says nothing; a future unlock doesn't count.
        let batch = [
            module(5, date(2026, 10, 5)),
            module(2, date(2026, 9, 14)),
            module(3, date(2026, 9, 14)),
            module(7, date(2026, 10, 19)),
        ];
        assert_eq!(unlock_week_one(&batch, today, None), None);
    }
}
