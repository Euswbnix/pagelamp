//! "≈ $x" before Generate, and everything that would stop the run (design §3.5, §4.1): the
//! gate's own blocks (question (b) on a cloud backend among them), the disclosure, an unpriced
//! model, and the monthly budget. Local and cheap: DB reads only, no network call.

use pagelamp_core::ai::{AiFeature, BlockReason, Destination};
use pagelamp_core::ai_gate::{
    ContextBudget, GateError, GatedContext, PlanScope, RenderedPrompt, assemble, calendar_context,
    note_context, plan_context, week_context,
};
use pagelamp_core::calendar::extraction::CalendarExtraction;
use pagelamp_core::planner::PlanTasks;
use pagelamp_core::store::Store;
use pagelamp_core::views::AsOf;
use pagelamp_llm::OutputSpec;
use pagelamp_llm::profile::ProviderProfile;

use super::settings::{self, backend_key};
use super::{BackendRef, CostEstimate, EstimateRequest, ModelChoice, prompts};
use crate::{App, AppError, AppErrorKind, Result};

/// Characters of material text an explanation may carry (design §4.2 starting value).
pub(crate) const EXPLANATION_CONTEXT_CHARS: usize = 200_000;
/// Output budgets per feature (tokens).
pub(crate) const EXPLANATION_MAX_OUTPUT: u32 = 6_000;
pub(crate) const PLAN_MAX_OUTPUT: u32 = 4_000;
pub(crate) const NOTE_MAX_OUTPUT: u32 = 1_000;
/// A syllabus reading (calendar design §7.3).
pub(crate) const CALENDAR_MAX_OUTPUT: u32 = 4_000;
/// Characters of material text a syllabus reading may carry, in the cloud and on this computer.
pub(crate) const CALENDAR_CONTEXT_CHARS: usize = 60_000;
pub(crate) const CALENDAR_LOCAL_CONTEXT_CHARS: usize = 24_000;
/// The default study-plan horizon (days).
pub(crate) const DEFAULT_PLAN_DAYS: u32 = 14;

impl App {
    pub(crate) fn estimate(&self, request: &EstimateRequest) -> Result<CostEstimate> {
        let feature = match request {
            EstimateRequest::StudyPlan { .. } => AiFeature::StudyPlan,
            EstimateRequest::WeeklyExplanation { .. } => AiFeature::WeeklyExplanation,
            EstimateRequest::WeeklyNote => AiFeature::WeeklyNote,
            EstimateRequest::CourseCalendar { courses } => {
                return self.calendar_estimate(courses);
            }
        };
        let store = self.read_store()?;
        let Some(choice) = feature_choice(&store, feature)? else {
            return Ok(blocked_estimate(BlockReason::NoModelChosen));
        };
        let (profile, destination) = self.estimate_profile(&choice)?;
        let codex = choice.backend == BackendRef::Codex;
        let at = AsOf::now_local();

        // The gate decides what may be sent at all, question (b) included.
        let context = match request {
            EstimateRequest::StudyPlan {
                horizon_days,
                courses,
            } => {
                let scope = PlanScope {
                    courses: courses.clone(),
                    horizon_days: horizon_days.unwrap_or(DEFAULT_PLAN_DAYS),
                };
                plan_context(&store, &scope, at)
            }
            EstimateRequest::WeeklyExplanation { course, week } => {
                let budget = ContextBudget {
                    max_chars: EXPLANATION_CONTEXT_CHARS,
                };
                week_context(&store, course, *week, at, destination, budget)
            }
            EstimateRequest::WeeklyNote | EstimateRequest::CourseCalendar { .. } => {
                note_context(&store, at)
            }
        };
        let context = match context {
            Ok(context) => context,
            Err(GateError::Blocked(reason)) => return Ok(blocked_estimate(reason)),
            Err(GateError::Store(err)) => return Err(err.into()),
        };
        let (prompt, output, max_output) = request_shape(feature, &context);
        let estimate = pagelamp_llm::estimate::estimate(
            &profile,
            &choice.model,
            &prompt,
            &output,
            choice.effort,
            max_output,
        );
        if codex {
            // Runs count against the plan: no price and no money budget, a weekly run cap.
            return Ok(CostEstimate {
                micro_usd_upper: None,
                input_tokens: estimate.input_tokens,
                max_output_tokens: estimate.max_output_tokens,
                reasoning_allowance: estimate.reasoning_allowance,
                repair_possible: estimate.repair_possible,
                price_known: false,
                would_block: self.codex_blocks(&store)?,
            });
        }
        let (upper, price_known) = priced(&estimate, &profile, destination);
        let would_block = self.other_blocks(
            &store,
            &choice.backend,
            &choice.model,
            &profile,
            destination,
            upper,
            price_known,
        )?;
        Ok(CostEstimate {
            micro_usd_upper: upper,
            input_tokens: estimate.input_tokens,
            max_output_tokens: estimate.max_output_tokens,
            reasoning_allowance: estimate.reasoning_allowance,
            repair_possible: estimate.repair_possible,
            price_known,
            would_block,
        })
    }

    /// "Read syllabi for N courses" (or one): each course's context gated on its own and
    /// counted; blocked only when no course can be read, or by what stops every run.
    fn calendar_estimate(&self, courses: &[String]) -> Result<CostEstimate> {
        if courses.is_empty() {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                "Choose at least one course.",
            ));
        }
        let store = self.read_store()?;
        let Some(choice) = feature_choice(&store, AiFeature::CourseCalendar)? else {
            return Ok(blocked_estimate(BlockReason::NoModelChosen));
        };
        let (profile, destination) = self.estimate_profile(&choice)?;
        let at = AsOf::now_local();
        let mut total: Option<pagelamp_llm::estimate::Estimate> = None;
        let mut first_block = None;
        for course in courses {
            let course = store.resolve_course_with(course, true)?;
            let signals = store.calendar_signals(&course.id)?;
            let budget = calendar_budget(destination);
            match calendar_context(&store, &course.id, at, destination, budget, &signals) {
                Ok(context) => {
                    let (prompt, output, max_output) =
                        request_shape(AiFeature::CourseCalendar, &context);
                    let one = pagelamp_llm::estimate::estimate(
                        &profile,
                        &choice.model,
                        &prompt,
                        &output,
                        choice.effort,
                        max_output,
                    );
                    total = Some(match total {
                        None => one,
                        Some(sum) => pagelamp_llm::estimate::Estimate {
                            input_tokens: sum.input_tokens + one.input_tokens,
                            max_output_tokens: sum.max_output_tokens + one.max_output_tokens,
                            reasoning_allowance: sum.reasoning_allowance + one.reasoning_allowance,
                            repair_possible: sum.repair_possible || one.repair_possible,
                            micro_usd_upper: sum
                                .micro_usd_upper
                                .zip(one.micro_usd_upper)
                                .map(|(a, b)| a + b),
                            price_known: sum.price_known && one.price_known,
                        },
                    });
                }
                Err(GateError::Blocked(reason)) => {
                    first_block.get_or_insert(reason);
                }
                Err(GateError::Store(err)) => return Err(err.into()),
            }
        }
        let Some(estimate) = total else {
            return Ok(blocked_estimate(
                first_block.unwrap_or(BlockReason::NoReadableMaterials),
            ));
        };
        let codex = choice.backend == BackendRef::Codex;
        let (upper, price_known) = priced(&estimate, &profile, destination);
        let would_block = if codex {
            self.codex_blocks(&store)?
        } else {
            self.other_blocks(
                &store,
                &choice.backend,
                &choice.model,
                &profile,
                destination,
                upper,
                price_known,
            )?
        };
        Ok(CostEstimate {
            micro_usd_upper: if codex { None } else { upper },
            input_tokens: estimate.input_tokens,
            max_output_tokens: estimate.max_output_tokens,
            reasoning_allowance: estimate.reasoning_allowance,
            repair_possible: estimate.repair_possible,
            price_known: !codex && price_known,
            would_block,
        })
    }

    /// The profile tokens are counted with, and where the model runs. The ChatGPT plan through
    /// Codex runs in OpenAI's cloud; its tokens are counted like OpenAI's Responses wire. No key
    /// needed: an estimate never reads the keychain.
    pub(crate) fn estimate_profile(
        &self,
        choice: &ModelChoice,
    ) -> Result<(ProviderProfile, Destination)> {
        if choice.backend == BackendRef::Codex {
            let profile = pagelamp_llm::profile::preset("openai")
                .expect("the openai preset")
                .clone();
            return Ok((profile, Destination::Cloud));
        }
        let profile = self.provider_profile(&choice.backend)?;
        let destination = estimate_destination(&profile, &choice.model);
        Ok((profile, destination))
    }

    /// Whether `choice`'s model runs on this computer, decided as a run decides it (a model
    /// server here is asked for its list). With `ask = false` nothing is asked: then only a
    /// server that runs every model here can confirm it (LM Studio); an Ollama model is unknown.
    pub async fn model_runs_here(&self, choice: &ModelChoice, ask: bool) -> Result<bool> {
        if ask {
            return Ok(self.run_destination(choice).await? == Destination::OnDevice);
        }
        let (profile, destination) = self.estimate_profile(choice)?;
        Ok(destination == Destination::OnDevice && profile.id != "ollama")
    }

    /// Where a run's model runs, decided by the run itself right before it builds its context
    /// (never an estimate's answer carried over). It fails closed: the ChatGPT plan, a provider
    /// off this computer, a server here that can't say (any but LM Studio and Ollama), and an
    /// Ollama model its daemon doesn't list as local (a cloud model, a model it doesn't list at
    /// all, or a list it can't give) all count as the cloud.
    pub(crate) async fn run_destination(&self, choice: &ModelChoice) -> Result<Destination> {
        if choice.backend == BackendRef::Codex {
            return Ok(Destination::Cloud);
        }
        let profile = self.provider_profile(&choice.backend)?;
        if !profile.on_device() {
            return Ok(Destination::Cloud);
        }
        Ok(match profile.id.as_str() {
            "lm_studio" => Destination::OnDevice,
            // The list comes from the daemon on this computer: nothing leaves it.
            "ollama" => match self.list_models(&choice.backend).await {
                Ok(models)
                    if models
                        .iter()
                        .any(|m| m.on_device && same_ollama_model(&m.id, &choice.model)) =>
                {
                    Destination::OnDevice
                }
                _ => Destination::Cloud,
            },
            _ => Destination::Cloud,
        })
    }

    /// What stops a run of `choice` with this prompt besides the gate: the disclosure, then the
    /// ChatGPT plan's weekly cap, or an unpriced model and the monthly budget (which the
    /// student may go past for this run with `override_budget`). `destination`: where the
    /// model runs (a run's own `run_destination`; else the estimate's).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_blocks(
        &self,
        store: &Store,
        choice: &ModelChoice,
        destination: Destination,
        prompt: &RenderedPrompt,
        output: &OutputSpec,
        max_output: u32,
        override_budget: bool,
    ) -> Result<Option<BlockReason>> {
        if choice.backend == BackendRef::Codex {
            return self.codex_blocks(store);
        }
        let (profile, _) = self.estimate_profile(choice)?;
        let estimate = pagelamp_llm::estimate::estimate(
            &profile,
            &choice.model,
            prompt,
            output,
            choice.effort,
            max_output,
        );
        let (upper, price_known) = priced(&estimate, &profile, destination);
        let block = self.other_blocks(
            store,
            &choice.backend,
            &choice.model,
            &profile,
            destination,
            upper,
            price_known,
        )?;
        Ok(block.filter(|reason| !(override_budget && *reason == BlockReason::BudgetReached)))
    }

    /// The ChatGPT plan: the disclosure, then the weekly run cap.
    fn codex_blocks(&self, store: &Store) -> Result<Option<BlockReason>> {
        // A Codex routing stored by an earlier build blocks here instead of running.
        if !self.chatgpt_plan_offered() {
            return Ok(Some(BlockReason::BackendDisabledInThisBuild));
        }
        let version = self.disclosure(&BackendRef::Codex)?.version;
        let acknowledged = settings::disclosures(store)?
            .get(&backend_key(&BackendRef::Codex))
            .is_some_and(|ack| ack.version == version);
        if !acknowledged {
            return Ok(Some(BlockReason::DisclosureNotAcknowledged));
        }
        if self.codex_cap_reached(store)? {
            return Ok(Some(BlockReason::WeeklyRunCapReached));
        }
        Ok(None)
    }

    /// The disclosure, an unpriced model and the budget, in that order. A model that runs on
    /// this computer (`destination`) is free: no price, no budget.
    #[allow(clippy::too_many_arguments)]
    fn other_blocks(
        &self,
        store: &Store,
        backend: &super::BackendRef,
        model: &str,
        profile: &ProviderProfile,
        destination: Destination,
        upper: Option<u64>,
        price_known: bool,
    ) -> Result<Option<BlockReason>> {
        let key = backend_key(backend);
        let facts = super::disclosure_for(profile);
        let acknowledged = settings::disclosures(store)?
            .get(&key)
            .is_some_and(|ack| ack.version == facts.version);
        if !acknowledged {
            return Ok(Some(BlockReason::DisclosureNotAcknowledged));
        }
        if destination == Destination::OnDevice {
            return Ok(None); // free: no price, no budget
        }
        if !price_known
            && !settings::unpriced(store)?
                .get(&key)
                .is_some_and(|models| models.iter().any(|m| m == model))
        {
            return Ok(Some(BlockReason::PriceUnknownNotAcknowledged));
        }
        if let (Some(cap), Some(upper)) = (settings::monthly_budget(store)?, upper) {
            let spent = super::usage::spent_this_month(store)?;
            if spent.saturating_add(upper) > cap {
                return Ok(Some(BlockReason::BudgetReached));
            }
        }
        Ok(None)
    }
}

/// The model a feature is routed to, if any.
pub(crate) fn feature_choice(store: &Store, feature: AiFeature) -> Result<Option<ModelChoice>> {
    Ok(settings::routing(store)?.0.get(&feature).cloned())
}

/// How much course text a syllabus reading may carry: ≈ 60k characters in the cloud, ≈ 24k on
/// this computer (calendar design §7.3).
/// Where a model runs, as far as PageLamp can tell without asking anything (estimates, and why
/// a run can't start): a provider off this computer is the cloud; on this computer LM Studio
/// runs its models here, Ollama does unless the model is one of its cloud models by its tag
/// (`is_cloud_model_id`), and any other server can't say, so it counts as the cloud. A run
/// decides again before it sends anything (`run_destination`).
pub(crate) fn estimate_destination(profile: &ProviderProfile, model: &str) -> Destination {
    if !profile.on_device() {
        return Destination::Cloud;
    }
    match profile.id.as_str() {
        "lm_studio" => Destination::OnDevice,
        "ollama" if !pagelamp_llm::profile::is_cloud_model_id(model) => Destination::OnDevice,
        _ => Destination::Cloud,
    }
}

/// The price list's estimate for a model at `destination`. It counts every server on this
/// computer as free, but a model such a server runs in the cloud has no known price.
fn priced(
    estimate: &pagelamp_llm::estimate::Estimate,
    profile: &ProviderProfile,
    destination: Destination,
) -> (Option<u64>, bool) {
    if destination == Destination::Cloud && profile.on_device() {
        (None, false)
    } else {
        (estimate.micro_usd_upper, estimate.price_known)
    }
}

/// Ollama names a model's default tag `latest`: `llama3.2` and `llama3.2:latest` are one model.
fn same_ollama_model(listed: &str, chosen: &str) -> bool {
    let base = |id: &str| id.strip_suffix(":latest").unwrap_or(id).to_string();
    base(listed) == base(chosen)
}

pub(crate) fn calendar_budget(destination: Destination) -> ContextBudget {
    ContextBudget {
        max_chars: match destination {
            Destination::Cloud => CALENDAR_CONTEXT_CHARS,
            Destination::OnDevice => CALENDAR_LOCAL_CONTEXT_CHARS,
        },
    }
}

/// The prompt, answer format and output budget a feature's run would use.
pub(crate) fn request_shape(
    feature: AiFeature,
    context: &GatedContext,
) -> (RenderedPrompt, OutputSpec, u32) {
    match feature {
        AiFeature::StudyPlan => (
            assemble(prompts::STUDY_PLAN, context, None),
            OutputSpec::for_type::<PlanTasks>("study_plan_tasks").unwrap_or(OutputSpec::Text),
            PLAN_MAX_OUTPUT,
        ),
        AiFeature::WeeklyExplanation => (
            assemble(prompts::WEEKLY_EXPLANATION, context, None),
            OutputSpec::Text,
            EXPLANATION_MAX_OUTPUT,
        ),
        AiFeature::WeeklyNote => (
            assemble(prompts::WEEKLY_NOTE, context, None),
            OutputSpec::Text,
            NOTE_MAX_OUTPUT,
        ),
        AiFeature::CourseCalendar => (
            assemble(prompts::COURSE_CALENDAR, context, None),
            OutputSpec::for_type::<CalendarExtraction>("course_calendar")
                .unwrap_or(OutputSpec::Text),
            CALENDAR_MAX_OUTPUT,
        ),
    }
}

/// An estimate for a run that can't start.
fn blocked_estimate(reason: BlockReason) -> CostEstimate {
    CostEstimate {
        micro_usd_upper: None,
        input_tokens: 0,
        max_output_tokens: 0,
        reasoning_allowance: 0,
        repair_possible: false,
        price_known: false,
        would_block: Some(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here(preset: &str) -> ProviderProfile {
        pagelamp_llm::profile::preset(preset)
            .unwrap()
            .with_base_url("http://127.0.0.1:11434".parse().unwrap())
    }

    /// Without asking anything, Ollama's cloud models by their tag, and any server here that
    /// can't say where its models run, count as the cloud; LM Studio's models run here.
    #[test]
    fn the_estimate_s_rule_for_where_a_model_runs() {
        assert_eq!(
            estimate_destination(&here("ollama"), "llama3.2"),
            Destination::OnDevice
        );
        for cloud in [
            "gpt-oss:120b-cloud",
            "qwen3-coder:480b-cloud",
            "kimi-k2:cloud",
        ] {
            assert_eq!(
                estimate_destination(&here("ollama"), cloud),
                Destination::Cloud
            );
        }
        assert_eq!(
            estimate_destination(&here("lm_studio"), "any-model"),
            Destination::OnDevice
        );
        assert_eq!(
            estimate_destination(&here("custom"), "any-model"),
            Destination::Cloud
        );
        let openai = pagelamp_llm::profile::preset("openai").unwrap();
        assert_eq!(estimate_destination(openai, "gpt-5"), Destination::Cloud);
    }
}
