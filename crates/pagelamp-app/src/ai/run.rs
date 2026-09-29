//! One model run of a feature (model-access design §3.3, §3.7): after the feature's own gate
//! and blocks, every backend goes the same way.
//!
//! - One run per generation id at a time (`busy` otherwise); `cancel_run` stops it: the HTTP
//!   driver drops the request, Codex gets SIGINT and then a kill.
//! - Events: `started`, `stage waiting_for_model`, the JSON-fallback notice and the repair
//!   stage, answer text (text answers only) and `usage`. The feature sends `finished` once it
//!   has checked and stored the answer.
//! - Every run that reached a backend is one `ai_usage` row: tokens and an estimated cost, or
//!   "free" on this computer, or one run of the ChatGPT plan's weekly cap. A cancelled HTTP
//!   run that never got its counts is recorded with estimated input tokens.
//! - The store is only opened before and after the model call, never across it.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;
use pagelamp_core::ai::{AiFeature, BlockReason, UsageRecord};
use pagelamp_core::ai_gate::RenderedPrompt;
use pagelamp_llm::estimate::{cjk_tenths, count_tokens_upper, usage_cost};
use pagelamp_llm::request::Notice;
use pagelamp_llm::{CancellationToken, GenerateRequest, LlmError, OutputSpec, StreamEvent, Usage};

use super::settings::backend_key;
use super::{BackendRef, GenEvent, GenNoticeCode, GenStage, ModelChoice, TokenUsage};
use crate::{App, AppError, AppErrorKind, Result};

/// The runs in progress in this process, by generation id. A batch ("Read syllabi for N
/// courses") is a run too: its token is the parent of each course's run, so stopping the batch
/// stops the course being read.
#[derive(Default)]
pub(crate) struct Runs(Mutex<HashMap<String, CancellationToken>>);

impl Runs {
    /// Register `id` (with a token of its own, or a child of `parent`'s); `busy` if a run with
    /// that id is still going.
    pub(crate) fn register(
        &self,
        id: &str,
        parent: Option<&CancellationToken>,
    ) -> Result<(Registration<'_>, CancellationToken)> {
        let mut running = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if running.contains_key(id) {
            return Err(AppError::new(
                AppErrorKind::Busy,
                "This is already running; wait for it or stop it first.",
            ));
        }
        let cancel = match parent {
            Some(parent) => parent.child_token(),
            None => CancellationToken::new(),
        };
        running.insert(id.to_string(), cancel.clone());
        Ok((
            Registration {
                runs: self,
                id: id.to_string(),
            },
            cancel,
        ))
    }

    /// Stop run `id` (and its children); false when nothing with that id runs.
    pub(crate) fn cancel(&self, id: &str) -> bool {
        let running = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match running.get(id) {
            Some(cancel) => {
                cancel.cancel();
                true
            }
            None => false,
        }
    }
}

/// Unregisters its run when dropped (also on an early `?` or a panic).
pub(crate) struct Registration<'a> {
    runs: &'a Runs,
    id: String,
}

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        let mut running = self.runs.0.lock().unwrap_or_else(|e| e.into_inner());
        running.remove(&self.id);
    }
}

/// What a feature asks of one run.
pub(crate) struct RunRequest<'a> {
    pub generation_id: &'a str,
    /// The batch this run belongs to, whose stop also stops it.
    pub parent: Option<&'a CancellationToken>,
    pub feature: AiFeature,
    pub choice: &'a ModelChoice,
    pub prompt: RenderedPrompt,
    pub output: OutputSpec,
    pub max_output_tokens: u32,
}

/// A finished run: the answer as it came (the feature validates it) and its provenance.
#[derive(Clone, Debug)]
pub(crate) struct RunOutcome {
    /// The parsed answer of a JSON run.
    pub json: Option<serde_json::Value>,
    pub backend_label: String,
    /// The model that answered (Codex may fall back from a retired default).
    pub model: String,
    pub on_device: bool,
}

/// How a run's cost is known, for its usage row.
#[derive(Clone, Copy)]
pub(crate) enum Billing<'a> {
    /// An API key: the price list's cost, if the model has one.
    Priced(&'a pagelamp_llm::ProviderProfile),
    /// A model on this computer.
    OnDevice,
    /// A run of the ChatGPT plan (its weekly cap counts it).
    Plan,
}

impl App {
    /// Run `request` on its backend (see the module docs). The caller has gated the context and
    /// checked every block; `on_event` gets the run's progress.
    pub(crate) async fn run_model(
        &self,
        request: RunRequest<'_>,
        on_event: &(dyn Fn(GenEvent) + Send + Sync),
    ) -> Result<RunOutcome> {
        let (_registration, cancel) = self
            .state
            .runs
            .register(request.generation_id, request.parent)?;
        match &request.choice.backend {
            BackendRef::Codex => self.codex_run(&request, on_event, &cancel).await,
            BackendRef::Provider { .. } => self.http_run(request, on_event, &cancel).await,
            BackendRef::ClaudeCode => Err(AppError::blocked(
                BlockReason::BackendDisabledInThisBuild,
                "The Claude plan isn't available in this build yet.",
            )),
        }
    }

    /// Stop run `id` (a generation or a batch); false when nothing with that id runs.
    pub(crate) fn cancel_run(&self, id: &str) -> bool {
        self.state.runs.cancel(id)
    }

    async fn http_run(
        &self,
        request: RunRequest<'_>,
        on_event: &(dyn Fn(GenEvent) + Send + Sync),
        cancel: &CancellationToken,
    ) -> Result<RunOutcome> {
        let provider = self.provider_for(&request.choice.backend)?;
        let backend_label = provider.row.label.clone();
        let profile = provider.profile.clone();
        let on_device = profile.on_device();
        let backend = provider.backend()?;
        let model = request.choice.model.clone();
        on_event(GenEvent::Started {
            generation_id: request.generation_id.to_string(),
            backend_label: backend_label.clone(),
            model: model.clone(),
            on_device,
        });
        on_event(GenEvent::Stage {
            stage: GenStage::WaitingForModel,
        });
        // Counted before the prompt moves into the request, for a run cancelled before the
        // provider reported its counts.
        let input_upper = count_tokens_upper(
            &format!(
                "{}{}",
                request.prompt.instructions(),
                request.prompt.user_text()
            ),
            cjk_tenths(profile.wire),
        );
        let text_answer = request.output == OutputSpec::Text;
        let forward = |event: StreamEvent| match event {
            StreamEvent::TextDelta(text) if text_answer => on_event(GenEvent::TextDelta { text }),
            StreamEvent::Notice(Notice::JsonFallback) => on_event(GenEvent::Notice {
                code: GenNoticeCode::JsonFallback,
            }),
            StreamEvent::Notice(Notice::Repairing) => on_event(GenEvent::Stage {
                stage: GenStage::Repairing,
            }),
            _ => {}
        };
        let result = backend
            .generate(
                GenerateRequest {
                    model: model.clone(),
                    prompt: request.prompt,
                    output: request.output,
                    effort: request.choice.effort,
                    max_output_tokens: request.max_output_tokens,
                },
                &forward,
                cancel.clone(),
            )
            .await;
        let billing = if on_device {
            Billing::OnDevice
        } else {
            Billing::Priced(&profile)
        };
        let key = backend_key(&request.choice.backend);
        match result {
            Ok(outcome) => {
                let usage = self.record_run_usage(
                    &key,
                    &model,
                    request.feature,
                    outcome.usage,
                    billing,
                    "ok",
                )?;
                on_event(GenEvent::Usage { usage });
                Ok(RunOutcome {
                    json: outcome.json,
                    backend_label,
                    model: outcome.model_reported.unwrap_or(model),
                    on_device,
                })
            }
            Err(LlmError::Cancelled) => {
                let estimated = Usage {
                    input_uncached: input_upper,
                    estimated: true,
                    ..Usage::default()
                };
                self.record_run_usage(
                    &key,
                    &model,
                    request.feature,
                    estimated,
                    billing,
                    "cancelled",
                )?;
                Err(AppError::cancelled())
            }
            Err(error) => {
                self.record_run_usage(
                    &key,
                    &model,
                    request.feature,
                    Usage::default(),
                    billing,
                    "failed",
                )?;
                Err(error.into())
            }
        }
    }

    /// Write a run's `ai_usage` row and return its counts as the facade shows them.
    pub(crate) fn record_run_usage(
        &self,
        backend: &str,
        model: &str,
        feature: AiFeature,
        usage: Usage,
        billing: Billing<'_>,
        outcome: &str,
    ) -> Result<TokenUsage> {
        let (micro_usd, cost_basis) = match billing {
            Billing::Priced(profile) => match usage_cost(profile, model, &usage) {
                Some(cost) => (Some(cost), "priced"),
                None => (None, "unpriced"),
            },
            Billing::OnDevice => (Some(0), "free_on_device"),
            Billing::Plan => (None, "plan"),
        };
        self.write_store()?.record_ai_usage(&UsageRecord {
            at: Utc::now(),
            backend: backend.to_string(),
            model: model.to_string(),
            feature,
            input_uncached: usage.input_uncached,
            cache_read: usage.cache_read,
            cache_write: usage.cache_write,
            output: usage.output,
            reasoning: usage.reasoning,
            micro_usd,
            cost_basis: cost_basis.to_string(),
            estimated: usage.estimated,
            outcome: outcome.to_string(),
        })?;
        Ok(TokenUsage {
            input_tokens: usage.input_uncached + usage.cache_read + usage.cache_write,
            cached_input_tokens: usage.cache_read,
            output_tokens: usage.output,
            reasoning_tokens: usage.reasoning,
        })
    }
}

/// A generation id as a Codex run folder name: letters, digits, `-` and `_` only.
pub(crate) fn run_folder_name(generation_id: &str) -> String {
    let safe: String = generation_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    format!("gen-{safe}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_id_is_registered_once_and_cancelled_by_id() {
        let runs = Runs::default();
        let (registration, cancel) = runs.register("gen-1", None).unwrap();
        assert!(matches!(runs.register("gen-1", None), Err(e) if e.kind == AppErrorKind::Busy));
        assert!(!runs.cancel("other"));
        assert!(runs.cancel("gen-1"));
        assert!(cancel.is_cancelled());
        drop(registration);
        assert!(runs.register("gen-1", None).is_ok(), "free again");
    }

    #[test]
    fn stopping_a_batch_stops_its_current_run() {
        let runs = Runs::default();
        let (_batch, batch_token) = runs.register("batch-1", None).unwrap();
        let (_child, child_token) = runs.register("batch-1:0", Some(&batch_token)).unwrap();
        assert!(runs.cancel("batch-1"));
        assert!(child_token.is_cancelled());
    }

    #[test]
    fn run_folders_are_safe_names() {
        assert_eq!(run_folder_name("7f3a-b2_c"), "gen-7f3a-b2_c");
        assert_eq!(run_folder_name("../../etc"), "gen-------etc");
    }
}
