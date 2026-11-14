//! Tasks: a request worked on by an agent in its own worktree, with every tool
//! request passed through the permission rules (spec §4, "Flujo de una tarea").

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::ports::{AgentRequest, AgentSession, MergeOutcome, RateLimit, UsageRecord};
use super::{AppError, AppResult, Hedy};
use crate::domain::permissions::{self, AllowRule, Verdict};
use crate::domain::task::{SHARED_DEPENDENCY_DIRS, branch_name, title_from_prompt};
use crate::domain::{
    AgentEvent, Commit, Decision, FileDiff, LaunchPurpose, LaunchSpec, Provider, Task, TaskId,
    TaskMode, TaskStatus, Workspace, policy,
};

/// Worktrees live inside the repository, excluded locally from git.
const WORKTREES_DIR: &str = ".hedy/worktrees";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub project: PathBuf,
    pub prompt: String,
    pub mode: TaskMode,
    /// `None` lets the CLI choose ("auto").
    pub model: Option<String>,
    /// Propose a plan and wait for approval before changing anything.
    pub plan_first: bool,
}

/// A task whose agent process is running.
pub(super) struct LiveTask {
    session: Box<dyn AgentSession>,
    /// Remembered by "Permitir en este chat".
    rules: Vec<AllowRule>,
    /// Open requests, to remember their rule when the user answers.
    pending: HashMap<String, (String, Value)>,
}

fn blocked_patterns() -> Vec<String> {
    permissions::DEFAULT_BLOCKED
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Hedy {
    fn now_secs(&self) -> i64 {
        self.clock.now_ms() / 1000
    }

    pub fn task(&self, id: &TaskId) -> AppResult<Task> {
        self.store
            .task(id)?
            .ok_or_else(|| AppError::TaskNotFound(id.clone()))
    }

    pub fn tasks(&self, project: &Path) -> AppResult<Vec<Task>> {
        let project = self.fs.canonicalize(project)?;
        Ok(self.store.tasks(&project)?)
    }

    pub fn task_events(&self, id: &TaskId) -> AppResult<Vec<AgentEvent>> {
        Ok(self.store.events(id)?)
    }

    pub fn is_running(&self, id: &TaskId) -> bool {
        self.live.lock().unwrap().contains_key(id)
    }

    /// Creates the worktree (for `Project` mode) and starts the agent.
    pub fn start_task(&self, request: NewTask) -> AppResult<Task> {
        let project = self.project_at(&request.project)?;
        let account_id = project.account.clone().ok_or(AppError::NoAccountResolved)?;
        policy::check_launch(Some(&project), &account_id, &self.projects()?)?;
        let account = self.account(account_id.as_str())?;
        if account.provider != Provider::Claude {
            return Err(AppError::ProviderNotYetSupported(account.provider));
        }
        if request.prompt.trim().is_empty() {
            return Err(AppError::EmptyPrompt);
        }

        let now = self.clock.now_ms();
        let id = TaskId::new(format!("t{now:x}"));
        let title = title_from_prompt(&request.prompt);
        let workspace = if request.mode.changes_files() {
            let repo = &project.path;
            let base_branch = self.git.current_branch(repo)?;
            let base_commit = self.git.head_commit(repo)?;
            let branch = branch_name(&title, &id);
            let worktree = repo.join(WORKTREES_DIR).join(id.as_str());
            self.git.ensure_excluded(repo, "/.hedy/")?;
            self.git
                .add_worktree(repo, &worktree, &branch, &base_commit)?;
            self.share_dependencies(repo, &worktree)?;
            Some(Workspace {
                worktree,
                branch,
                base_branch,
                base_commit,
            })
        } else {
            None
        };

        let task = Task {
            id,
            project: project.path.clone(),
            mode: request.mode,
            title,
            prompt: request.prompt.clone(),
            account: account.id,
            model: request.model,
            workspace,
            session_id: None,
            status: TaskStatus::Running,
            created_at: now / 1000,
        };
        self.store.save_task(&task)?;
        self.record(
            &task.id,
            AgentEvent::UserMessage {
                text: request.prompt.clone(),
            },
        )?;
        self.spawn(&task, request.prompt, request.plan_first, None)?;
        self.events.task_updated(&task);
        Ok(task)
    }

    /// Links ignored dependency folders (node_modules, .venv...) into the worktree.
    fn share_dependencies(&self, repo: &Path, worktree: &Path) -> AppResult<()> {
        for name in SHARED_DEPENDENCY_DIRS {
            if self.fs.exists(&repo.join(name)) && self.git.is_ignored(repo, name) {
                // A symlink is not a directory to git: exclude it explicitly.
                self.git.ensure_excluded(repo, &format!("/{name}"))?;
                self.fs
                    .symlink_dir(&repo.join(name), &worktree.join(name))?;
            }
        }
        Ok(())
    }

    fn spawn(
        &self,
        task: &Task,
        prompt: String,
        plan_first: bool,
        resume: Option<String>,
    ) -> AppResult<()> {
        let account = self.account(task.account.as_str())?;
        let program = self.binary_for(account.provider)?;
        let launch =
            LaunchSpec::for_account(&account, LaunchPurpose::Run(vec![]), program.as_deref())?
                .expect("run is always launchable for subscription accounts")
                .in_dir(task.work_dir());

        let hedy = self.clone();
        let id = task.id.clone();
        let sink = std::sync::Arc::new(move |event| hedy.on_agent_event(&id, event));
        // Held while starting, so events that arrive early wait for the session.
        let mut live = self.live.lock().unwrap();
        let session = self.agents.start(
            AgentRequest {
                launch,
                mode: task.mode,
                plan_first: plan_first && task.mode.changes_files(),
                model: task.model.clone(),
                prompt,
                resume,
            },
            sink,
        )?;
        live.insert(
            task.id.clone(),
            LiveTask {
                session,
                rules: Vec::new(),
                pending: HashMap::new(),
            },
        );
        Ok(())
    }

    fn record(&self, id: &TaskId, event: AgentEvent) -> AppResult<()> {
        self.store.append_event(id, self.now_secs(), &event)?;
        self.events.event(id, &event);
        Ok(())
    }

    fn set_status(&self, task: &mut Task, status: TaskStatus) -> AppResult<()> {
        if task.status != status {
            task.status = status;
            self.store.save_task(task)?;
            self.events.task_updated(task);
        }
        Ok(())
    }

    /// Called on the adapter's thread for every event of a running task.
    fn on_agent_event(&self, id: &TaskId, event: AgentEvent) {
        if let Err(error) = self.handle_event(id, event) {
            let _ = self.record(
                id,
                AgentEvent::Error {
                    message: error.to_string(),
                },
            );
        }
    }

    fn handle_event(&self, id: &TaskId, event: AgentEvent) -> AppResult<()> {
        let mut task = self.task(id)?;
        match &event {
            AgentEvent::PermissionRequest {
                request_id,
                tool,
                input,
                ..
            } => {
                let mut live = self.live.lock().unwrap();
                let Some(running) = live.get_mut(id) else {
                    return Ok(());
                };
                let verdict = permissions::evaluate(
                    tool,
                    input,
                    &task.work_dir(),
                    &blocked_patterns(),
                    &running.rules,
                );
                let resolved = |decision, reason| AgentEvent::PermissionResolved {
                    request_id: request_id.clone(),
                    decision,
                    automatic: true,
                    reason,
                };
                match verdict {
                    Verdict::Allow => {
                        running.session.answer(request_id, true, None)?;
                        drop(live);
                        self.record(id, event.clone())?;
                        return self.record(
                            id,
                            resolved(Decision::Allow, Some("Permitido en este chat".into())),
                        );
                    }
                    Verdict::Deny(reason) => {
                        running.session.answer(request_id, false, Some(&reason))?;
                        drop(live);
                        self.record(id, event.clone())?;
                        return self.record(id, resolved(Decision::Deny, Some(reason)));
                    }
                    Verdict::Ask => {
                        running
                            .pending
                            .insert(request_id.clone(), (tool.clone(), input.clone()));
                        drop(live);
                        self.set_status(&mut task, TaskStatus::WaitingApproval)?;
                    }
                }
            }
            AgentEvent::Plan {
                request_id: Some(_),
                ..
            } => {
                self.set_status(&mut task, TaskStatus::WaitingApproval)?;
            }
            AgentEvent::SessionStarted { session_id, .. } => {
                task.session_id = Some(session_id.clone());
                self.store.save_task(&task)?;
            }
            AgentEvent::Finished { ok, .. } => {
                let status = if *ok {
                    TaskStatus::Review
                } else {
                    TaskStatus::Failed
                };
                self.set_status(&mut task, status)?;
            }
            AgentEvent::Usage(sample) => {
                self.store.record_usage(&UsageRecord {
                    at: self.now_secs(),
                    account: task.account.clone(),
                    project: task.project.clone(),
                    task: task.id.clone(),
                    sample: sample.clone(),
                })?;
            }
            AgentEvent::RateLimit {
                status,
                utilization,
                resets_at,
            } => {
                self.store.set_rate_limit(
                    &task.account,
                    &RateLimit {
                        status: status.clone(),
                        utilization: *utilization,
                        resets_at: *resets_at,
                        at: self.now_secs(),
                    },
                )?;
            }
            AgentEvent::Error { .. } if !self.is_running(id) => {
                self.set_status(&mut task, TaskStatus::Failed)?;
            }
            _ => {}
        }
        self.record(id, event)
    }

    fn with_session<T>(
        &self,
        id: &TaskId,
        f: impl FnOnce(&mut LiveTask) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut live = self.live.lock().unwrap();
        let running = live
            .get_mut(id)
            .ok_or_else(|| AppError::TaskNotRunning(id.clone()))?;
        f(running)
    }

    /// Answers a permission card.
    pub fn answer_permission(
        &self,
        id: &TaskId,
        request_id: &str,
        decision: Decision,
    ) -> AppResult<()> {
        self.with_session(id, |running| {
            running.session.answer(
                request_id,
                decision.allows(),
                (!decision.allows()).then_some("El usuario rechazó esta acción."),
            )?;
            if let Some((tool, input)) = running.pending.remove(request_id)
                && decision == Decision::AllowForTask
            {
                running.rules.push(permissions::rule_for(&tool, &input));
            }
            Ok(())
        })?;
        self.resolved(id, request_id, decision)
    }

    /// Approves the proposed plan, or sends it back with the user's feedback.
    pub fn answer_plan(
        &self,
        id: &TaskId,
        request_id: &str,
        approve: bool,
        feedback: Option<&str>,
    ) -> AppResult<()> {
        let message = feedback
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(|f| format!("El usuario pide cambios al plan: {f}"));
        self.with_session(id, |running| {
            running.session.answer(
                request_id,
                approve,
                if approve {
                    None
                } else {
                    Some(
                        message
                            .as_deref()
                            .unwrap_or("El usuario no aprobó el plan."),
                    )
                },
            )?;
            Ok(())
        })?;
        self.resolved(
            id,
            request_id,
            if approve {
                Decision::Allow
            } else {
                Decision::Deny
            },
        )
    }

    fn resolved(&self, id: &TaskId, request_id: &str, decision: Decision) -> AppResult<()> {
        self.record(
            id,
            AgentEvent::PermissionResolved {
                request_id: request_id.to_owned(),
                decision,
                automatic: false,
                reason: None,
            },
        )?;
        let mut task = self.task(id)?;
        self.set_status(&mut task, TaskStatus::Running)
    }

    /// A follow-up message. Restarts the agent (resuming its session) if needed.
    pub fn send_message(&self, id: &TaskId, text: &str) -> AppResult<()> {
        let mut task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        if text.trim().is_empty() {
            return Err(AppError::EmptyPrompt);
        }
        self.record(
            id,
            AgentEvent::UserMessage {
                text: text.to_owned(),
            },
        )?;
        let sent = {
            let mut live = self.live.lock().unwrap();
            match live.get_mut(id) {
                Some(running) => Some(running.session.send(text)),
                None => None,
            }
        };
        match sent {
            Some(Ok(())) => {}
            // The process is gone (crashed or stopped): resume its session.
            Some(Err(_)) | None => {
                self.live.lock().unwrap().remove(id);
                self.spawn(&task, text.to_owned(), false, task.session_id.clone())?;
            }
        }
        self.set_status(&mut task, TaskStatus::Running)
    }

    /// Stops the agent; the worktree stays for review.
    pub fn stop_task(&self, id: &TaskId) -> AppResult<()> {
        if let Some(mut running) = self.live.lock().unwrap().remove(id) {
            running.session.stop();
        }
        let mut task = self.task(id)?;
        if !task.status.is_closed() && task.status != TaskStatus::Failed {
            self.set_status(&mut task, TaskStatus::Review)?;
        }
        Ok(())
    }

    fn workspace(task: &Task) -> AppResult<&Workspace> {
        task.workspace
            .as_ref()
            .ok_or_else(|| AppError::TaskWithoutChanges(task.id.clone()))
    }

    /// Changes made by the task since it started.
    pub fn task_diff(&self, id: &TaskId) -> AppResult<Vec<FileDiff>> {
        let task = self.task(id)?;
        if task.status.is_closed() {
            return Ok(Vec::new());
        }
        let ws = Self::workspace(&task)?;
        Ok(self.git.diff(&ws.worktree, &ws.base_commit)?)
    }

    /// "Aceptar cambios": commit in the worktree and merge into the base branch.
    pub fn accept_task(&self, id: &TaskId) -> AppResult<Task> {
        let mut task = self.task(id)?;
        let ws = Self::workspace(&task)?.clone();
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        if let Some(mut running) = self.live.lock().unwrap().remove(id) {
            running.session.stop();
        }
        self.git.commit_all(&ws.worktree, &task.title)?;
        match self.git.merge(&task.project, &ws.base_branch, &ws.branch)? {
            MergeOutcome::Merged => {}
            blocked => {
                self.set_status(&mut task, TaskStatus::Review)?;
                return Err(AppError::MergeBlocked {
                    branch: ws.branch.clone(),
                    reason: blocked,
                });
            }
        }
        self.git
            .remove_worktree(&task.project, &ws.worktree, &ws.branch)?;
        self.set_status(&mut task, TaskStatus::Accepted)?;
        Ok(task)
    }

    /// "Descartar": stops the agent and deletes the worktree and its branch.
    pub fn discard_task(&self, id: &TaskId) -> AppResult<Task> {
        let mut task = self.task(id)?;
        if let Some(mut running) = self.live.lock().unwrap().remove(id) {
            running.session.stop();
        }
        if let Some(ws) = &task.workspace
            && !task.status.is_closed()
        {
            self.git
                .remove_worktree(&task.project, &ws.worktree, &ws.branch)?;
        }
        self.set_status(&mut task, TaskStatus::Discarded)?;
        Ok(task)
    }

    pub fn recent_commits(&self, project: &Path, limit: usize) -> AppResult<Vec<Commit>> {
        let project = self.fs.canonicalize(project)?;
        Ok(self.git.recent_commits(&project, limit)?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::application::testing::{AgentCall, Fixture};

    fn new_task(f: &Fixture, prompt: &str, plan_first: bool) -> Task {
        f.add_claude("Personal");
        f.fs.add_git_root("/code/moira");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        f.hedy
            .start_task(NewTask {
                project: f.dir("/code/moira"),
                prompt: prompt.into(),
                mode: TaskMode::Project,
                model: Some("opus".into()),
                plan_first,
            })
            .unwrap()
    }

    fn permission(id: &str, tool: &str, input: Value) -> AgentEvent {
        AgentEvent::PermissionRequest {
            request_id: id.into(),
            tool: tool.into(),
            summary: permissions::summarize(tool, &input),
            input,
        }
    }

    #[test]
    fn starts_in_a_new_worktree_with_the_project_account() {
        let f = Fixture::new();
        let task = new_task(&f, "Agrega recordatorios por WhatsApp.\nCon tests.", true);

        let ws = task.workspace.clone().unwrap();
        assert_eq!(task.title, "Agrega recordatorios por WhatsApp");
        assert!(ws.worktree.starts_with("/code/moira/.hedy/worktrees"));
        assert!(ws.branch.starts_with("hedy/agrega-recordatorios"));
        assert_eq!(
            (ws.base_branch.as_str(), ws.base_commit.as_str()),
            ("main", "abc123")
        );
        assert_eq!(f.git.excluded(), vec!["/.hedy/"]);

        let request = f.agents.last_request();
        assert_eq!(request.launch.cwd.as_ref(), Some(&ws.worktree));
        assert_eq!(request.launch.env[0].0, "CLAUDE_CONFIG_DIR");
        assert!(request.plan_first);
        assert_eq!(request.model.as_deref(), Some("opus"));
        assert_eq!(
            f.hedy.task_events(&task.id).unwrap()[0],
            AgentEvent::UserMessage {
                text: task.prompt.clone()
            }
        );
    }

    #[test]
    fn ignored_dependency_folders_are_shared_with_the_worktree() {
        let f = Fixture::new();
        f.fs.add_existing("/code/moira/node_modules");
        f.fs.add_existing("/code/moira/vendor");
        f.git.ignore("node_modules");
        let task = new_task(&f, "Hola", false);
        let wt = task.workspace.unwrap().worktree;
        assert_eq!(
            f.fs.symlinks(),
            vec![(f.dir("/code/moira/node_modules"), wt.join("node_modules"))]
        );
        assert_eq!(f.git.excluded(), vec!["/.hedy/", "/node_modules"]);
    }

    #[test]
    fn protected_files_and_outside_writes_are_denied_without_asking() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        f.agents
            .emit(permission("r1", "Read", json!({"file_path": ".env"})));
        f.agents.emit(permission(
            "r2",
            "Write",
            json!({"file_path": "/etc/hosts"}),
        ));

        assert_eq!(
            f.agents.calls(),
            vec![
                AgentCall::Answer("r1".into(), false),
                AgentCall::Answer("r2".into(), false),
            ]
        );
        let events = f.hedy.task_events(&task.id).unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::PermissionResolved {
                automatic: true,
                decision: Decision::Deny,
                ..
            }
        )));
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Running);
    }

    #[test]
    fn allow_for_task_remembers_the_rule() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        f.agents
            .emit(permission("r1", "Bash", json!({"command": "pytest -q"})));
        assert_eq!(
            f.hedy.task(&task.id).unwrap().status,
            TaskStatus::WaitingApproval
        );
        assert!(f.agents.calls().is_empty(), "waits for the user");

        f.hedy
            .answer_permission(&task.id, "r1", Decision::AllowForTask)
            .unwrap();
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Running);

        f.agents.emit(permission(
            "r2",
            "Bash",
            json!({"command": "pytest tests/x.py"}),
        ));
        f.agents
            .emit(permission("r3", "Bash", json!({"command": "rm -rf /"})));
        assert_eq!(
            f.agents.calls(),
            vec![
                AgentCall::Answer("r1".into(), true),
                AgentCall::Answer("r2".into(), true),
            ]
        );
        assert_eq!(
            f.hedy.task(&task.id).unwrap().status,
            TaskStatus::WaitingApproval
        );
    }

    #[test]
    fn plan_approval_and_feedback() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", true);
        f.agents.emit(AgentEvent::Plan {
            text: "1. Buscar".into(),
            request_id: Some("p1".into()),
        });
        assert_eq!(
            f.hedy.task(&task.id).unwrap().status,
            TaskStatus::WaitingApproval
        );
        f.hedy
            .answer_plan(&task.id, "p1", false, Some("usa Celery"))
            .unwrap();
        f.agents.emit(AgentEvent::Plan {
            text: "1. Buscar con Celery".into(),
            request_id: Some("p2".into()),
        });
        f.hedy.answer_plan(&task.id, "p2", true, None).unwrap();
        assert_eq!(
            f.agents.calls(),
            vec![
                AgentCall::Answer("p1".into(), false),
                AgentCall::Answer("p2".into(), true),
            ]
        );
        assert_eq!(
            f.agents.answer_messages(),
            vec![
                Some("El usuario pide cambios al plan: usa Celery".into()),
                None,
            ]
        );
    }

    #[test]
    fn finishing_records_usage_and_moves_to_review() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        f.agents.emit(AgentEvent::SessionStarted {
            session_id: "s-1".into(),
            model: "claude-opus".into(),
        });
        f.agents.emit(AgentEvent::Usage(crate::domain::UsageSample {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        }));
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: Some(1000),
        });
        let task = f.hedy.task(&task.id).unwrap();
        assert_eq!(task.status, TaskStatus::Review);
        assert_eq!(task.session_id.as_deref(), Some("s-1"));
        let usage = f.hedy.account_usage(&task.account).unwrap();
        assert_eq!(usage.week.total_tokens(), 150);
    }

    #[test]
    fn follow_ups_reuse_or_resume_the_session() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        f.agents.emit(AgentEvent::SessionStarted {
            session_id: "s-1".into(),
            model: "m".into(),
        });
        f.hedy.send_message(&task.id, "y los tests?").unwrap();
        assert_eq!(
            f.agents.calls(),
            vec![AgentCall::Send("y los tests?".into())]
        );

        f.hedy.stop_task(&task.id).unwrap();
        assert!(!f.hedy.is_running(&task.id));
        f.hedy.send_message(&task.id, "sigue").unwrap();
        let request = f.agents.last_request();
        assert_eq!(request.resume.as_deref(), Some("s-1"));
        assert_eq!(request.prompt, "sigue");
    }

    #[test]
    fn accept_commits_merges_and_cleans_up() {
        let f = Fixture::new();
        let task = new_task(&f, "Recordatorios", false);
        let accepted = f.hedy.accept_task(&task.id).unwrap();
        assert_eq!(accepted.status, TaskStatus::Accepted);
        let ws = task.workspace.unwrap();
        assert_eq!(
            f.git.log(),
            vec![
                format!("commit {} Recordatorios", ws.worktree.display()),
                format!("merge main <- {}", ws.branch),
                format!("remove {}", ws.branch),
            ]
        );
        assert!(matches!(
            f.hedy.send_message(&task.id, "x"),
            Err(AppError::TaskClosed(_))
        ));
    }

    #[test]
    fn blocked_merge_keeps_the_task_in_review() {
        let f = Fixture::new();
        let task = new_task(&f, "Recordatorios", false);
        f.git.set_merge_outcome(MergeOutcome::Dirty);
        let err = f.hedy.accept_task(&task.id).unwrap_err();
        assert!(matches!(
            err,
            AppError::MergeBlocked {
                reason: MergeOutcome::Dirty,
                ..
            }
        ));
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
    }

    #[test]
    fn discard_removes_the_worktree() {
        let f = Fixture::new();
        let task = new_task(&f, "Algo", false);
        f.hedy.discard_task(&task.id).unwrap();
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Discarded);
        assert!(
            f.git
                .log()
                .iter()
                .any(|l| l.starts_with("remove hedy/algo"))
        );
        assert!(f.agents.calls().contains(&AgentCall::Stop));
    }

    #[test]
    fn ask_mode_works_in_the_project_without_worktree() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        let task = f
            .hedy
            .start_task(NewTask {
                project: f.dir("/code/moira"),
                prompt: "¿Dónde se crean las citas?".into(),
                mode: TaskMode::Ask,
                model: None,
                plan_first: true,
            })
            .unwrap();
        assert!(task.workspace.is_none());
        let request = f.agents.last_request();
        assert_eq!(request.launch.cwd, Some(f.dir("/code/moira")));
        assert!(!request.plan_first, "plans only apply to changes");
        assert!(matches!(
            f.hedy.task_diff(&task.id),
            Err(AppError::TaskWithoutChanges(_))
        ));
    }

    #[test]
    fn only_claude_for_now() {
        let f = Fixture::new();
        f.hedy
            .add_account(crate::application::NewAccount {
                name: "ChatGPT".into(),
                provider: Provider::Codex,
                api_key: false,
                default_folder: false,
                color: None,
            })
            .unwrap();
        f.hedy
            .use_account(&f.dir("/code/x"), "chatgpt", None)
            .unwrap();
        let err = f
            .hedy
            .start_task(NewTask {
                project: f.dir("/code/x"),
                prompt: "hola".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
            })
            .unwrap_err();
        assert!(matches!(
            err,
            AppError::ProviderNotYetSupported(Provider::Codex)
        ));
    }
}
