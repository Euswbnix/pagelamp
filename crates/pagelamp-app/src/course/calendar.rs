//! The course calendar in the facade (docs/design/v0.3-course-calendar.md §4, §7): the calendar
//! in force, proposals, candidate materials and reading the syllabus with AI.
//!
//! This build has the contract and the parts that need no schema v4 and no model:
//! - `course_calendar` and `calendar_candidates` compute the candidates (§7.1) and why AI
//!   reading can't run; there are no stored calendars or proposals yet;
//! - `syllabus_reading_offers` lists the courses "Read syllabi for N courses" would read;
//! - AI reading is refused as `blocked`: the course's own reason first, else
//!   `backend_disabled_in_this_build`; a batch reports that per course;
//! - storing proposals and calendars (accept, dismiss, the scan, the student's sources and
//!   dates form) arrives with schema v4 and says "not available in this build yet".

use pagelamp_core::ai::BlockReason;
use pagelamp_core::calendar::candidates::{
    CalendarCandidate, CandidateSignals, ScoredCandidate, course_candidates,
};
use pagelamp_core::calendar::proposal::{AcceptedCalendar, CalendarProposal};
use pagelamp_core::model::{AiMaterialsState, Course, CourseGroup};
use pagelamp_core::store::Store;
use pagelamp_core::term::CalendarStatus;
use pagelamp_core::views::{self, AsOf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::dates::CourseDatesInput;
use crate::ai::GenEvent;
use crate::{App, AppError, AppErrorKind, Result, SourceSyncResult, SyncEvent};

/// `SyllabusOffer::reason_code` for a course without a calendar in force.
pub const OFFER_NO_CALENDAR: &str = "no_calendar";

/// Everything the Timeline tab shows about a course's calendar.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CourseCalendarView {
    pub course_id: String,
    /// The calendar in force, if the student accepted or typed one.
    pub accepted: Option<AcceptedCalendar>,
    /// Pending proposals, at most one per origin.
    pub proposals: Vec<CalendarProposal>,
    pub status: CalendarStatus,
    /// The materials a reading would use, and why (read first, then by score).
    pub candidates: Vec<CalendarCandidate>,
    /// Why AI reading can't run for this course now.
    pub blocked: Option<BlockReason>,
}

/// A course "Read syllabi for N courses" would read (the facade decides which).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SyllabusOffer {
    pub course_id: String,
    /// Why it is offered: `no_calendar`.
    pub reason_code: String,
    /// How many candidate materials it has.
    pub candidates: u32,
    /// Some candidate has text to read.
    pub has_text: bool,
}

/// Options of an AI reading run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadCalendarOptions {
    /// The student chose to go over the monthly budget for this run.
    pub override_budget: bool,
}

/// How one course of "Read syllabi for N courses" ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarRunOutcome {
    pub course_id: String,
    /// The proposal the run made; `None` when it was blocked, failed, stopped or found no dates.
    pub proposal_id: Option<i64>,
    /// The proposal has no conflicts and isn't low quality (`accept_passing_proposals`).
    pub passing: bool,
    /// The gate stopped this course (each course is gated on its own).
    pub blocked: Option<BlockReason>,
    /// The run failed, or was stopped (`cancelled`).
    pub error: Option<AppErrorKind>,
}

/// Progress of `read_course_calendars`: one course after another, each with its `GenEvent`s.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CalendarBatchEvent {
    CourseStarted {
        course_id: String,
        /// 0-based.
        index: u32,
        total: u32,
    },
    Gen {
        course_id: String,
        event: GenEvent,
    },
    CourseFinished {
        outcome: CalendarRunOutcome,
    },
}

impl App {
    /// The calendar in force, pending proposals, the candidates and why AI reading can't run.
    /// Hidden courses are addressable.
    pub fn course_calendar(&self, course: &str) -> Result<CourseCalendarView> {
        let store = self.read_store()?;
        let course = store.resolve_course_with(course, true)?;
        let at = AsOf::now_local();
        let status = views::course_timeline(&store, &course, at)?.calendar;
        let candidates = candidates_of(&store, &course, at)?;
        Ok(CourseCalendarView {
            course_id: course.id.clone(),
            accepted: None,
            proposals: Vec::new(),
            status,
            blocked: Some(reading_block(&course)),
            candidates: candidates.into_iter().map(|c| c.candidate).collect(),
        })
    }

    /// The materials a syllabus reading would use, why, and whether each has text (§7.1).
    pub fn calendar_candidates(&self, course: &str) -> Result<Vec<CalendarCandidate>> {
        let store = self.read_store()?;
        let course = store.resolve_course_with(course, true)?;
        Ok(candidates_of(&store, &course, AsOf::now_local())?
            .into_iter()
            .map(|c| c.candidate)
            .collect())
    }

    /// The student's add (`include`) and remove (`exclude`) of candidate materials.
    pub fn set_calendar_sources(
        &self,
        _course: &str,
        _include: Vec<String>,
        _exclude: Vec<String>,
    ) -> Result<Vec<CalendarCandidate>> {
        Err(not_yet("Choosing the materials to read"))
    }

    /// Download the chosen files only (it counts as viewing them in Canvas; D46).
    pub async fn download_material_files(
        &self,
        _course: &str,
        _material_ids: Vec<String>,
        _on_event: impl Fn(SyncEvent) + Send + Sync,
    ) -> Result<SourceSyncResult> {
        Err(not_yet("Downloading chosen files"))
    }

    /// The deterministic syllabus scan (no model); `None` when there is nothing new to propose.
    pub fn scan_course_calendar(&self, _course: &str) -> Result<Option<CalendarProposal>> {
        Err(not_yet("Scanning the syllabus"))
    }

    /// The course dates form (`None` clears the student's dates).
    pub fn set_course_dates(
        &self,
        _course: &str,
        _dates: Option<CourseDatesInput>,
    ) -> Result<CourseCalendarView> {
        Err(not_yet("The course dates form"))
    }

    /// Accept a proposal, optionally with the student's edits and conflict choices.
    pub fn accept_calendar_proposal(
        &self,
        proposal_id: i64,
        _edits: Option<CourseDatesInput>,
    ) -> Result<CourseCalendarView> {
        Err(no_proposal(proposal_id))
    }

    /// Accept several proposals that have no conflicts and aren't low quality.
    pub fn accept_passing_proposals(
        &self,
        proposal_ids: Vec<i64>,
    ) -> Result<Vec<CourseCalendarView>> {
        match proposal_ids.first() {
            Some(&id) => Err(no_proposal(id)),
            None => Ok(Vec::new()),
        }
    }

    pub fn dismiss_calendar_proposal(&self, proposal_id: i64) -> Result<()> {
        Err(no_proposal(proposal_id))
    }

    /// The courses "Read syllabi for N courses" would read: current, upcoming or unknown
    /// courses, not hidden, without a calendar in force, whose materials AI may read and that
    /// have a candidate to read. Model setup doesn't matter here (the button then says "Set up
    /// AI to read syllabi").
    pub fn syllabus_reading_offers(&self) -> Result<Vec<SyllabusOffer>> {
        let store = self.read_store()?;
        let at = AsOf::now_local();
        let mut offers = Vec::new();
        for summary in views::list_courses(&store, false, at)? {
            let course = &summary.course;
            let without_calendar = matches!(
                summary.timeline.calendar,
                CalendarStatus::NoCalendar | CalendarStatus::Proposed
            );
            if summary.lifecycle.group == CourseGroup::Past
                || !without_calendar
                || !course.ai_materials().is_readable()
            {
                continue;
            }
            let candidates = candidates_of(&store, course, at)?;
            if !candidates.iter().any(|c| c.candidate.included) {
                continue;
            }
            offers.push(SyllabusOffer {
                course_id: course.id.clone(),
                reason_code: OFFER_NO_CALENDAR.to_string(),
                candidates: u32::try_from(candidates.len()).unwrap_or(u32::MAX),
                has_text: candidates.iter().any(|c| c.candidate.has_text),
            });
        }
        Ok(offers)
    }

    /// "Read the syllabus with AI" (§7.3): gated like every model run; makes a proposal.
    pub async fn read_course_calendar(
        &self,
        course: &str,
        _generation_id: &str,
        _options: ReadCalendarOptions,
        _on_event: impl Fn(GenEvent) + Send + Sync,
    ) -> Result<CalendarProposal> {
        let course = self.read_store()?.resolve_course_with(course, true)?;
        let reason = reading_block(&course);
        Err(AppError::blocked(reason, block_message(reason)))
    }

    /// "Read syllabi for N courses": one course after another, each gated on its own.
    pub async fn read_course_calendars(
        &self,
        courses: Vec<String>,
        _batch_id: &str,
        _options: ReadCalendarOptions,
        on_event: impl Fn(CalendarBatchEvent) + Send + Sync,
    ) -> Result<Vec<CalendarRunOutcome>> {
        let store = self.read_store()?;
        let resolved = courses
            .iter()
            .map(|course| store.resolve_course_with(course, true))
            .collect::<std::result::Result<Vec<Course>, _>>()?;
        let total = u32::try_from(resolved.len()).unwrap_or(u32::MAX);
        let mut outcomes = Vec::with_capacity(resolved.len());
        for (index, course) in resolved.iter().enumerate() {
            on_event(CalendarBatchEvent::CourseStarted {
                course_id: course.id.clone(),
                index: u32::try_from(index).unwrap_or(u32::MAX),
                total,
            });
            let outcome = CalendarRunOutcome {
                course_id: course.id.clone(),
                proposal_id: None,
                passing: false,
                blocked: Some(reading_block(course)),
                error: None,
            };
            on_event(CalendarBatchEvent::CourseFinished {
                outcome: outcome.clone(),
            });
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    /// Stop a running generation or batch by its id; it ends with `cancelled`. Unknown or
    /// finished ids are fine (nothing to stop).
    pub fn cancel_generation(&self, _generation_id: &str) -> Result<()> {
        Ok(())
    }
}

/// The course's candidates with the signals stored so far (none before schema v4).
fn candidates_of(store: &Store, course: &Course, at: AsOf) -> Result<Vec<ScoredCandidate>> {
    Ok(course_candidates(
        store,
        course,
        at,
        &CandidateSignals::default(),
    )?)
}

/// Why AI reading can't run for `course` in this build: the course's own reason first.
fn reading_block(course: &Course) -> BlockReason {
    if course.hidden {
        return BlockReason::CourseHidden;
    }
    match course.ai_materials() {
        AiMaterialsState::WithheldByPolicy => BlockReason::CoursePolicyProhibited,
        AiMaterialsState::TurnedOff => BlockReason::CourseAiTurnedOff,
        AiMaterialsState::Readable => BlockReason::BackendDisabledInThisBuild,
    }
}

fn block_message(reason: BlockReason) -> &'static str {
    match reason {
        BlockReason::CourseHidden => "This course is hidden: show it to read its syllabus.",
        BlockReason::CoursePolicyProhibited => {
            "This course's AI policy is \"prohibited\", so its materials aren't sent to a model."
        }
        BlockReason::CourseAiTurnedOff => "AI access is turned off for this course.",
        _ => "Reading the syllabus with AI isn't available in this build yet.",
    }
}

fn no_proposal(id: i64) -> AppError {
    AppError::new(
        AppErrorKind::NotFound,
        format!("There is no calendar proposal {id}."),
    )
}

fn not_yet(what: &str) -> AppError {
    AppError::new(
        AppErrorKind::Internal,
        format!("{what} is not available in this build yet."),
    )
}
