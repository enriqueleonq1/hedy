//! Runs a case end to end through Hedy, as the app would, answering every
//! card itself: plans are approved and permissions allowed. Hedy's own rules
//! still apply (protected files, writes only inside the worktree).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use hedy_core::adapters::CODE_MAP_PROGRAM;
use hedy_core::application::ports::TaskEvents;
use hedy_core::application::{Hedy, NewTask, Setting};
use hedy_core::domain::{
    AccountId, AgentEvent, Decision, Task, TaskId, TaskMode, TaskStatus, UsageSample,
};

use hedy_core::experiments;

use crate::{Case, Layer, RunResult, Variant};

enum Update {
    Event(TaskId, AgentEvent),
    /// A task changed status. Hedy saves it after recording the event that
    /// caused it, so the end of a task is seen here, not in `Finished`.
    Status(TaskId, TaskStatus),
}

/// Forwards everything Hedy reports to the runner's thread.
struct ChannelEvents(Sender<Update>);

impl TaskEvents for ChannelEvents {
    fn event(&self, task: &TaskId, event: &AgentEvent) {
        let _ = self.0.send(Update::Event(task.clone(), event.clone()));
    }

    fn task_updated(&self, task: &Task) {
        let _ = self.0.send(Update::Status(task.id.clone(), task.status));
    }
}

/// A folder removed when dropped, on every way out (errors included).
pub struct RemoveOnDrop(pub PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct RunnerOptions {
    pub account: AccountId,
    pub model: Option<String>,
    /// The code-map server, for the layers that use it.
    pub code_map: Option<String>,
    /// Where repos are copied for each run.
    pub work_dir: PathBuf,
}

pub struct Runner {
    hedy: Hedy,
    events: Receiver<Update>,
    options: RunnerOptions,
}

/// Seconds since the epoch.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Hedy evals",
            "-c",
            "user.email=evals@hedy.invalid",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .context("no se pudo ejecutar git")?;
    if !status.status.success() {
        bail!("git {args:?}: {}", String::from_utf8_lossy(&status.stderr));
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// The last `max` bytes of `text`, on a character boundary.
fn tail(text: &str, max: usize) -> String {
    let mut start = text.len().saturating_sub(max);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].trim().to_owned()
}

/// What the runner saw of a task while it ran.
#[derive(Default)]
struct Observed {
    usage: UsageSample,
    actions: u32,
    files_read: u32,
    rereads: u32,
    /// Files read so far, to count the rereads.
    read: HashSet<String>,
    denied: u32,
    error: Option<String>,
}

impl Runner {
    /// `hedy` must run on its own data folder: each run registers a project.
    pub fn new(hedy: Hedy, options: RunnerOptions) -> Self {
        let (tx, rx) = channel();
        let hedy = hedy.with_task_events(Arc::new(ChannelEvents(tx)));
        Self {
            hedy,
            events: rx,
            options,
        }
    }

    /// One run with `variant`'s experiments. Whatever goes wrong (copying
    /// the repo, launching the agent, running the check) fails this run only:
    /// it is recorded in [`RunResult::error`] and the suite goes on.
    pub fn run(&self, case: &Case, layer: Layer, variant: &Variant, attempt: u32) -> RunResult {
        let started_at = now();
        let mut result = RunResult {
            case: case.name.clone(),
            layer,
            variant: variant.name.clone(),
            attempt,
            account: self.options.account.as_str().to_owned(),
            model: self.options.model.clone(),
            passed: false,
            tokens: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd: None,
            duration_ms: 0,
            actions: 0,
            files_read: 0,
            rereads: 0,
            denied: 0,
            notes_read: 0,
            error: None,
            check_output: String::new(),
            started_at,
        };
        // Hedy, the agent's hooks and the memory server it starts all read
        // this process's experiments.
        experiments::set(variant.experiments);
        let repo = self.options.work_dir.join(format!(
            "{}-{layer}-{}-{attempt}-{started_at}",
            case.name,
            variant.name.replace('+', "_")
        ));
        let _repo = RemoveOnDrop(repo.clone());
        if let Err(e) = self.run_in(case, layer, &repo, &mut result) {
            result.passed = false;
            result.error.get_or_insert_with(|| format!("{e:#}"));
        }
        result
    }

    /// Prepares the repo and runs the task in it; always forgets the project.
    fn run_in(&self, case: &Case, layer: Layer, repo: &Path, out: &mut RunResult) -> Result<()> {
        copy_dir(&case.repo(), repo)?;
        git(repo, &["init", "-q", "-b", "main"])?;
        git(repo, &["add", "-A"])?;
        git(repo, &["commit", "-q", "-m", "Estado inicial del caso"])?;
        let repo = repo.canonicalize()?;
        let result = self.run_task(case, layer, &repo, out);
        let _ = self.hedy.remove_project(&repo);
        result
    }

    fn run_task(&self, case: &Case, layer: Layer, repo: &Path, out: &mut RunResult) -> Result<()> {
        let hedy = &self.hedy;
        let account = self.options.account.as_str();
        hedy.use_account(repo, account, None)?;
        if layer.memory() {
            for note in &case.spec.notes {
                hedy.add_note(Some(repo), &note.text, note.category)?;
            }
        } else {
            hedy.set_memory_reader(repo, account, false)?;
        }
        let code_map = match layer.code_map() {
            true => Some(self.options.code_map.as_deref().with_context(|| {
                format!("la capa {layer} necesita el mapa del código ({CODE_MAP_PROGRAM})")
            })?),
            false => None,
        };
        hedy.set_setting(Setting::CodeMap, code_map)?;

        let clock = Instant::now();
        let task = hedy.start_task(NewTask {
            images: Vec::new(),
            project: repo.to_owned(),
            prompt: case.spec.prompt.clone(),
            mode: TaskMode::Project,
            model: self.options.model.clone(),
            plan_first: false,
        });
        let task = match task {
            Ok(task) => task,
            Err(e) => {
                out.duration_ms = clock.elapsed().as_millis() as u64;
                return Err(e.into());
            }
        };
        let result = self.finish(case, &task.id, repo, clock, out);
        // In review the agent's session is still alive: discarding stops it
        // and removes the worktree, whatever happened above.
        match hedy.task(&task.id) {
            Ok(current) if current.status.is_closed() => {}
            _ => {
                let _ = hedy.discard_task(&task.id);
            }
        }
        result
    }

    /// Waits for the task, then checks its work.
    fn finish(
        &self,
        case: &Case,
        id: &TaskId,
        repo: &Path,
        clock: Instant,
        out: &mut RunResult,
    ) -> Result<()> {
        let observed = self.drive(id, Duration::from_secs(case.spec.timeout_secs));
        out.duration_ms = clock.elapsed().as_millis() as u64;
        let usage = &observed.usage;
        out.tokens = usage.total_tokens();
        out.input_tokens = usage.input_tokens;
        out.output_tokens = usage.output_tokens;
        out.cache_read_tokens = usage.cache_read_tokens;
        out.cache_write_tokens = usage.cache_write_tokens;
        out.cost_usd = usage.cost_usd;
        out.actions = observed.actions;
        out.files_read = observed.files_read;
        out.rereads = observed.rereads;
        out.denied = observed.denied;
        out.error = observed.error;
        out.notes_read = self.hedy.memory(repo)?.used_last_task.len();
        if out.error.is_none() {
            let task = self.hedy.task(id)?;
            (out.passed, out.check_output) = self.check(case, &task.work_dir())?;
        }
        Ok(())
    }

    /// Answers cards and collects usage until the task stops.
    fn drive(&self, id: &TaskId, timeout: Duration) -> Observed {
        let mut seen = Observed::default();
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let update = match self.events.recv_timeout(left.max(Duration::from_millis(1))) {
                Ok(update) => update,
                Err(RecvTimeoutError::Timeout) => {
                    let _ = self.hedy.stop_task(id);
                    seen.error = Some(format!("no terminó en {} s", timeout.as_secs()));
                    return seen;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    seen.error = Some("Hedy dejó de enviar eventos".into());
                    return seen;
                }
            };
            let event = match update {
                Update::Event(task, event) if &task == id => event,
                Update::Status(task, status) if &task == id => {
                    match status {
                        TaskStatus::Review => {
                            // Errors the agent recovered from do not fail the run.
                            seen.error = None;
                            return seen;
                        }
                        TaskStatus::Failed => {
                            seen.error
                                .get_or_insert_with(|| "el agente terminó con error".into());
                            return seen;
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };
            let answered = match &event {
                AgentEvent::PermissionRequest { request_id, .. } => {
                    self.hedy.answer_permission(id, request_id, Decision::Allow)
                }
                AgentEvent::Plan {
                    request_id: Some(request_id),
                    ..
                } => self.hedy.answer_plan(id, request_id, true, None),
                AgentEvent::Usage(sample) => {
                    seen.usage.add(sample);
                    Ok(())
                }
                AgentEvent::ToolUsed { tool, summary } if tool == "Read" => {
                    seen.actions += 1;
                    seen.files_read += 1;
                    if !seen.read.insert(summary.clone()) {
                        seen.rereads += 1;
                    }
                    Ok(())
                }
                AgentEvent::PermissionResolved {
                    decision: Decision::Deny,
                    automatic: true,
                    ..
                } => {
                    seen.denied += 1;
                    Ok(())
                }
                AgentEvent::FileEdited { .. }
                | AgentEvent::CommandRun { .. }
                | AgentEvent::TestsRun { .. }
                | AgentEvent::ToolUsed { .. } => {
                    seen.actions += 1;
                    Ok(())
                }
                AgentEvent::Error { message } => {
                    seen.error.get_or_insert_with(|| message.clone());
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(e) = answered {
                seen.error = Some(e.to_string());
                return seen;
            }
        }
    }

    /// Copies the hidden checks into the worktree and runs the check command.
    fn check(&self, case: &Case, work_dir: &Path) -> Result<(bool, String)> {
        if case.checks().is_dir() {
            copy_dir(&case.checks(), work_dir)?;
        }
        let output = Command::new("timeout")
            .args(["300", "sh", "-c", &case.spec.check])
            .current_dir(work_dir)
            .output()
            .context("no se pudo correr la verificación")?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok((output.status.success(), tail(&text, 2000)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_the_end_on_a_char_boundary() {
        assert_eq!(tail("añoño", 3), "ño");
        assert_eq!(tail("ok", 100), "ok");
    }
}
