//! Store methods of course calendars (schema 4, `course_calendars`; docs/design/
//! v0.3-course-calendar.md §3.2, §7.10): proposals, the calendar in force, and what the
//! student's accept, dismiss or dates form does to them.
//!
//! - One accepted row per course (the calendar in force) and at most one proposed row per
//!   course and origin: a newer proposal from the same reader replaces the older one.
//! - Accepting (or the dates form) supersedes the calendar in force and mirrors the first class
//!   and the end of exams (else the last day of classes) into `user_term_start/end`, so v3
//!   readers still see the right span (§3.2 "Version skew").
//! - Retention: the latest 3 superseded rows per course (for undo) and dismissed rows for 30
//!   days (their fingerprint stops the same materials from being proposed again).
//!
//! Rows hold material text (labels, topics, quotes): local display only (§7.11).

use chrono::Duration;
use rusqlite::{Row, params};
use serde::{Deserialize, Serialize};

use super::{Store, TextValue, expect_changed, get_opt_value, get_value, opt_date_text, ts_text};
use crate::Result;
use crate::ai_gate::ManifestEntry;
use crate::calendar::CourseCalendar;
use crate::calendar::assemble::{CalendarConflict, ProposedDate};
use crate::calendar::validate::DropCount;
use crate::model::Timestamp;
use crate::term::CalendarOrigin;

/// Superseded rows kept per course (for undo).
pub const KEEP_SUPERSEDED: usize = 3;
/// Days a dismissed row is kept.
pub const KEEP_DISMISSED_DAYS: i64 = 30;

/// Where a row is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalendarState {
    Proposed,
    /// The calendar in force.
    Accepted,
    Dismissed,
    /// Was in force; replaced by a later accept or dates form.
    Superseded,
}

impl CalendarState {
    pub fn as_str(self) -> &'static str {
        match self {
            CalendarState::Proposed => "proposed",
            CalendarState::Accepted => "accepted",
            CalendarState::Dismissed => "dismissed",
            CalendarState::Superseded => "superseded",
        }
    }
}

impl TextValue for CalendarState {
    fn parse_text(text: &str) -> Option<Self> {
        [
            CalendarState::Proposed,
            CalendarState::Accepted,
            CalendarState::Dismissed,
            CalendarState::Superseded,
        ]
        .into_iter()
        .find(|state| state.as_str() == text)
    }
}

impl TextValue for CalendarOrigin {
    fn parse_text(text: &str) -> Option<Self> {
        [
            CalendarOrigin::User,
            CalendarOrigin::Legacy,
            CalendarOrigin::Scan,
            CalendarOrigin::Ai,
            CalendarOrigin::AiApp,
            CalendarOrigin::Restored,
        ]
        .into_iter()
        .find(|origin| origin.as_str() == text)
    }
}

/// What the checks found, stored with a row (`checks_json`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarChecks {
    pub conflicts: Vec<CalendarConflict>,
    pub dropped: Vec<DropCount>,
    pub low_quality: bool,
    pub passing: bool,
    /// V8 found disagreement: once accepted, the calendar counts weeks at Medium.
    pub disagrees_with_notes: bool,
    /// Show the one-time question (b) reminder with this proposal (D37 option 2).
    pub sharing_reminder: bool,
}

/// Where an AI-read calendar came from (the "AI-generated · backend · model · date" label).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarProvenance {
    pub generation_id: Option<String>,
    /// As the label shows it ("ChatGPT plan (through OpenAI Codex)").
    pub backend_label: String,
    pub model: String,
    pub prompt_version: u32,
}

/// A row to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCalendarRow {
    pub course_id: String,
    pub origin: CalendarOrigin,
    pub calendar: CourseCalendar,
    /// The dates with their quotes (`evidence_json`); empty for the student's own dates.
    pub dates: Vec<ProposedDate>,
    pub checks: CalendarChecks,
    pub manifest: Vec<ManifestEntry>,
    pub fingerprint: String,
    pub provenance: Option<CalendarProvenance>,
}

/// A stored row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarRow {
    pub id: i64,
    pub course_id: String,
    pub origin: CalendarOrigin,
    pub state: CalendarState,
    pub calendar: CourseCalendar,
    pub dates: Vec<ProposedDate>,
    pub checks: CalendarChecks,
    pub manifest: Vec<ManifestEntry>,
    pub fingerprint: String,
    pub provenance: Option<CalendarProvenance>,
    pub created_at: Timestamp,
    pub decided_at: Option<Timestamp>,
}

const CALENDAR_COLUMNS: &str = "id, course_id, origin, state, calendar_json, evidence_json, \
     checks_json, manifest_json, fingerprint, generation_id, backend, model, prompt_version, \
     created_at, decided_at";

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("calendar rows serialise")
}

/// A JSON column, or a conversion error naming it (never a panic on a bad row).
fn from_json<T: serde::de::DeserializeOwned>(row: &Row<'_>, column: &str) -> rusqlite::Result<T> {
    let text: String = row.get(column)?;
    serde_json::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("bad {column}: {error}").into(),
        )
    })
}

fn calendar_from_row(row: &Row<'_>) -> rusqlite::Result<CalendarRow> {
    let backend: Option<String> = row.get("backend")?;
    let model: Option<String> = row.get("model")?;
    let provenance = match (backend, model) {
        (Some(backend_label), Some(model)) => Some(CalendarProvenance {
            generation_id: row.get("generation_id")?,
            backend_label,
            model,
            prompt_version: row
                .get::<_, Option<u32>>("prompt_version")?
                .unwrap_or_default(),
        }),
        _ => None,
    };
    Ok(CalendarRow {
        id: row.get("id")?,
        course_id: row.get("course_id")?,
        origin: get_value(row, "origin")?,
        state: get_value(row, "state")?,
        calendar: from_json(row, "calendar_json")?,
        dates: from_json(row, "evidence_json")?,
        checks: from_json(row, "checks_json")?,
        manifest: from_json(row, "manifest_json")?,
        fingerprint: row.get("fingerprint")?,
        provenance,
        created_at: get_value(row, "created_at")?,
        decided_at: get_opt_value(row, "decided_at")?,
    })
}

/// The span a calendar mirrors into `user_term_start/end`: the first class, and the end of
/// exams, else the last segment's last day of classes.
fn mirrored_span(
    calendar: &CourseCalendar,
) -> (Option<chrono::NaiveDate>, Option<chrono::NaiveDate>) {
    let start = calendar.segments.first().map(|s| s.first_class);
    let end = calendar
        .exam_period
        .map(|span| span.end)
        .or_else(|| calendar.segments.last().and_then(|s| s.last_class));
    (start, end)
}

impl Store {
    /// Store a proposal (state `proposed`); a proposal from the same origin for the course is
    /// replaced. Returns its id.
    pub fn insert_calendar_proposal(&self, row: &NewCalendarRow, now: Timestamp) -> Result<i64> {
        self.atomic(|| {
            self.conn.execute(
                "DELETE FROM course_calendars
                 WHERE course_id = ?1 AND origin = ?2 AND state = 'proposed'",
                params![row.course_id, row.origin.as_str()],
            )?;
            self.insert_calendar_row(row, CalendarState::Proposed, now, None)
        })
    }

    fn insert_calendar_row(
        &self,
        row: &NewCalendarRow,
        state: CalendarState,
        now: Timestamp,
        decided_at: Option<Timestamp>,
    ) -> Result<i64> {
        let provenance = row.provenance.as_ref();
        self.conn.execute(
            "INSERT INTO course_calendars
                 (course_id, origin, state, calendar_json, evidence_json, checks_json,
                  manifest_json, fingerprint, generation_id, backend, model, prompt_version,
                  created_at, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                row.course_id,
                row.origin.as_str(),
                state.as_str(),
                json(&row.calendar),
                json(&row.dates),
                json(&row.checks),
                json(&row.manifest),
                row.fingerprint,
                provenance.and_then(|p| p.generation_id.as_deref()),
                provenance.map(|p| p.backend_label.as_str()),
                provenance.map(|p| p.model.as_str()),
                provenance.map(|p| p.prompt_version),
                ts_text(now),
                decided_at.map(ts_text),
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// One row by id, in any state.
    pub fn calendar_row(&self, id: i64) -> Result<Option<CalendarRow>> {
        self.query_opt(
            &format!("SELECT {CALENDAR_COLUMNS} FROM course_calendars WHERE id = ?1"),
            [id],
            calendar_from_row,
        )
    }

    /// The course's pending proposals, newest first.
    pub fn calendar_proposals(&self, course_id: &str) -> Result<Vec<CalendarRow>> {
        self.query_list(
            &format!(
                "SELECT {CALENDAR_COLUMNS} FROM course_calendars
                 WHERE course_id = ?1 AND state = 'proposed'
                 ORDER BY created_at DESC, id DESC"
            ),
            [course_id],
            calendar_from_row,
        )
    }

    /// The course's calendar in force.
    pub fn accepted_calendar(&self, course_id: &str) -> Result<Option<CalendarRow>> {
        self.query_opt(
            &format!(
                "SELECT {CALENDAR_COLUMNS} FROM course_calendars
                 WHERE course_id = ?1 AND state = 'accepted'"
            ),
            [course_id],
            calendar_from_row,
        )
    }

    /// Accept proposal `id`, optionally with the student's edits (a new calendar and its
    /// dates): the calendar in force becomes superseded, and the span is mirrored into
    /// `user_term_*`. `NotFound` unless `id` is a pending proposal.
    pub fn accept_calendar_proposal(
        &self,
        id: i64,
        edited: Option<(CourseCalendar, Vec<ProposedDate>)>,
        now: Timestamp,
    ) -> Result<CalendarRow> {
        self.atomic(|| {
            let row = self
                .calendar_row(id)?
                .filter(|row| row.state == CalendarState::Proposed)
                .ok_or_else(|| crate::Error::NotFound(format!("calendar proposal '{id}'")))?;
            self.supersede_calendar(&row.course_id, now)?;
            let now_text = ts_text(now);
            match edited {
                Some((calendar, dates)) => self.conn.execute(
                    "UPDATE course_calendars
                     SET state = 'accepted', decided_at = ?2, calendar_json = ?3,
                         evidence_json = ?4
                     WHERE id = ?1",
                    params![id, now_text, json(&calendar), json(&dates)],
                )?,
                None => self.conn.execute(
                    "UPDATE course_calendars SET state = 'accepted', decided_at = ?2 WHERE id = ?1",
                    params![id, now_text],
                )?,
            };
            let accepted = self
                .calendar_row(id)?
                .ok_or_else(|| crate::Error::NotFound(format!("calendar proposal '{id}'")))?;
            self.mirror_calendar(&accepted.course_id, Some(&accepted.calendar))?;
            self.prune_superseded(&accepted.course_id)?;
            Ok(accepted)
        })
    }

    /// The student's own dates (the dates form): `Some` puts a `user` calendar in force, `None`
    /// clears the calendar in force and `user_term_*` ("Undo"). Returns the new row.
    pub fn set_student_calendar(
        &self,
        course_id: &str,
        calendar: Option<&CourseCalendar>,
        now: Timestamp,
    ) -> Result<Option<CalendarRow>> {
        self.atomic(|| {
            self.supersede_calendar(course_id, now)?;
            self.mirror_calendar(course_id, calendar)?;
            let Some(calendar) = calendar else {
                return Ok(None);
            };
            let id = self.insert_calendar_row(
                &NewCalendarRow {
                    course_id: course_id.to_string(),
                    origin: CalendarOrigin::User,
                    calendar: calendar.clone(),
                    dates: Vec::new(),
                    checks: CalendarChecks {
                        passing: true,
                        ..CalendarChecks::default()
                    },
                    manifest: Vec::new(),
                    fingerprint: "user".to_string(),
                    provenance: None,
                },
                CalendarState::Accepted,
                now,
                Some(now),
            )?;
            self.prune_superseded(course_id)?;
            self.calendar_row(id)
        })
    }

    /// Dismiss proposal `id` (kept 30 days, so its materials aren't proposed again).
    pub fn dismiss_calendar_proposal(&self, id: i64, now: Timestamp) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE course_calendars SET state = 'dismissed', decided_at = ?2
             WHERE id = ?1 AND state = 'proposed'",
            params![id, ts_text(now)],
        )?;
        expect_changed(changed, "calendar proposal", &id.to_string())
    }

    /// Whether the student dismissed a proposal of `origin` made from these materials
    /// (`fingerprint`) in the last 30 days.
    pub fn calendar_fingerprint_dismissed(
        &self,
        course_id: &str,
        origin: CalendarOrigin,
        fingerprint: &str,
    ) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM course_calendars
             WHERE course_id = ?1 AND origin = ?2 AND fingerprint = ?3 AND state = 'dismissed'",
            params![course_id, origin.as_str(), fingerprint],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// Delete dismissed rows older than 30 days.
    pub fn prune_dismissed_calendars(&self, now: Timestamp) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM course_calendars WHERE state = 'dismissed' AND decided_at < ?1",
            [ts_text(now - Duration::days(KEEP_DISMISSED_DAYS))],
        )?)
    }

    /// The calendar in force becomes superseded.
    fn supersede_calendar(&self, course_id: &str, now: Timestamp) -> Result<()> {
        self.conn.execute(
            "UPDATE course_calendars SET state = 'superseded', decided_at = ?2
             WHERE course_id = ?1 AND state = 'accepted'",
            params![course_id, ts_text(now)],
        )?;
        Ok(())
    }

    /// Mirror `calendar`'s span into `user_term_*` (`None` clears them).
    fn mirror_calendar(&self, course_id: &str, calendar: Option<&CourseCalendar>) -> Result<()> {
        let (start, end) = calendar.map(mirrored_span).unwrap_or((None, None));
        let changed = self.conn.execute(
            "UPDATE courses SET user_term_start = ?2, user_term_end = ?3 WHERE id = ?1",
            params![course_id, opt_date_text(start), opt_date_text(end)],
        )?;
        expect_changed(changed, "course", course_id)
    }

    /// Keep the latest 3 superseded rows of the course.
    fn prune_superseded(&self, course_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM course_calendars
             WHERE course_id = ?1 AND state = 'superseded' AND id NOT IN (
                 SELECT id FROM course_calendars
                 WHERE course_id = ?1 AND state = 'superseded'
                 ORDER BY decided_at DESC, id DESC LIMIT ?2)",
            params![course_id, KEEP_SUPERSEDED as i64],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate, Utc};
    use serde_json::json;

    use super::*;
    use crate::calendar::legacy_calendar;
    use crate::model::{CourseUpsert, SourceKind, SourceRecord};
    use crate::term::DateSpan;

    const COURSE: &str = "canvas:lms.example.edu/course/101";

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("{text}T12:00:00Z"))
            .unwrap()
            .with_timezone(&Utc)
    }

    fn demo_store() -> Store {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_source(&SourceRecord {
                id: "canvas:lms.example.edu".into(),
                kind: SourceKind::Canvas,
                label: "Demo LMS".into(),
                config: json!({}),
                last_synced_at: None,
                last_error: None,
                last_error_kind: None,
            })
            .unwrap();
        store
            .upsert_course(&CourseUpsert {
                id: COURSE.into(),
                source_id: "canvas:lms.example.edu".into(),
                external_id: "101".into(),
                code: Some("DEMO101".into()),
                name: "Intro to Demo Studies".into(),
                term_start: None,
                term_end: None,
                url: None,
                syllabus_text: None,
                lms: Default::default(),
            })
            .unwrap();
        store
    }

    fn proposal(origin: CalendarOrigin, first_class: &str, fingerprint: &str) -> NewCalendarRow {
        let mut calendar = legacy_calendar(date(first_class), Some(date("2026-12-08")));
        calendar.exam_period = Some(DateSpan {
            start: date("2026-12-10"),
            end: date("2026-12-21"),
        });
        NewCalendarRow {
            course_id: COURSE.into(),
            origin,
            calendar,
            dates: Vec::new(),
            checks: CalendarChecks {
                passing: true,
                sharing_reminder: origin == CalendarOrigin::Ai,
                ..CalendarChecks::default()
            },
            manifest: vec![ManifestEntry {
                material_id: "m1".into(),
                content_hash: Some("h1".into()),
                chunk_ords: vec![0, 1],
            }],
            fingerprint: fingerprint.into(),
            provenance: (origin == CalendarOrigin::Ai).then(|| CalendarProvenance {
                generation_id: None,
                backend_label: "ChatGPT plan (through OpenAI Codex)".into(),
                model: "gpt-6-luna".into(),
                prompt_version: 1,
            }),
        }
    }

    fn user_term(store: &Store) -> (Option<NaiveDate>, Option<NaiveDate>) {
        let data = store.course_term_data(COURSE).unwrap().unwrap();
        (data.user_term_start, data.user_term_end)
    }

    #[test]
    fn a_newer_proposal_replaces_the_same_readers_older_one() {
        let store = demo_store();
        let scan = store
            .insert_calendar_proposal(
                &proposal(CalendarOrigin::Scan, "2026-09-08", "f1"),
                at("2026-09-20"),
            )
            .unwrap();
        let ai = store
            .insert_calendar_proposal(
                &proposal(CalendarOrigin::Ai, "2026-09-09", "f1"),
                at("2026-09-21"),
            )
            .unwrap();
        let rescan = store
            .insert_calendar_proposal(
                &proposal(CalendarOrigin::Scan, "2026-09-10", "f2"),
                at("2026-09-22"),
            )
            .unwrap();
        let ids: Vec<i64> = store
            .calendar_proposals(COURSE)
            .unwrap()
            .iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, [rescan, ai]);
        assert!(store.calendar_row(scan).unwrap().is_none());
        let ai = store.calendar_row(ai).unwrap().unwrap();
        assert_eq!(ai.state, CalendarState::Proposed);
        assert_eq!(ai.provenance.unwrap().model, "gpt-6-luna");
        assert!(ai.checks.sharing_reminder);
        assert_eq!(ai.manifest[0].chunk_ords, [0, 1]);
        assert!(store.accepted_calendar(COURSE).unwrap().is_none());
    }

    #[test]
    fn accepting_puts_one_calendar_in_force_and_mirrors_its_span() {
        let store = demo_store();
        let first = store
            .insert_calendar_proposal(
                &proposal(CalendarOrigin::Scan, "2026-09-08", "f1"),
                at("2026-09-20"),
            )
            .unwrap();
        let second = store
            .insert_calendar_proposal(
                &proposal(CalendarOrigin::Ai, "2026-09-09", "f1"),
                at("2026-09-20"),
            )
            .unwrap();
        let accepted = store
            .accept_calendar_proposal(first, None, at("2026-09-21"))
            .unwrap();
        assert_eq!(accepted.state, CalendarState::Accepted);
        assert_eq!(accepted.decided_at, Some(at("2026-09-21")));
        // v3 readers see the first class and the end of exams.
        assert_eq!(
            user_term(&store),
            (Some(date("2026-09-08")), Some(date("2026-12-21")))
        );
        // The other proposal stays until the student decides; accepting it (edited) replaces
        // the calendar in force.
        let mut edited = store.calendar_row(second).unwrap().unwrap().calendar;
        edited.exam_period = None;
        let second = store
            .accept_calendar_proposal(second, Some((edited, Vec::new())), at("2026-09-22"))
            .unwrap();
        assert_eq!(
            store.accepted_calendar(COURSE).unwrap().unwrap().id,
            second.id
        );
        assert_eq!(second.calendar.exam_period, None);
        assert_eq!(
            user_term(&store),
            (Some(date("2026-09-09")), Some(date("2026-12-08")))
        );
        assert_eq!(
            store.calendar_row(first).unwrap().unwrap().state,
            CalendarState::Superseded
        );
        // Only pending proposals can be accepted.
        assert!(matches!(
            store.accept_calendar_proposal(first, None, at("2026-09-23")),
            Err(crate::Error::NotFound(_))
        ));
    }

    #[test]
    fn the_dates_form_and_undo() {
        let store = demo_store();
        let calendar = legacy_calendar(date("2026-09-08"), Some(date("2026-12-08")));
        let row = store
            .set_student_calendar(COURSE, Some(&calendar), at("2026-09-21"))
            .unwrap()
            .unwrap();
        assert_eq!(
            (row.origin, row.state),
            (CalendarOrigin::User, CalendarState::Accepted)
        );
        assert_eq!(
            user_term(&store),
            (Some(date("2026-09-08")), Some(date("2026-12-08")))
        );
        assert!(
            store
                .set_student_calendar(COURSE, None, at("2026-09-22"))
                .unwrap()
                .is_none()
        );
        assert!(store.accepted_calendar(COURSE).unwrap().is_none());
        assert_eq!(user_term(&store), (None, None));
        assert_eq!(
            store.calendar_row(row.id).unwrap().unwrap().state,
            CalendarState::Superseded
        );
    }

    #[test]
    fn three_superseded_calendars_are_kept_for_undo() {
        let store = demo_store();
        let mut ids = Vec::new();
        for day in 10..16 {
            let calendar = legacy_calendar(date(&format!("2026-09-{day}")), None);
            let row = store
                .set_student_calendar(COURSE, Some(&calendar), at(&format!("2026-09-{day}")))
                .unwrap()
                .unwrap();
            ids.push(row.id);
        }
        let kept: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| store.calendar_row(*id).unwrap().is_some())
            .collect();
        // The one in force and the latest 3 before it.
        assert_eq!(kept, ids[2..]);
    }

    #[test]
    fn dismissed_materials_are_remembered_for_thirty_days() {
        let store = demo_store();
        let id = store
            .insert_calendar_proposal(
                &proposal(CalendarOrigin::Scan, "2026-09-08", "f1"),
                at("2026-09-20"),
            )
            .unwrap();
        store
            .dismiss_calendar_proposal(id, at("2026-09-21"))
            .unwrap();
        assert!(store.calendar_proposals(COURSE).unwrap().is_empty());
        assert!(
            store
                .calendar_fingerprint_dismissed(COURSE, CalendarOrigin::Scan, "f1")
                .unwrap()
        );
        assert!(
            !store
                .calendar_fingerprint_dismissed(COURSE, CalendarOrigin::Scan, "f2")
                .unwrap()
        );
        assert!(matches!(
            store.dismiss_calendar_proposal(id, at("2026-09-21")),
            Err(crate::Error::NotFound(_))
        ));
        assert_eq!(
            store.prune_dismissed_calendars(at("2026-10-20")).unwrap(),
            0
        );
        assert_eq!(
            store.prune_dismissed_calendars(at("2026-10-22")).unwrap(),
            1
        );
        assert!(
            !store
                .calendar_fingerprint_dismissed(COURSE, CalendarOrigin::Scan, "f1")
                .unwrap()
        );
    }

    #[test]
    fn a_legacy_row_from_the_v4_migration_reads_back() {
        let store = demo_store();
        store
            .conn
            .execute(
                "INSERT INTO course_calendars
                     (course_id, origin, state, calendar_json, evidence_json, checks_json,
                      manifest_json, fingerprint, created_at, decided_at)
                 VALUES (?1, 'legacy', 'accepted', ?2, '[]', '{}', '[]', 'legacy', ?3, ?3)",
                params![
                    COURSE,
                    json(&legacy_calendar(date("2026-09-08"), None)),
                    ts_text(at("2026-09-01"))
                ],
            )
            .unwrap();
        let row = store.accepted_calendar(COURSE).unwrap().unwrap();
        assert_eq!(row.origin, CalendarOrigin::Legacy);
        assert_eq!(row.checks, CalendarChecks::default());
        assert!(row.provenance.is_none() && row.dates.is_empty());
    }
}
