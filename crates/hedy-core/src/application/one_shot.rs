//! A one-shot, read-only request to an account's own agent ("Sugerir con
//! IA", "Resumir con IA"): no memory, no MCP, no resume, tools denied. Its
//! tokens count as the given task's usage. A limit reported mid-answer stops
//! it: Hedy never moves to another account (rule 3).

use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use super::ports::{AgentBackend, AgentRequest, EventSink, RateLimit};
use super::{AppResult, Hedy};
use crate::domain::{Account, AgentEvent, LimitLevel, Phase, Stage, TaskId, TaskMode};

/// What to ask, and of whom.
pub(super) struct OneShot<'a> {
    /// Whose usage the tokens count as.
    pub task: &'a TaskId,
    pub account: &'a Account,
    /// The stage whose model to use (the account's default when it has none).
    pub stage: &'a Stage,
    pub cwd: PathBuf,
    pub prompt: String,
    /// The answer to any tool request.
    pub refusal: String,
    pub timeout: Duration,
}

/// Why a one-shot request gave no answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OneShotFailure {
    /// The account reported its limit while answering.
    AtLimit,
    /// It ended without a usable answer (with the agent's error, if any).
    Failed(Option<String>),
    TimedOut,
}

impl Hedy {
    /// Runs the request and returns the agent's raw answer (its final
    /// summary, else its last message). Port failures are errors; what the
    /// agent did is the inner result.
    pub(super) fn one_shot(&self, shot: OneShot) -> AppResult<Result<String, OneShotFailure>> {
        let (backend, launch) = self.backend(shot.account)?;
        let model = match backend {
            AgentBackend::OpenAiCompatible { .. } => shot
                .stage
                .model
                .clone()
                .or_else(|| shot.account.provider.default_api_model().map(str::to_owned)),
            _ => shot.stage.model.clone(),
        };
        let (tx, rx) = mpsc::channel();
        let sink: EventSink = Arc::new(move |event| {
            let _ = tx.send(event);
        });
        let mut session = self.agents.start(
            AgentRequest {
                backend,
                launch: launch.in_dir(shot.cwd),
                mode: TaskMode::Ask,
                phase: Phase::Review,
                plan_first: false,
                model,
                prompt: shot.prompt,
                images: Vec::new(),
                resume: None,
                memory: None,
                mcp: Vec::new(),
            },
            sink,
        )?;
        let deadline = Instant::now() + shot.timeout;
        let mut last_text: Option<String> = None;
        let result = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(AgentEvent::Usage(sample)) => {
                    // The tokens were spent: they count for the account.
                    let _ = self.handle_usage(shot.task, &shot.account.id, &sample);
                }
                Ok(AgentEvent::RateLimit {
                    status,
                    utilization,
                    resets_at,
                }) => {
                    self.store.set_rate_limit(
                        &shot.account.id,
                        &RateLimit {
                            status: status.clone(),
                            utilization,
                            resets_at,
                            at: self.now_secs(),
                        },
                    )?;
                    if LimitLevel::from_report(&status, utilization) == Some(LimitLevel::Reached) {
                        break Err(OneShotFailure::AtLimit);
                    }
                }
                Ok(AgentEvent::Message { text }) => last_text = Some(text),
                Ok(AgentEvent::PermissionRequest { request_id, .. }) => {
                    let _ = session.answer(&request_id, false, Some(&shot.refusal));
                }
                Ok(AgentEvent::Finished { ok, summary, .. }) => {
                    let answer = if summary.trim().is_empty() {
                        last_text.take()
                    } else {
                        Some(summary)
                    };
                    break answer
                        .filter(|a| ok && !a.trim().is_empty())
                        .ok_or(OneShotFailure::Failed(None));
                }
                Ok(AgentEvent::Error { message }) => {
                    break Err(OneShotFailure::Failed(Some(message)));
                }
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => break Err(OneShotFailure::TimedOut),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err(OneShotFailure::Failed(None));
                }
            }
        };
        session.stop();
        Ok(result)
    }
}
