//! Study plans PageLamp writes itself (model-access design §5.1): the model proposes tasks from
//! the courses' structure only; PageLamp's scheduler dates them within the student's capacity.
//!
//! - `generate_study_plan`: registered as a run from the start (`cancel_generation` stops it,
//!   `activity()` lists it), gated like every run (the plan context is structure only, for every
//!   visible course whatever its AI state), then scheduled. Tasks that would produce graded work
//!   are left out whatever the model said. The result is a draft (a `draft` generation row).
//! - `accept_study_plan`: the student keeps the draft; it is saved as the latest plan with
//!   `origin = pagelamp`.
//! - `set_study_plan_item_done` ticks an item (the weekly progress reminder counts it).

use std::collections::HashSet;

use chrono::{SubsecRound, Utc};
use pagelamp_core::ai::{AiFeature, BlockReason};
use pagelamp_core::ai_gate::{ContextSummary, GateError, PlanScope, StudentNote, plan_context};
use pagelamp_core::model::{PlanOrigin, StoredStudyPlan, StudyPlan};
use pagelamp_core::planner::{
    self, MAX_HORIZON_DAYS, PlanTasks, PlannerInput, UnscheduledTask, without_graded_work,
};
use pagelamp_core::reminders::DayOfWeek;
use pagelamp_core::store::{GenerationRecord, GenerationStatus};
use pagelamp_core::term::AiLabel;
use pagelamp_core::views::AsOf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::estimate::DEFAULT_PLAN_DAYS;
use super::prompts::PROMPT_VERSION;
use super::run::RunRequest;
use super::settings::backend_key;
use super::{GenEvent, GenStage, GenerationMeta, feature_choice, request_shape_with_note};
use crate::{App, AppError, AppErrorKind, Result};

/// Study hours per week when the request doesn't say.
pub const DEFAULT_HOURS_PER_WEEK: u32 = 10;
/// The most study hours per week a request may ask for.
pub const MAX_HOURS_PER_WEEK: u32 = 80;

/// What to plan (design §5.1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct StudyPlanRequest {
    /// Days the plan covers, from today: 1–56; default 14.
    pub horizon_days: Option<u32>,
    /// Study hours per week: 1–80; default 10. A day holds at most 4 hours.
    pub hours_per_week: Option<u32>,
    /// Weekdays without study.
    pub days_off: Vec<DayOfWeek>,
    /// Course ids or codes; empty: every visible, active course.
    pub courses: Vec<String>,
    /// The student's own note ("focus on the midterm"), cut to 500 characters, sent as data.
    pub note: Option<String>,
    /// The student chose to go over the monthly budget for this run.
    pub override_budget: bool,
}

/// A draft plan: dates set by PageLamp's scheduler, waiting for `accept_study_plan`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GeneratedStudyPlan {
    pub meta: GenerationMeta,
    pub plan: StudyPlan,
    /// Tasks the scheduler couldn't place, and why.
    pub unscheduled: Vec<UnscheduledTask>,
    pub warnings: Vec<PlanWarning>,
}

/// Something PageLamp changed in the model's proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanWarning {
    pub code: PlanWarningCode,
    pub count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanWarningCode {
    /// Tasks that would produce graded work (answers, solutions, write-ups) were left out.
    GradedWorkLeftOut,
    /// Material ids the model made up were dropped.
    UnknownMaterialsDropped,
}

impl App {
    /// Write a study plan draft (see the module docs). `Invalid` for a request out of range or
    /// with no active course to plan; `Blocked` for what stops a run (no model chosen, the
    /// disclosure, the budget…); `Busy` while a run with `generation_id` goes on.
    pub async fn generate_study_plan(
        &self,
        request: StudyPlanRequest,
        generation_id: &str,
        on_event: impl Fn(GenEvent) + Send + Sync,
    ) -> Result<GeneratedStudyPlan> {
        let result = self.write_plan(&request, generation_id, &on_event).await;
        on_event(GenEvent::Finished { ok: result.is_ok() });
        result
    }

    /// Keep draft `generation_id`: it becomes the latest plan (`origin = pagelamp`).
    /// `NotFound` for an unknown id; `Invalid` for a draft already accepted.
    pub fn accept_study_plan(&self, generation_id: &str) -> Result<StoredStudyPlan> {
        let store = self.write_store()?;
        let record = store
            .generation(generation_id)?
            .filter(|record| record.feature == AiFeature::StudyPlan)
            .ok_or_else(|| {
                AppError::new(
                    AppErrorKind::NotFound,
                    format!("There is no study plan draft {generation_id}."),
                )
            })?;
        if record.status != GenerationStatus::Draft {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                "This study plan draft was already accepted.",
            ));
        }
        let draft: GeneratedStudyPlan = record
            .output_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .ok_or_else(|| {
                AppError::new(
                    AppErrorKind::Internal,
                    "The study plan draft can't be read.",
                )
            })?;
        let label = AiLabel {
            backend_label: draft.meta.backend_label.clone(),
            model: draft.meta.model.clone(),
            created_at: draft.meta.created_at,
            on_device: draft.meta.on_device,
        };
        Ok(store.in_transaction(|store| {
            let stored = store.save_study_plan_as(
                &draft.plan,
                PlanOrigin::PageLamp,
                Some(generation_id),
                Some(&label),
            )?;
            store.record_generation(&GenerationRecord {
                status: GenerationStatus::Accepted,
                ..record
            })?;
            Ok(stored)
        })?)
    }

    /// Tick item `item_index` of plan `plan_id` done or not; the plan as readers see it (the
    /// index counts those items). `NotFound` for an unknown plan or item.
    pub fn set_study_plan_item_done(
        &self,
        plan_id: i64,
        item_index: u32,
        done: bool,
    ) -> Result<StoredStudyPlan> {
        Ok(self
            .write_store()?
            .set_study_plan_item_done(plan_id, item_index, done)?)
    }

    async fn write_plan(
        &self,
        request: &StudyPlanRequest,
        generation_id: &str,
        on_event: &(dyn Fn(GenEvent) + Send + Sync),
    ) -> Result<GeneratedStudyPlan> {
        let (horizon, hours) = checked(request)?;
        // Registered until the draft is stored.
        let (_run, cancel) = self.register_run(generation_id, None)?;
        on_event(GenEvent::Stage {
            stage: GenStage::BuildingContext,
        });
        let at = AsOf::now_local();
        let started = Utc::now().trunc_subsecs(0);
        // Everything read before the model call; the store isn't held across it.
        let (choice, context, prompt, output, max_output) = {
            let store = self.read_store()?;
            let Some(choice) = feature_choice(&store, AiFeature::StudyPlan)? else {
                return Err(blocked(BlockReason::NoModelChosen));
            };
            let (profile, _) = self.estimate_profile(&choice)?;
            let scope = PlanScope {
                courses: request.courses.clone(),
                horizon_days: horizon,
            };
            let context = match plan_context(&store, &scope, at) {
                Ok(context) => context,
                Err(GateError::Blocked(reason)) => return Err(blocked(reason)),
                Err(GateError::Store(err)) => return Err(err.into()),
            };
            if context.summary().courses.is_empty() {
                return Err(AppError::new(
                    AppErrorKind::Invalid,
                    "There is no active course to plan for.",
                ));
            }
            let note = request.note.as_deref().and_then(StudentNote::new);
            let (prompt, output, max_output) =
                request_shape_with_note(AiFeature::StudyPlan, &context, note.as_ref());
            if let Some(reason) = self.run_blocks(
                &store,
                &choice,
                &prompt,
                &output,
                max_output,
                request.override_budget,
            )? {
                return Err(blocked(reason));
            }
            let estimate = pagelamp_llm::estimate::estimate(
                &profile,
                &choice.model,
                &prompt,
                &output,
                choice.effort,
                max_output,
            );
            on_event(GenEvent::Context {
                summary: context.summary().clone(),
                input_tokens: Some(estimate.input_tokens),
            });
            (choice, context, prompt, output, max_output)
        };
        let run = self
            .run_model(
                RunRequest {
                    generation_id,
                    cancel: &cancel,
                    feature: AiFeature::StudyPlan,
                    choice: &choice,
                    prompt,
                    output,
                    max_output_tokens: max_output,
                },
                on_event,
            )
            .await;
        let summary = serde_json::to_string(&Summary {
            context: context.summary(),
            manifest: context.manifest(),
        })
        .ok();
        let record = |status, output_json, error_kind: Option<&str>| GenerationRecord {
            id: generation_id.to_string(),
            feature: AiFeature::StudyPlan,
            course_id: None,
            week: None,
            backend: backend_key(&choice.backend),
            model: choice.model.clone(),
            status,
            created_at: started,
            prompt_version: PROMPT_VERSION,
            output_json,
            summary_json: summary.clone(),
            error_kind: error_kind.map(str::to_string),
        };
        let run = match run {
            Ok(run) => run,
            Err(err) => {
                let (status, kind) = match err.kind {
                    AppErrorKind::Cancelled => (GenerationStatus::Cancelled, None),
                    _ => (
                        GenerationStatus::Failed,
                        err.model_error.map(|kind| kind.as_str()),
                    ),
                };
                self.write_store()?
                    .record_generation(&record(status, None, kind))?;
                return Err(err);
            }
        };
        on_event(GenEvent::Stage {
            stage: GenStage::Scheduling,
        });
        let Some(tasks) = run
            .json
            .clone()
            .and_then(|json| serde_json::from_value::<PlanTasks>(json).ok())
        else {
            let bad = pagelamp_core::ai::ModelErrorKind::BadOutput;
            self.write_store()?.record_generation(&record(
                GenerationStatus::Failed,
                None,
                Some(bad.as_str()),
            ))?;
            return Err(AppError {
                model_error: Some(bad),
                ..AppError::new(
                    AppErrorKind::Model,
                    "The model's answer wasn't a list of study tasks.",
                )
            });
        };
        let (tasks, graded) = without_graded_work(tasks.tasks);
        let known_materials: HashSet<String> = context
            .manifest()
            .materials
            .iter()
            .map(|entry| entry.material_id.clone())
            .collect();
        let known_courses: HashSet<String> = context
            .summary()
            .courses
            .iter()
            .map(|course| course.course_id.clone())
            .collect();
        let days_off: Vec<chrono::Weekday> =
            request.days_off.iter().map(|day| day.weekday()).collect();
        let scheduled = planner::schedule(
            &tasks,
            &PlannerInput {
                start: at.today,
                horizon_days: horizon,
                hours_per_week: hours,
                days_off: &days_off,
                known_material_ids: &known_materials,
                known_course_ids: &known_courses,
            },
        );
        let warnings = [
            (PlanWarningCode::GradedWorkLeftOut, graded),
            (
                PlanWarningCode::UnknownMaterialsDropped,
                scheduled.dropped_material_ids,
            ),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(code, count)| PlanWarning { code, count })
        .collect();
        let generated = GeneratedStudyPlan {
            meta: GenerationMeta {
                generation_id: generation_id.to_string(),
                feature: AiFeature::StudyPlan,
                backend_label: run.backend_label,
                model: run.model,
                on_device: run.on_device,
                created_at: started,
                usage: run.cost.usage,
                est_cost_micro_usd: run.cost.micro_usd,
                estimated: run.cost.estimated,
                context: context.summary().clone(),
                prompt_version: PROMPT_VERSION,
            },
            plan: scheduled.plan,
            unscheduled: scheduled.unscheduled,
            warnings,
        };
        self.write_store()?.record_generation(&record(
            GenerationStatus::Draft,
            serde_json::to_string(&generated).ok(),
            None,
        ))?;
        Ok(generated)
    }
}

/// What a generation row keeps about what was sent (no text).
#[derive(Serialize)]
struct Summary<'a> {
    context: &'a ContextSummary,
    manifest: &'a pagelamp_core::ai_gate::ContextManifest,
}

/// The request's horizon and weekly hours, or `Invalid`.
fn checked(request: &StudyPlanRequest) -> Result<(u32, u32)> {
    let horizon = request.horizon_days.unwrap_or(DEFAULT_PLAN_DAYS);
    let hours = request.hours_per_week.unwrap_or(DEFAULT_HOURS_PER_WEEK);
    let study_days = 7 - request.days_off.iter().collect::<HashSet<_>>().len().min(7);
    if !(1..=MAX_HORIZON_DAYS).contains(&horizon) {
        return Err(AppError::new(
            AppErrorKind::Invalid,
            format!("A plan covers 1 to {MAX_HORIZON_DAYS} days."),
        ));
    }
    if !(1..=MAX_HOURS_PER_WEEK).contains(&hours) {
        return Err(AppError::new(
            AppErrorKind::Invalid,
            format!("Plan 1 to {MAX_HOURS_PER_WEEK} study hours a week."),
        ));
    }
    if study_days == 0 {
        return Err(AppError::new(
            AppErrorKind::Invalid,
            "Leave at least one day of the week for study.",
        ));
    }
    Ok((horizon, hours))
}

fn blocked(reason: BlockReason) -> AppError {
    AppError::blocked(
        reason,
        match reason {
            BlockReason::NoModelChosen => "Choose a model for study plans first.",
            _ => "PageLamp can't start this study plan run.",
        },
    )
}
