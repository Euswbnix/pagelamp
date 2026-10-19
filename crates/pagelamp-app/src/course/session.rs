//! "Apply these dates to my other courses this term" (docs/design/v0.3-course-calendar.md §14):
//! the other courses of a course's school session, copying the student's dates to some of them,
//! and putting them back exactly. Which courses share a session is core's rule alone
//! (`pagelamp_core::term::same_session`); a course without dates already borrows the session's
//! week 1 at read time, and copying makes the dates that course's own (High).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use pagelamp_core::model::TermAnchorSource;
use pagelamp_core::store::Store;
use pagelamp_core::term::{CONFIRMED_DATES_KEY, COPIED_DATES_KEY};
use pagelamp_core::views::{self, AsOf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::set_dates_confirmed;
use crate::{App, AppError, AppErrorKind, Result};

/// The other courses of a course's session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionCourses {
    /// The school's session code (e.g. "20269"); `None` when the course's code or name has
    /// none, and then `courses` is empty.
    pub session: Option<String>,
    /// The campus digit of the course's code; `None` when its code doesn't say (then no other
    /// course shares its session, and `courses` is empty).
    pub campus: Option<u8>,
    /// The other courses of the same session, section group and campus: removed ones left out,
    /// hidden ones in (`hidden`), the course itself not listed.
    pub courses: Vec<SessionCourse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionCourse {
    pub course_id: String,
    pub name: String,
    pub code: Option<String>,
    pub hidden: bool,
    /// The student set dates in that course (dates from PageLamp 0.1 included): applying
    /// replaces them, unless the course has a calendar of its own (`own_calendar`).
    pub own_dates: bool,
    /// Those dates as saved; both `None` without own dates. With a calendar of its own, its
    /// span as mirrored there (the end can be the end of exams).
    pub student_start: Option<NaiveDate>,
    pub student_end: Option<NaiveDate>,
    /// It counts its weeks from a calendar of its own (accepted from its syllabus or saved with
    /// the dates form): applying leaves it alone (`ApplySkipReason::OwnCalendar`).
    pub own_calendar: bool,
    /// What counts the course's weeks now (lent dates included).
    pub week_one_monday: Option<NaiveDate>,
    pub anchor: TermAnchorSource,
}

/// Why `apply_course_dates_to` left a course alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplySkipReason {
    /// Not a course of the same session.
    NotSameSession,
    /// It is the course to copy from.
    FromCourse,
    /// The course to copy from has no dates of the student's.
    NoDates,
    /// The course to copy from has dates from PageLamp 0.1 the student never confirmed: saving
    /// them confirms them first.
    DatesNotConfirmed,
    /// It has a calendar of its own (accepted from its syllabus or saved with the dates form),
    /// which counts its weeks whatever its term dates say: the student changes it there.
    OwnCalendar,
    /// No such course, or it was removed.
    UnknownCourse,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SkippedCourse {
    /// The course as asked for (its id when it is known).
    pub course_id: String,
    pub reason: ApplySkipReason,
}

/// What a course had before `apply_course_dates_to` replaced it: `restore_course_dates` puts it
/// back exactly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReplacedDates {
    pub course_id: String,
    /// The student's dates before (both `None`: it had none).
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    /// Those were dates from PageLamp 0.1 the student had never confirmed.
    pub legacy: bool,
    /// They were themselves copied from this course (its id).
    pub copied_from: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ApplyDatesReport {
    /// The courses that now have the dates (ids).
    pub applied: Vec<String>,
    pub skipped: Vec<SkippedCourse>,
    /// One per applied course, for undo (`restore_course_dates`).
    pub replaced: Vec<ReplacedDates>,
}

impl App {
    /// The other courses of `course`'s session (code, name or id), each with the dates it
    /// counts its weeks from now, for "apply these dates to my other courses this term".
    pub fn session_courses(&self, course: &str) -> Result<SessionCourses> {
        let store = self.read_store()?;
        let course = store.resolve_course_with(course, true)?;
        let view = views::session_courses(&store, &course, AsOf::now_local())?;
        let courses = view
            .courses
            .into_iter()
            .map(|other| SessionCourse {
                own_dates: other.student_start.is_some() || other.student_end.is_some(),
                own_calendar: other.own_calendar,
                course_id: other.course.id,
                name: other.course.name,
                code: other.course.code,
                hidden: other.course.hidden,
                student_start: other.student_start,
                student_end: other.student_end,
                week_one_monday: other.term.week_one_monday,
                anchor: other.term.anchor,
            })
            .collect();
        Ok(SessionCourses {
            session: view.session,
            campus: view.campus,
            courses,
        })
    }

    /// Copy the student's first and last day of classes of `from` to each course of `to` (codes,
    /// names or ids) of the same session, as that course's own confirmed dates, replacing any
    /// it had. With a calendar in force, `from`'s dates are that calendar's first and last day
    /// of classes. Only confirmed dates are copied (0.1 dates the student never confirmed are
    /// refused: saving them first confirms them). A course with a calendar of its own is
    /// skipped: its calendar counts its weeks whatever its term dates say. A course named twice
    /// gets them once. Each copy remembers where it came from until the course's own dates are
    /// saved again. One transaction; the report says what was skipped and what was replaced
    /// (undo: `restore_course_dates`). A calendar in force with two parts or with a break its
    /// weeks skip is refused: two dates can't carry it.
    pub fn apply_course_dates_to(&self, from: &str, to: Vec<String>) -> Result<ApplyDatesReport> {
        let store = self.write_store()?;
        let from = store.resolve_course_with(from, true)?;
        let others = views::session_courses(&store, &from, AsOf::now_local())?.courses;
        let session: BTreeSet<String> = others.iter().map(|o| o.course.id.clone()).collect();
        let own_calendars: BTreeSet<String> = others
            .iter()
            .filter(|o| o.own_calendar)
            .map(|o| o.course.id.clone())
            .collect();
        let source_calendar = views::calendar_of(&store, &from.id)?;
        if source_calendar.as_ref().is_some_and(|in_force| {
            in_force.calendar.segments.len() > 1
                || in_force.calendar.breaks.iter().any(|b| !b.numbered)
        }) {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                "This course's calendar has breaks or two parts; set the other courses' dates \
                 on their own.",
            ));
        }
        let (start, end) = match &source_calendar {
            // `user_term_end` mirrors the calendar's end of exams when it has one.
            Some(in_force) => (
                in_force.calendar.segments.first().map(|s| s.first_class),
                in_force.calendar.segments.last().and_then(|s| s.last_class),
            ),
            None => {
                let dates = store.course_term_data(&from.id)?.unwrap_or_default();
                (dates.user_term_start, dates.user_term_end)
            }
        };
        Ok(store.in_transaction(|store| {
            let confirmed = confirmed_courses(store)?;
            let mut copied = copied_courses(store)?;
            let mut report = ApplyDatesReport::default();
            let mut seen = BTreeSet::new();
            for asked in to {
                let target = match store.resolve_course_with(&asked, true) {
                    Ok(course) => course,
                    Err(pagelamp_core::Error::NotFound(_)) => {
                        report.skipped.push(SkippedCourse {
                            course_id: asked,
                            reason: ApplySkipReason::UnknownCourse,
                        });
                        continue;
                    }
                    Err(err) => return Err(err),
                };
                if !seen.insert(target.id.clone()) {
                    continue;
                }
                let reason = if target.id == from.id {
                    Some(ApplySkipReason::FromCourse)
                } else if !session.contains(&target.id) {
                    Some(ApplySkipReason::NotSameSession)
                } else if start.is_none() && end.is_none() {
                    Some(ApplySkipReason::NoDates)
                } else if source_calendar.is_none() && !confirmed.contains(&from.id) {
                    // An accepted or saved calendar is the student's own.
                    Some(ApplySkipReason::DatesNotConfirmed)
                } else if own_calendars.contains(&target.id) {
                    Some(ApplySkipReason::OwnCalendar)
                } else {
                    None
                };
                if let Some(reason) = reason {
                    report.skipped.push(SkippedCourse {
                        course_id: target.id,
                        reason,
                    });
                    continue;
                }
                let before = store.course_term_data(&target.id)?.unwrap_or_default();
                let had_dates = before.user_term_start.is_some() || before.user_term_end.is_some();
                store.set_course_term(&target.id, start, end)?;
                set_dates_confirmed(store, &target.id, true)?;
                let earlier = copied.insert(target.id.clone(), from.id.clone());
                report.replaced.push(ReplacedDates {
                    course_id: target.id.clone(),
                    start: before.user_term_start,
                    end: before.user_term_end,
                    legacy: had_dates && !confirmed.contains(&target.id),
                    copied_from: earlier,
                });
                report.applied.push(target.id);
            }
            if !report.applied.is_empty() {
                store.set_setting(COPIED_DATES_KEY, &copied)?;
            }
            Ok(report)
        })?)
    }

    /// Undo `apply_course_dates_to`: each course gets back the dates it had, confirmed or not as
    /// before, and where they had been copied from, if anywhere. A course removed since (gone,
    /// or waiting to be purged) is left alone, and so is one with a calendar of its own since
    /// (as applying leaves it alone): its term dates mirror that calendar. One transaction.
    pub fn restore_course_dates(&self, replaced: Vec<ReplacedDates>) -> Result<()> {
        let store = self.write_store()?;
        Ok(store.in_transaction(|store| {
            let mut copied = copied_courses(store)?;
            for dates in &replaced {
                if store.get_course(&dates.course_id)?.is_none()
                    || store.tombstone(&dates.course_id)?.is_some()
                    || views::calendar_of(store, &dates.course_id)?.is_some()
                {
                    continue;
                }
                store.set_course_term(&dates.course_id, dates.start, dates.end)?;
                let has_dates = dates.start.is_some() || dates.end.is_some();
                set_dates_confirmed(store, &dates.course_id, has_dates && !dates.legacy)?;
                // The earlier "copied from", while that course is still there.
                match &dates.copied_from {
                    Some(from) if store.get_course(from)?.is_some() => {
                        copied.insert(dates.course_id.clone(), from.clone())
                    }
                    _ => copied.remove(&dates.course_id),
                };
            }
            store.set_setting(COPIED_DATES_KEY, &copied)?;
            Ok(())
        })?)
    }
}

fn confirmed_courses(store: &Store) -> pagelamp_core::Result<BTreeSet<String>> {
    Ok(store
        .setting_or_absent::<BTreeSet<String>>(CONFIRMED_DATES_KEY)?
        .unwrap_or_default())
}

fn copied_courses(store: &Store) -> pagelamp_core::Result<BTreeMap<String, String>> {
    Ok(store
        .setting_or_absent::<BTreeMap<String, String>>(COPIED_DATES_KEY)?
        .unwrap_or_default())
}

/// The course's own dates were saved or cleared: they are no longer copied ones.
pub(crate) fn forget_copied(store: &Store, course_id: &str) -> pagelamp_core::Result<()> {
    let mut copied = copied_courses(store)?;
    if copied.remove(course_id).is_some() {
        store.set_setting(COPIED_DATES_KEY, &copied)?;
    }
    Ok(())
}
