//! Runs a case end to end through Hedy, as the app would, answering every
//! card itself: plans are approved and permissions allowed. Hedy's own rules
//! still apply (protected files, writes only inside the worktree).

use std::collections::{BTreeMap, HashSet};
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

use crate::{Case, Layer, RunResult, StepTrace, Variant};
use hedy_core::domain::flow::NodeKind;

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
    /// Where each run's patch and verdict are saved (`--diffs`).
    pub diffs: Option<PathBuf>,
}

pub struct Runner {
    pub(crate) hedy: Hedy,
    events: Receiver<Update>,
    pub(crate) options: RunnerOptions,
}

/// Seconds since the epoch.
pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The bare repo of a run's team, next to the run's repo.
fn team_repo(repo: &Path) -> std::path::PathBuf {
    let mut name = repo.as_os_str().to_owned();
    name.push(".team.git");
    name.into()
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<()> {
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

/// Copies a folder, without git's own (`.git`).
pub(crate) fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            continue;
        }
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

/// The case's starting repo in `repo`, as a git repo with one commit.
pub(crate) fn prepare_repo(case: &Case, repo: &Path) -> Result<PathBuf> {
    copy_dir(&case.repo(), repo)?;
    git(repo, &["init", "-q", "-b", "main"])?;
    git(repo, &["add", "-A"])?;
    git(repo, &["commit", "-q", "-m", "Estado inicial del caso"])?;
    Ok(repo.canonicalize()?)
}

/// The commit `repo` is at.
pub(crate) fn head(repo: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .context("no se pudo ejecutar git")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Runs a shell command in `dir` for at most five minutes: whether it
/// passed, and the end of what it printed.
pub(crate) fn run_command(dir: &Path, command: &str) -> Result<(bool, String)> {
    let output = Command::new("timeout")
        .args(["300", "sh", "-c", command])
        .current_dir(dir)
        .output()
        .context("no se pudo correr la verificación")?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((output.status.success(), tail(&text, 2000)))
}

/// `--flows file:<path>`: a flow written as text (the experiments' own).
pub const FLOW_FILE: &str = "file:";

/// What a flow is called in a result's variant: a template's key, or a
/// flow file's name without its extension.
pub fn flow_label(flow: &str) -> String {
    match flow.strip_prefix(FLOW_FILE) {
        Some(path) => Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_owned()),
        None => flow.to_owned(),
    }
}

/// Everything changed in `work_dir` since `base`, new files included, as a
/// patch `git apply` takes. A throwaway index leaves the worktree's own alone;
/// Python's caches are left out.
pub fn changes_since(work_dir: &Path, base: &str) -> Result<String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let index =
        std::env::temp_dir().join(format!("hedy-evals-index-{}-{stamp}", std::process::id()));
    let _index = RemoveOnDrop(index.clone());
    let git = |args: &[&str]| -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .env("GIT_INDEX_FILE", &index)
            .current_dir(work_dir)
            .output()
            .context("no se pudo ejecutar git")?;
        if !output.status.success() {
            bail!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };
    git(&["read-tree", base])?;
    git(&[
        "add",
        "-A",
        "--",
        ".",
        ":(exclude,glob)**/__pycache__/**",
        ":(exclude,glob)**/*.pyc",
    ])?;
    git(&["diff", "--cached", "--binary", base])
}

/// Saves a run's patch as `<case>-<layer>-<variant>-<attempt>-<start>.patch`
/// in `dir`, with the hidden check's verdict beside it in a `.json` of the
/// same name. Returns the patch's file name.
pub fn save_diff(dir: &Path, run: &RunResult, patch: &str) -> Result<String> {
    std::fs::create_dir_all(dir)?;
    let stem = format!(
        "{}-{}-{}-{}-{}",
        run.case,
        run.layer,
        run.variant.replace(['+', ':', '/'], "_"),
        run.attempt,
        run.started_at
    );
    let verdict = serde_json::json!({
        "case": run.case,
        "layer": run.layer,
        "variant": run.variant,
        "attempt": run.attempt,
        "model": run.model,
        "passed": run.passed,
        "error": run.error,
        "checkOutput": run.check_output,
    });
    std::fs::write(dir.join(format!("{stem}.patch")), patch)?;
    std::fs::write(
        dir.join(format!("{stem}.json")),
        serde_json::to_string_pretty(&verdict)?,
    )?;
    Ok(format!("{stem}.patch"))
}

/// What the runner saw of a task while it ran.
#[derive(Default)]
pub(crate) struct Observed {
    usage: UsageSample,
    actions: u32,
    files_read: u32,
    rereads: u32,
    /// Files read so far, to count the rereads.
    read: HashSet<String>,
    denied: u32,
    subagents: u32,
    /// Each action by kind (`Read`, `Edit`, `Bash:git`, an MCP tool…).
    tools: BTreeMap<String, u32>,
    pub(crate) error: Option<String>,
    /// A flow's steps, in order.
    steps: Vec<StepTrace>,
    /// What the agent last said.
    pub(crate) last_message: Option<String>,
    /// Files read before its first edit: how long it took to find its way.
    pub(crate) reads_before_edit: Option<u32>,
    /// The runner stopped it at its cap of actions.
    pub(crate) stopped: bool,
}

impl Observed {
    /// Adds what was seen to `out`'s totals (a run with several turns adds
    /// each); the last error stands.
    pub(crate) fn add_to(&self, out: &mut RunResult) {
        let usage = &self.usage;
        out.tokens += usage.total_tokens();
        out.input_tokens += usage.input_tokens;
        out.output_tokens += usage.output_tokens;
        out.cache_read_tokens += usage.cache_read_tokens;
        out.cache_write_tokens += usage.cache_write_tokens;
        out.cost_usd = match (out.cost_usd, usage.cost_usd) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
        };
        out.actions += self.actions;
        out.files_read += self.files_read;
        out.rereads += self.rereads;
        out.denied += self.denied;
        out.subagents += self.subagents;
        for (tool, n) in &self.tools {
            *out.tools.entry(tool.clone()).or_default() += n;
        }
        out.error = self.error.clone();
        out.steps.extend(self.steps.iter().cloned());
    }
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
    /// `flow`: `none` (one agent) or a template's key; a flow shows in the
    /// result's variant (`base+flow:fix-loop`), so the report compares it
    /// with the same variant without a flow.
    pub fn run(
        &self,
        case: &Case,
        layer: Layer,
        variant: &Variant,
        flow: &str,
        attempt: u32,
    ) -> RunResult {
        let started_at = now();
        let name = match flow {
            "none" => variant.name.clone(),
            key => format!("{}+flow:{}", variant.name, flow_label(key)),
        };
        let mut result = self.new_result(case, layer, &name, attempt, started_at);
        // Hedy, the agent's hooks and the memory server it starts all read
        // this process's experiments.
        experiments::set(variant.experiments);
        let repo = self.options.work_dir.join(format!(
            "{}-{layer}-{}-{attempt}-{started_at}",
            case.name,
            name.replace(['+', ':'], "_")
        ));
        let _repo = RemoveOnDrop(repo.clone());
        // Its team's repo too, however the run ends.
        let _team = RemoveOnDrop(team_repo(&repo));
        if let Err(e) = self.run_in(case, layer, flow, &repo, &mut result) {
            result.passed = false;
            result.error.get_or_insert_with(|| format!("{e:#}"));
        }
        result
    }

    /// A run's result before it starts: nothing spent, nothing passed.
    pub(crate) fn new_result(
        &self,
        case: &Case,
        layer: Layer,
        variant: &str,
        attempt: u32,
        started_at: i64,
    ) -> RunResult {
        RunResult {
            case: case.name.clone(),
            layer,
            variant: variant.to_owned(),
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
            subagents: 0,
            tools: BTreeMap::new(),
            notes_read: 0,
            error: None,
            steps: Vec::new(),
            check_output: String::new(),
            diff: None,
            rescue: None,
            long: None,
            started_at,
        }
    }

    /// Prepares the repo and runs the task in it; always forgets the project.
    fn run_in(
        &self,
        case: &Case,
        layer: Layer,
        flow: &str,
        repo: &Path,
        out: &mut RunResult,
    ) -> Result<()> {
        let repo = prepare_repo(case, repo)?;
        let result = self.run_task(case, layer, flow, &repo, out);
        self.forget_project(&repo);
        result
    }

    /// Forgets a run's project and its team.
    pub(crate) fn forget_project(&self, repo: &Path) {
        self.leave_teams();
        let _ = std::fs::remove_dir_all(team_repo(repo));
        let _ = self.hedy.remove_project(repo);
    }

    /// Gives a run's project its account, and its memory and code map as
    /// `layer` says.
    pub(crate) fn prepare_project(&self, case: &Case, layer: Layer, repo: &Path) -> Result<()> {
        let hedy = &self.hedy;
        let account = self.options.account.as_str();
        hedy.use_account(repo, account, None)?;
        if layer.memory() {
            for note in &case.spec.notes {
                hedy.add_note(Some(repo), &note.text, note.category)?;
            }
            if !case.spec.team_notes.is_empty() {
                self.join_team(case, repo)?;
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
        Ok(())
    }

    /// A task in `repo` that asks `prompt`, as the user would.
    pub(crate) fn new_task(&self, repo: &Path, prompt: &str) -> NewTask {
        NewTask {
            images: Vec::new(),
            project: repo.to_owned(),
            prompt: prompt.to_owned(),
            mode: TaskMode::Project,
            model: self.options.model.clone(),
            plan_first: false,
            permissions: None,
        }
    }

    /// Discards a task still open: in review its agent's session is alive.
    pub(crate) fn close_task(&self, id: &TaskId) {
        match self.hedy.task(id) {
            Ok(current) if current.status.is_closed() => {}
            _ => {
                let _ = self.hedy.discard_task(id);
            }
        }
    }

    fn run_task(
        &self,
        case: &Case,
        layer: Layer,
        flow: &str,
        repo: &Path,
        out: &mut RunResult,
    ) -> Result<()> {
        let hedy = &self.hedy;
        self.prepare_project(case, layer, repo)?;
        let clock = Instant::now();
        let request = self.new_task(repo, &case.spec.prompt);
        let task = match flow {
            "none" => hedy.start_task(request),
            // A flow written as text: saved (for every project) and run by its key.
            key if key.starts_with(FLOW_FILE) => std::fs::read_to_string(&key[FLOW_FILE.len()..])
                .map_err(|e| hedy_core::application::AppError::InvalidFlow(vec![e.to_string()]))
                .and_then(|text| hedy.save_flow_text(None, &text))
                .and_then(|entry| hedy.start_flow(request, &entry.key)),
            key => hedy.start_flow(request, key),
        };
        let task = match task {
            Ok(task) => task,
            Err(e) => {
                out.duration_ms = clock.elapsed().as_millis() as u64;
                return Err(e.into());
            }
        };
        let result = self.finish(case, &task.id, repo, clock, out);
        // Stops the agent and removes the worktree, whatever happened above.
        self.close_task(&task.id);
        result
    }

    /// Puts the project in a team made for this run (in a bare repo next to
    /// it), with the case's team-layer notes.
    fn join_team(&self, case: &Case, repo: &Path) -> Result<()> {
        let bare = team_repo(repo);
        std::fs::create_dir_all(&bare)?;
        git(&bare, &["init", "-q", "--bare", "-b", "main"])?;
        let hedy = &self.hedy;
        let (team, _) = hedy.team_create(&bare.to_string_lossy(), "Equipo", "Ana", "Evals")?;
        hedy.team_link_project(&team, repo)?;
        for note in &case.spec.team_notes {
            hedy.team_add_note(&team, None, &note.text, note.category, None)?;
        }
        Ok(())
    }

    /// Leaves every team a run made (their keys are in the keyring).
    fn leave_teams(&self) {
        for team in self.hedy.teams().unwrap_or_default() {
            let _ = self.hedy.team_leave(&team.id);
        }
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
        let observed = self.drive(id, Duration::from_secs(case.spec.timeout_secs), false, None);
        out.duration_ms = clock.elapsed().as_millis() as u64;
        observed.add_to(out);
        out.notes_read = self.hedy.memory(repo)?.used_last_task.len();
        let task = self.hedy.task(id)?;
        // Before the check, whose hidden files are not the agent's work.
        let patch = match (&self.options.diffs, &task.workspace) {
            (Some(_), Some(space)) => Some(changes_since(&space.worktree, &space.base_commit)?),
            _ => None,
        };
        if out.error.is_none() {
            (out.passed, out.check_output) = self.check(case, &task.work_dir())?;
        }
        if let (Some(dir), Some(patch)) = (&self.options.diffs, patch) {
            out.diff = Some(save_diff(dir, out, &patch)?);
        }
        Ok(())
    }

    /// Answers cards and collects usage until the task stops. `resumed`: a
    /// follow-up was just sent, so a review seen before the task runs again
    /// is the last turn's. `cap`: the task is stopped after that many actions.
    pub(crate) fn drive(
        &self,
        id: &TaskId,
        timeout: Duration,
        resumed: bool,
        cap: Option<u32>,
    ) -> Observed {
        let mut seen = Observed::default();
        let mut running = !resumed;
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
                        TaskStatus::Running => {
                            running = true;
                            continue;
                        }
                        TaskStatus::Review if !running => continue,
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
            match &event {
                AgentEvent::Message { text } => seen.last_message = Some(text.clone()),
                AgentEvent::FileEdited { .. } if seen.reads_before_edit.is_none() => {
                    seen.reads_before_edit = Some(seen.files_read);
                }
                _ => {}
            }
            let answered = match &event {
                AgentEvent::PermissionRequest { request_id, .. } => {
                    self.hedy.answer_permission(id, request_id, Decision::Allow)
                }
                AgentEvent::Plan {
                    request_id: Some(request_id),
                    ..
                } => self.hedy.answer_plan(id, request_id, true, None),
                // A flow's approval: the eval approves, as it approves plans.
                AgentEvent::FlowApproval { .. } => {
                    self.hedy.answer_flow_approval(id, true, None).map(drop)
                }
                AgentEvent::Usage(sample) => {
                    seen.usage.add(sample);
                    if let Some(StepTrace::Role {
                        tokens, cost_usd, ..
                    }) = seen.steps.last_mut()
                    {
                        *tokens += sample.total_tokens();
                        if let Some(c) = sample.cost_usd {
                            *cost_usd = Some(cost_usd.unwrap_or(0.0) + c);
                        }
                    }
                    Ok(())
                }
                // A role of a flow starts: the step its usage goes to.
                AgentEvent::PhaseStarted { .. } => {
                    if let Some(run) = self.hedy.task(id).ok().and_then(|t| t.flow) {
                        let node = run.position.node.clone();
                        let role = match run.current().map(|n| &n.kind) {
                            Some(NodeKind::Role(seat)) => seat.role.clone(),
                            Some(NodeKind::Parallel { roles }) => roles
                                .get(run.lane)
                                .map(|s| s.role.clone())
                                .unwrap_or_default(),
                            _ => String::new(),
                        };
                        seen.steps.push(StepTrace::Role {
                            node,
                            role,
                            tokens: 0,
                            cost_usd: None,
                        });
                    }
                    Ok(())
                }
                AgentEvent::FlowCheck { node, what, passed } => {
                    seen.steps.push(StepTrace::Check {
                        node: node.clone(),
                        what: what.clone(),
                        passed: *passed,
                    });
                    Ok(())
                }
                AgentEvent::ToolUsed { tool, summary } if tool == "Read" => {
                    seen.actions += 1;
                    *seen.tools.entry("Read".into()).or_default() += 1;
                    seen.files_read += 1;
                    if !seen.read.insert(summary.clone()) {
                        seen.rereads += 1;
                    }
                    Ok(())
                }
                AgentEvent::ToolUsed { tool, .. } if tool == "Task" || tool == "Agent" => {
                    seen.actions += 1;
                    *seen.tools.entry(tool.clone()).or_default() += 1;
                    seen.subagents += 1;
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
                    let kind = match event {
                        AgentEvent::FileEdited { .. } => "Edit".to_owned(),
                        AgentEvent::CommandRun { command, .. }
                        | AgentEvent::TestsRun { command, .. } => {
                            let program = command.split_whitespace().next().unwrap_or("");
                            format!("Bash:{program}")
                        }
                        AgentEvent::ToolUsed { tool, .. } => tool.clone(),
                        _ => unreachable!(),
                    };
                    *seen.tools.entry(kind).or_default() += 1;
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
            if cap.is_some_and(|cap| seen.actions >= cap) && !seen.stopped {
                seen.stopped = true;
                let _ = self.hedy.stop_task(id);
            }
        }
    }

    /// Runs `command` on a copy of `work` (with `checks` added, if any), so
    /// `work` stays as it is.
    pub(crate) fn in_copy(
        &self,
        work: &Path,
        checks: Option<&Path>,
        command: &str,
    ) -> Result<(bool, String)> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let copy = self.options.work_dir.join(format!("copy-{stamp}"));
        let _copy = RemoveOnDrop(copy.clone());
        copy_dir(work, &copy)?;
        if let Some(checks) = checks.filter(|c| c.is_dir()) {
            copy_dir(checks, &copy)?;
        }
        run_command(&copy, command)
    }

    /// Copies the hidden checks into the worktree and runs the check command.
    fn check(&self, case: &Case, work_dir: &Path) -> Result<(bool, String)> {
        if case.checks().is_dir() {
            copy_dir(&case.checks(), work_dir)?;
        }
        run_command(work_dir, &case.spec.check)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_with(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = tmp.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        git(tmp.path(), &["init", "-q", "-b", "main"]).unwrap();
        git(tmp.path(), &["add", "-A"]).unwrap();
        git(tmp.path(), &["commit", "-q", "-m", "base"]).unwrap();
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let head = String::from_utf8(head.stdout).unwrap().trim().to_owned();
        (tmp, head)
    }

    #[test]
    fn a_runs_changes_include_new_and_removed_files_but_not_caches() {
        let (tmp, base) = repo_with(&[("app/a.py", "x = 1\n"), ("old.py", "gone\n")]);
        let dir = tmp.path();
        std::fs::write(dir.join("app/a.py"), "x = 2\n").unwrap();
        std::fs::remove_file(dir.join("old.py")).unwrap();
        std::fs::write(dir.join("new.py"), "y = 3\n").unwrap();
        std::fs::create_dir_all(dir.join("app/__pycache__")).unwrap();
        std::fs::write(dir.join("app/__pycache__/a.cpython-314.pyc"), [0u8, 1]).unwrap();
        // The agent's own commits count too.
        git(dir, &["add", "new.py"]).unwrap();
        git(dir, &["commit", "-q", "-m", "agent"]).unwrap();

        let patch = changes_since(dir, &base).unwrap();
        assert!(patch.contains("-x = 1\n+x = 2"), "{patch}");
        assert!(patch.contains("deleted file mode") && patch.contains("-gone"));
        assert!(patch.contains("+++ b/new.py") && patch.contains("+y = 3"));
        assert!(!patch.contains("__pycache__"), "{patch}");
        // The worktree's index is untouched: the edit is still unstaged.
        let staged = Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(staged.stdout.is_empty());

        // It applies on the base.
        git(dir, &["checkout", "-q", "-f", &base]).unwrap();
        git(dir, &["clean", "-qfdx"]).unwrap();
        std::fs::write(dir.join("p.patch"), &patch).unwrap();
        git(dir, &["apply", "p.patch"]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("new.py")).unwrap(),
            "y = 3\n"
        );
    }

    #[test]
    fn a_saved_diff_has_its_verdict_beside_it() {
        let tmp = tempfile::tempdir().unwrap();
        let run: RunResult = serde_json::from_value(serde_json::json!({
            "case": "parse-duration", "layer": "memory", "variant": "base+flow:tdd",
            "attempt": 2, "account": "eval", "model": "haiku", "passed": false,
            "tokens": 1, "costUsd": null, "durationMs": 1, "actions": 0, "notesRead": 0,
            "error": null, "checkOutput": "FAILED (failures=1)", "startedAt": 7
        }))
        .unwrap();
        let name = save_diff(tmp.path(), &run, "diff --git a/x b/x\n").unwrap();
        assert_eq!(name, "parse-duration-memory-base_flow_tdd-2-7.patch");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join(&name)).unwrap(),
            "diff --git a/x b/x\n"
        );
        let verdict: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                tmp.path()
                    .join("parse-duration-memory-base_flow_tdd-2-7.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(verdict["passed"], false);
        assert_eq!(verdict["checkOutput"], "FAILED (failures=1)");
        assert_eq!(verdict["variant"], "base+flow:tdd");
    }

    #[test]
    fn tail_keeps_the_end_on_a_char_boundary() {
        assert_eq!(tail("añoño", 3), "ño");
        assert_eq!(tail("ok", 100), "ok");
    }
}
