//! Tasks: a request worked on by agents in its own worktree, phase by phase
//! (spec §4, "Flujo de una tarea"), with every tool request passed through the
//! permission rules.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde_json::Value;

use super::carry::Carry;
use super::ports::{AgentBackend, AgentRequest, AgentSession, RateLimit, Secret, UsageRecord};
use super::{AppError, AppResult, Hedy, NewImage, Setting};
use crate::domain::carry::carried_prompt;
use crate::domain::permissions::{self, AllowRule, Verdict};
use crate::domain::phase::pipeline;
use crate::domain::prompts;
use crate::domain::task::{SHARED_DEPENDENCY_DIRS, branch_name, title_from_prompt};
use crate::domain::{
    Account, AccountId, AgentEvent, Attachment, Commit, Credentials, Decision, FileDiff,
    HoldReason, LaunchPurpose, LaunchSpec, LimitLevel, Notice, NoticeKind, Phase, PromptImage,
    Provider, Stage, Task, TaskId, TaskMode, TaskStatus, UsageSample, Workspace, policy,
};
use crate::tr;

/// Worktrees live inside the repository, excluded locally from git.
const WORKTREES_DIR: &str = ".hedy/worktrees";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub project: PathBuf,
    pub prompt: String,
    pub mode: TaskMode,
    /// A single model for the whole task (project account). `None` follows the
    /// project's phases ("auto por fase").
    pub model: Option<String>,
    /// Propose a plan and wait for approval before changing anything.
    pub plan_first: bool,
    /// Images attached to the request.
    pub images: Vec<NewImage>,
}

/// A task whose agent process is running.
pub(super) struct LiveTask {
    session: Box<dyn AgentSession>,
    /// Events from other (replaced) sessions are ignored.
    number: u64,
    /// Remembered by "Permitir en este chat".
    rules: Vec<AllowRule>,
    /// Open requests, to remember their rule when the user answers.
    pub(super) pending: HashMap<String, (String, Value)>,
    /// Highest limit level already shown for this session.
    pub(super) warned: Option<LimitLevel>,
}

/// Tasks stored before phases existed have no stages.
pub(super) fn current_stage(task: &Task) -> Stage {
    task.stages.get(task.stage).cloned().unwrap_or(Stage {
        phase: Phase::Implement,
        account: task.account.clone(),
        model: task.model.clone(),
    })
}

impl Hedy {
    pub(super) fn now_secs(&self) -> i64 {
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

    /// The adapter and launch for an account, or why it cannot work yet.
    pub(super) fn backend(&self, account: &Account) -> AppResult<(AgentBackend, LaunchSpec)> {
        if account.credentials == Credentials::ApiKey {
            let key = self
                .secrets
                .api_key(&account.id)?
                .ok_or_else(|| AppError::MissingApiKey(account.id.clone()))?;
            let base_url = match self.setting(Setting::ApiUrl(account.provider))? {
                Some(url) => url,
                None => account
                    .provider
                    .api_base_url()
                    .ok_or(AppError::ProviderNotYetSupported(account.provider))?
                    .to_owned(),
            };
            let backend = AgentBackend::OpenAiCompatible {
                base_url,
                api_key: Secret::new(key),
            };
            return Ok((backend, LaunchSpec::program("", vec![])));
        }
        let provider = account.provider;
        let (backend, args) = match provider {
            Provider::Claude => (AgentBackend::ClaudeCode, vec![]),
            Provider::Codex => (AgentBackend::Codex, vec![]),
            _ => {
                let args = provider
                    .acp_args()
                    .ok_or(AppError::ProviderNotYetSupported(provider))?;
                let backend = AgentBackend::Acp {
                    name: provider.cli_name().to_owned(),
                    model_flag: provider.model_flag(),
                };
                (backend, args.iter().map(|a| a.to_string()).collect())
            }
        };
        let program = self.binary_for(provider)?;
        let launch =
            LaunchSpec::for_account(account, LaunchPurpose::Run(args), program.as_deref())?
                .expect("run is always launchable for subscription accounts");
        Ok((backend, launch))
    }

    /// Creates the worktree, works out the phases and starts the first one.
    pub fn start_task(&self, request: NewTask) -> AppResult<Task> {
        self.start_task_with(request, None)
    }

    /// `start_task`, optionally continuing a conversation: its context opens
    /// the prompt, before the user's instruction (see `carry`).
    pub(super) fn start_task_with(
        &self,
        request: NewTask,
        carry: Option<Carry>,
    ) -> AppResult<Task> {
        let project = self.project_at(&request.project)?;
        let main_account = project.account.clone().ok_or(AppError::NoAccountResolved)?;
        if request.prompt.trim().is_empty() {
            return Err(AppError::EmptyPrompt);
        }
        let stages = pipeline(
            &main_account,
            &project.phases,
            request.mode,
            request.plan_first,
            request.model.as_deref(),
        );
        // Every agent of the pipeline must be allowed here and ready to work.
        let projects = self.projects()?;
        // Agents that read without asking work in a worktree, without `.env`.
        let mut isolated = false;
        for (index, stage) in stages.iter().enumerate() {
            policy::check_launch(Some(&project), &stage.account, &projects)?;
            let account = self.account(stage.account.as_str())?;
            self.backend(&account)?;
            isolated |= account.provider.always_in_worktree();
            // The images go to whoever starts; later phases get them when
            // their agent takes them.
            if index == 0 {
                self.check_images(&request.images, &account)?;
            }
        }

        // A carried conversation is checked before anything is created.
        let carried = match carry {
            Some(carry) => {
                let first = self.account(stages[0].account.as_str())?;
                Some(self.prepare_carry(&carry, &first, request.images.len())?)
            }
            None => None,
        };

        let now = self.clock.now_ms();
        let id = self.new_task_id(now);
        let title = title_from_prompt(&request.prompt);
        let (prompt, origin, images) = match carried {
            Some(carried) => (
                carried_prompt(&carried.origin.context, &request.prompt),
                Some(carried.origin),
                [request.images, carried.images].concat(),
            ),
            None => (request.prompt, None, request.images),
        };
        // Codex and ACP agents read files without asking Hedy, so they always
        // work in a worktree, where `.env` and friends do not exist.
        let workspace = if request.mode.changes_files() || isolated {
            Some(self.create_workspace(&project.path, &title, &id)?)
        } else {
            None
        };
        let first = stages[0].clone();
        let mut task = Task {
            id,
            project: project.path.clone(),
            mode: request.mode,
            title,
            prompt: prompt.clone(),
            account: first.account,
            model: first.model,
            workspace,
            session_id: None,
            status: TaskStatus::Running,
            created_at: now / 1000,
            stages,
            stage: 0,
            plan: None,
            merge: None,
            wait: None,
            conversation: false,
            ship: None,
            origin,
            images: Vec::new(),
        };
        task.images = self.store_images(&task.id, &images)?;
        self.store.save_task(&task)?;
        self.record(
            &task.id,
            AgentEvent::UserMessage {
                text: prompt.clone(),
                images: task.images.clone(),
            },
        )?;
        self.run_stage(&mut task, 0, prompt, None)?;
        self.events.task_updated(&task);
        Ok(task)
    }

    /// A new, unique task id. The counter keeps ids unique when two tasks
    /// start in the same millisecond.
    pub(super) fn new_task_id(&self, now_ms: i64) -> TaskId {
        let seq = self.sessions.fetch_add(1, Ordering::Relaxed) % 256;
        TaskId::new(format!("t{now_ms:x}{seq:02x}"))
    }

    fn create_workspace(&self, repo: &Path, title: &str, id: &TaskId) -> AppResult<Workspace> {
        let base_branch = self.git.current_branch(repo)?;
        let base_commit = self.git.head_commit(repo)?;
        let branch = branch_name(title, id);
        let worktree = repo.join(WORKTREES_DIR).join(id.as_str());
        self.git.ensure_excluded(repo, "/.hedy/")?;
        self.git
            .add_worktree(repo, &worktree, &branch, &base_commit)?;
        self.share_dependencies(repo, &worktree)?;
        self.install_skills_in(repo, &worktree)?;
        Ok(Workspace {
            worktree,
            branch,
            base_branch,
            base_commit,
        })
    }

    /// Links ignored dependency folders (node_modules, .venv...) into the worktree.
    pub(super) fn share_dependencies(&self, repo: &Path, worktree: &Path) -> AppResult<()> {
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

    /// Starts stage `index` in a new agent session. If it cannot start, the
    /// task fails with the reason in its history instead of staying "running".
    pub(super) fn run_stage(
        &self,
        task: &mut Task,
        index: usize,
        prompt: String,
        resume: Option<String>,
    ) -> AppResult<()> {
        let result = self.start_stage(task, index, prompt, resume);
        if let Err(error) = &result {
            self.fail(task, error);
        }
        result
    }

    fn fail(&self, task: &mut Task, error: &AppError) {
        self.stop_session(&task.id);
        self.hold_queue(&task.id, HoldReason::Failed);
        let _ = self.record(
            &task.id,
            AgentEvent::Error {
                message: error.to_string(),
            },
        );
        let failed_now = task.status != TaskStatus::Failed;
        let _ = self.set_status(task, TaskStatus::Failed);
        if failed_now {
            self.notify(task, NoticeKind::Failed);
        }
    }

    /// Tells the driving adapter about something the user may want to hear
    /// about, unless notifications are off. Only names the agent and the
    /// project: never file contents, commands or the prompt.
    pub(super) fn notify(&self, task: &Task, kind: NoticeKind) {
        if !self.notifications_on().unwrap_or(true) {
            return;
        }
        let agent = self
            .account(task.account.as_str())
            .map(|a| a.provider.agent_label(None))
            .unwrap_or_else(|_| "Hedy".into());
        let project = self.place_name(task);
        self.events
            .notice(task, &Notice::new(kind, &agent, &project));
    }

    /// Where the task belongs, for people: its project's name, or
    /// "Conversación" for a question outside any project.
    pub(super) fn place_name(&self, task: &Task) -> String {
        if task.conversation {
            tr!("Conversación", "Conversation")
        } else {
            self.project_name(&task.project)
        }
    }

    /// The project's name, or its folder's when it is no longer registered.
    pub(super) fn project_name(&self, path: &Path) -> String {
        self.project_at(path).map(|p| p.name).unwrap_or_else(|_| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        })
    }

    fn start_stage(
        &self,
        task: &mut Task,
        index: usize,
        prompt: String,
        resume: Option<String>,
    ) -> AppResult<()> {
        let stage = task
            .stages
            .get(index)
            .cloned()
            .unwrap_or_else(|| current_stage(task));
        let account = self.account(stage.account.as_str())?;
        task.stage = index;
        task.account = account.id.clone();
        task.model = stage.model.clone();
        self.store.save_task(task)?;
        self.record(
            &task.id,
            AgentEvent::PhaseStarted {
                phase: stage.phase,
                agent: account.provider.agent_label(stage.model.as_deref()),
                account: account.name.clone(),
            },
        )?;
        // Every phase sees the images of the request.
        let images = self.prompt_images(&task.id, &task.images);
        self.spawn(task, &account, &stage, prompt, images, resume)
    }

    fn spawn(
        &self,
        task: &Task,
        account: &Account,
        stage: &Stage,
        mut prompt: String,
        mut images: Vec<PromptImage>,
        resume: Option<String>,
    ) -> AppResult<()> {
        let (backend, launch) = self.backend(account)?;
        // An agent that takes no images (a later phase, a handoff) is told
        // they exist instead of getting them.
        if !images.is_empty() && !self.agents.image_support(&backend).accepts() {
            let count = images.len();
            prompt.push_str("\n\n");
            prompt.push_str(&tr!(
                "(El usuario adjuntó {count} imagen(es) que este agente no puede ver.)",
                "(The user attached {count} image(s) this agent cannot see.)"
            ));
            images.clear();
        }
        // APIs need an explicit model; CLIs pick their own when none is set.
        let model = match backend {
            AgentBackend::OpenAiCompatible { .. } => stage
                .model
                .clone()
                .or_else(|| account.provider.default_api_model().map(str::to_owned)),
            _ => stage.model.clone(),
        };
        // A conversation has no project: no memory and nothing to map.
        let project = if task.conversation {
            None
        } else {
            self.project_at(&task.project).ok()
        };
        let memory = project
            .as_ref()
            .and_then(|p| self.memory_link(p, &task.id, account));
        let mcp = if task.conversation {
            Vec::new()
        } else {
            self.code_map_servers(&task.project)?
        };
        let number = self.sessions.fetch_add(1, Ordering::Relaxed) + 1;
        let hedy = self.clone();
        let id = task.id.clone();
        // Tokens belong to this session's account, even after a handoff.
        let spender = account.id.clone();
        let sink = Arc::new(move |event| hedy.on_agent_event(&id, number, &spender, event));
        // Held while starting, so events that arrive early wait for the session.
        let mut live = self.live.lock().unwrap();
        if let Some(mut old) = live.remove(&task.id) {
            old.session.stop();
        }
        let session = self.agents.start(
            AgentRequest {
                backend,
                launch: launch.in_dir(task.work_dir()),
                mode: task.mode,
                phase: stage.phase,
                plan_first: stage.phase == Phase::Plan,
                model,
                prompt,
                images,
                resume,
                memory,
                mcp,
            },
            sink,
        )?;
        live.insert(
            task.id.clone(),
            LiveTask {
                session,
                number,
                rules: Vec::new(),
                pending: HashMap::new(),
                warned: None,
            },
        );
        Ok(())
    }

    pub(super) fn record(&self, id: &TaskId, event: AgentEvent) -> AppResult<()> {
        if !matches!(event, AgentEvent::ToolStarted { .. }) {
            self.store.append_event(id, self.now_secs(), &event)?;
        }
        self.events.event(id, &event);
        Ok(())
    }

    pub(super) fn set_status(&self, task: &mut Task, status: TaskStatus) -> AppResult<()> {
        if task.status != status {
            task.status = status;
            self.store.save_task(task)?;
            self.events.task_updated(task);
        }
        Ok(())
    }

    pub(super) fn stop_session(&self, id: &TaskId) {
        if let Some(mut running) = self.live.lock().unwrap().remove(id) {
            running.session.stop();
        }
    }

    /// Called on the adapter's thread for every event of a session.
    fn on_agent_event(&self, id: &TaskId, number: u64, spender: &AccountId, event: AgentEvent) {
        if let AgentEvent::Usage(sample) = &event {
            // Recorded even from a replaced session: the tokens were spent.
            if let Err(error) = self.handle_usage(id, spender, sample) {
                let _ = self.record(
                    id,
                    AgentEvent::Error {
                        message: error.to_string(),
                    },
                );
            }
        }
        let current = self.live.lock().unwrap().get(id).map(|l| l.number) == Some(number);
        if !current {
            return;
        }
        if let Err(error) = self.handle_event(id, event)
            && let Ok(mut task) = self.task(id)
            && task.status != TaskStatus::Failed
        {
            self.fail(&mut task, &error);
        }
    }

    pub(super) fn handle_usage(
        &self,
        id: &TaskId,
        spender: &AccountId,
        sample: &UsageSample,
    ) -> AppResult<()> {
        let task = self.task(id)?;
        let mut sample = sample.clone();
        // API agents report tokens only; the cost comes from the user's price.
        if sample.cost_usd.is_none()
            && let Ok(account) = self.account(spender.as_str())
            && matches!(account.credentials, Credentials::ApiKey)
            && let Some(price) = self.api_price(account.provider)?
        {
            sample.cost_usd = Some(price.cost(&sample));
        }
        self.store.record_usage(&UsageRecord {
            at: self.now_secs(),
            account: spender.clone(),
            project: task.project.clone(),
            task: task.id.clone(),
            sample,
        })?;
        Ok(())
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
                let verdict = permissions::evaluate(tool, input, &task.work_dir(), &running.rules);
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
                        let waiting = task.status == TaskStatus::WaitingApproval;
                        self.set_status(&mut task, TaskStatus::WaitingApproval)?;
                        if !waiting {
                            self.notify(&task, NoticeKind::PermissionNeeded);
                        }
                    }
                }
            }
            AgentEvent::Plan {
                text,
                request_id: Some(_),
            } => {
                task.plan = Some(text.clone());
                self.store.save_task(&task)?;
                let waiting = task.status == TaskStatus::WaitingApproval;
                self.set_status(&mut task, TaskStatus::WaitingApproval)?;
                if !waiting {
                    self.notify(&task, NoticeKind::PlanReady);
                }
            }
            AgentEvent::SessionStarted { session_id, .. } => {
                task.session_id = Some(session_id.clone());
                self.store.save_task(&task)?;
            }
            AgentEvent::Finished { ok, summary, .. } => {
                self.record(id, event.clone())?;
                // Messages written during the turn go first, to the same
                // session; after a failure or a limit they wait for the user.
                let limited = self.limit_reached_now(id);
                if !*ok || limited {
                    let reason = if limited {
                        HoldReason::Limit
                    } else {
                        HoldReason::Failed
                    };
                    self.hold_queue(id, reason);
                } else if self.deliver_queue_at_turn_end(&mut task)? {
                    return Ok(());
                }
                let next = task.stage + 1;
                let advances =
                    *ok && current_stage(&task).phase != Phase::Plan && next < task.stages.len();
                if advances {
                    // The next phase starts in a new session with the context so far.
                    self.stop_session(id);
                    let base = task
                        .workspace
                        .as_ref()
                        .map(|w| w.base_commit.clone())
                        .unwrap_or_default();
                    let prompt = prompts::stage_prompt(
                        task.stages[next].phase,
                        &task.prompt,
                        task.plan.as_deref(),
                        &base,
                        Some(summary),
                    );
                    return self.run_stage(&mut task, next, prompt, None);
                }
                let (status, notice) = if *ok {
                    let changes = task.mode.changes_files();
                    (TaskStatus::Review, NoticeKind::Finished { changes })
                } else {
                    (TaskStatus::Failed, NoticeKind::Failed)
                };
                let changed = task.status != status;
                self.set_status(&mut task, status)?;
                if changed {
                    self.notify(&task, notice);
                }
                return Ok(());
            }
            AgentEvent::Usage(_) => {}
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
                self.warn_about_limit(&task, status, *utilization, *resets_at)?;
            }
            _ => {}
        }
        self.record(id, event)
    }

    /// Shows a limit card once per level and session. Never switches accounts.
    fn warn_about_limit(
        &self,
        task: &Task,
        status: &str,
        percent: Option<f64>,
        resets_at: Option<i64>,
    ) -> AppResult<()> {
        let Some(level) = LimitLevel::from_report(status, percent) else {
            return Ok(());
        };
        {
            let mut live = self.live.lock().unwrap();
            let Some(running) = live.get_mut(&task.id) else {
                return Ok(());
            };
            if running.warned == Some(LimitLevel::Reached) || running.warned == Some(level) {
                return Ok(());
            }
            running.warned = Some(level);
        }
        self.record(
            &task.id,
            AgentEvent::LimitWarning {
                account: task.account.to_string(),
                level,
                percent,
                resets_at,
            },
        )?;
        if level == LimitLevel::Reached {
            self.hold_queue(&task.id, HoldReason::Limit);
            self.notify(task, NoticeKind::LimitReached);
        }
        Ok(())
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
        let rejected = tr!(
            "El usuario rechazó esta acción.",
            "The user rejected this action."
        );
        let waiting = self.with_session(id, |running| {
            running.session.answer(
                request_id,
                decision.allows(),
                (!decision.allows()).then_some(rejected.as_str()),
            )?;
            // Chained shell commands are allowed once but never remembered.
            if let Some((tool, input)) = running.pending.remove(request_id)
                && decision == Decision::AllowForTask
                && let Some(rule) = permissions::rule_for(&tool, &input)
            {
                running.rules.push(rule);
            }
            Ok(!running.pending.is_empty())
        })?;
        self.resolved(id, request_id, decision, waiting)
    }

    /// Approves the proposed plan, or sends it back with the user's feedback.
    /// An approved plan goes on with the same agent when it also implements;
    /// otherwise the implementer starts with the plan as context.
    pub fn answer_plan(
        &self,
        id: &TaskId,
        request_id: &str,
        approve: bool,
        feedback: Option<&str>,
    ) -> AppResult<()> {
        let mut task = self.task(id)?;
        if !approve {
            let message = feedback
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .map(|f| {
                    tr!(
                        "El usuario pide cambios al plan: {f}",
                        "The user asks for changes to the plan: {f}"
                    )
                })
                .unwrap_or_else(|| {
                    tr!(
                        "El usuario no aprobó el plan.",
                        "The user did not approve the plan."
                    )
                });
            self.with_session(id, |running| {
                Ok(running.session.answer(request_id, false, Some(&message))?)
            })?;
            return self.resolved(id, request_id, Decision::Deny, false);
        }

        let here = current_stage(&task);
        let next = task.stage + 1;
        let handed_over = here.phase == Phase::Plan
            && task.stages.get(next).is_some_and(|n| !n.same_agent(&here));
        self.with_session(id, |running| {
            if handed_over {
                Ok(running
                    .session
                    .answer(request_id, false, Some(&prompts::plan_handed_off()))?)
            } else {
                Ok(running.session.answer(request_id, true, None)?)
            }
        })?;
        self.resolved(id, request_id, Decision::Allow, false)?;
        if here.phase != Phase::Plan || next >= task.stages.len() {
            return Ok(());
        }
        task = self.task(id)?;
        if handed_over {
            self.stop_session(id);
            let base = task
                .workspace
                .as_ref()
                .map(|w| w.base_commit.clone())
                .unwrap_or_default();
            let prompt = prompts::stage_prompt(
                Phase::Implement,
                &task.prompt,
                task.plan.as_deref(),
                &base,
                None,
            );
            return self.run_stage(&mut task, next, prompt, None);
        }
        // Same agent: it goes on implementing in its session.
        let account = self.account(task.stages[next].account.as_str())?;
        task.stage = next;
        self.store.save_task(&task)?;
        self.record(
            id,
            AgentEvent::PhaseStarted {
                phase: task.stages[next].phase,
                agent: account
                    .provider
                    .agent_label(task.stages[next].model.as_deref()),
                account: account.name,
            },
        )
    }

    /// Records the user's answer. The task runs again unless other cards
    /// (parallel tool calls) still wait.
    fn resolved(
        &self,
        id: &TaskId,
        request_id: &str,
        decision: Decision,
        others_waiting: bool,
    ) -> AppResult<()> {
        self.record(
            id,
            AgentEvent::PermissionResolved {
                request_id: request_id.to_owned(),
                decision,
                automatic: false,
                reason: None,
            },
        )?;
        if others_waiting {
            return Ok(());
        }
        let mut task = self.task(id)?;
        self.set_status(&mut task, TaskStatus::Running)
    }

    /// A follow-up message: it goes now when the agent is idle, or waits in
    /// the queue until its turn ends (see [`Hedy::queue_message`]).
    pub fn send_message(&self, id: &TaskId, text: &str) -> AppResult<()> {
        self.queue_message(id, text).map(drop)
    }

    /// Sends a user turn now, with the images already stored for it.
    /// Restarts the agent (resuming its session) if needed.
    pub(super) fn deliver(
        &self,
        task: &mut Task,
        text: &str,
        attachments: Vec<Attachment>,
    ) -> AppResult<()> {
        let id = &task.id.clone();
        // The user (or the renewed limit) sets the task going: no more waiting.
        if task.wait.take().is_some() {
            self.store.save_task(task)?;
            self.events.task_updated(task);
        }
        let images = self.prompt_images(id, &attachments);
        self.record(
            id,
            AgentEvent::UserMessage {
                text: text.to_owned(),
                images: attachments,
            },
        )?;
        let sent = {
            let mut live = self.live.lock().unwrap();
            live.get_mut(id)
                .map(|running| running.session.send(text, &images))
        };
        match sent {
            Some(Ok(())) => {}
            // The process is gone (crashed or stopped): resume its session.
            Some(Err(_)) | None => {
                self.live.lock().unwrap().remove(id);
                let stage = current_stage(task);
                let account = self.account(stage.account.as_str())?;
                self.spawn(
                    task,
                    &account,
                    &stage,
                    text.to_owned(),
                    images,
                    task.session_id.clone(),
                )?;
            }
        }
        self.set_status(task, TaskStatus::Running)
    }

    /// Who the task may be handed over to (another provider or an API key).
    pub fn handoff_options(&self, id: &TaskId) -> AppResult<Vec<Account>> {
        let task = self.task(id)?;
        let project = self.task_project(&task)?;
        let current = self.account(task.account.as_str())?;
        let accounts = self.accounts()?;
        let projects = self.projects()?;
        Ok(
            policy::handoff_candidates(&current, &project, &accounts, &projects)
                .into_iter()
                .cloned()
                .collect(),
        )
    }

    /// Moves the task to another agent, which continues from the current state.
    /// Always the user's decision (spec rule 3).
    pub fn handoff_task(
        &self,
        id: &TaskId,
        account: &str,
        model: Option<String>,
    ) -> AppResult<Task> {
        let mut task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        let target = self
            .handoff_options(id)?
            .into_iter()
            .find(|a| a.id.as_str() == account)
            .ok_or_else(|| AppError::NotAHandoffCandidate(account.to_owned()))?;
        self.backend(&target)?;
        self.stop_session(id);

        let previous = self.account(task.account.as_str())?;
        let previous_label = previous.provider.agent_label(task.model.as_deref());
        let changed: Vec<String> = match &task.workspace {
            Some(ws) => self
                .git
                .diff(&ws.worktree, &ws.base_commit)?
                .into_iter()
                .map(|f| f.path)
                .collect(),
            None => Vec::new(),
        };
        if task.stages.is_empty() {
            task.stages.push(current_stage(&task));
        }
        // The exhausted account leaves every remaining phase it had.
        for stage in task.stages.iter_mut().skip(task.stage) {
            if stage.account == previous.id {
                stage.account = target.id.clone();
                stage.model = model.clone();
            }
        }
        let prompt = prompts::handoff_prompt(
            &task.prompt,
            task.plan.as_deref(),
            &previous_label,
            &changed,
        );
        let index = task.stage;
        // Waiting for the old account's limit makes no sense any more.
        task.wait = None;
        self.run_stage(&mut task, index, prompt, None)?;
        self.set_status(&mut task, TaskStatus::Running)?;
        Ok(task)
    }

    /// Stops the agent; the worktree stays for review.
    pub fn stop_task(&self, id: &TaskId) -> AppResult<()> {
        self.stop_session(id);
        self.hold_queue(id, HoldReason::Stopped);
        let mut task = self.task(id)?;
        if !task.status.is_closed() && task.status != TaskStatus::Failed {
            self.set_status(&mut task, TaskStatus::Review)?;
        }
        Ok(())
    }

    pub(super) fn workspace(task: &Task) -> AppResult<&Workspace> {
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

    /// "Aceptar cambios" with the proposed commit message (see
    /// [`Hedy::accept_task_with`]).
    pub fn accept_task(&self, id: &TaskId) -> AppResult<Task> {
        self.accept_task_with(id, None)
    }

    /// "Descartar": stops the agent and deletes the worktree and its branch.
    pub fn discard_task(&self, id: &TaskId) -> AppResult<Task> {
        let mut task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        self.stop_session(id);
        if let Some(ws) = &task.workspace {
            self.git
                .remove_worktree(&task.project, &ws.worktree, &ws.branch)?;
        }
        self.set_status(&mut task, TaskStatus::Discarded)?;
        self.clear_queue(id)?;
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
    use crate::application::ports::MergeOutcome;
    use crate::application::testing::{AgentCall, Fixture};

    fn new_task(f: &Fixture, prompt: &str, plan_first: bool) -> Task {
        f.add_claude("Personal");
        f.fs.add_git_root("/code/moira");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
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
                images: Vec::new(),
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
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: None,
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
                images: Vec::new(),
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

    /// Claude plans, Codex implements, Kimi writes tests.
    fn phased(f: &Fixture) -> Task {
        use crate::domain::{Assignment, PhasePlan};
        f.add_claude("Personal");
        f.add_codex("ChatGPT");
        f.add_kimi("Kimi");
        f.hedy.set_api_key("kimi", "sk").unwrap();
        f.fs.add_git_root("/code/moira");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        let assign = |account: &str, model: Option<&str>| Assignment {
            account: crate::domain::AccountId::new(account).unwrap(),
            model: model.map(str::to_owned),
        };
        f.hedy
            .set_project_phases(
                &f.dir("/code/moira"),
                PhasePlan::from([
                    (Phase::Plan, assign("personal", Some("opus"))),
                    (Phase::Implement, assign("chatgpt", None)),
                    (Phase::Tests, assign("kimi", Some("kimi-k3"))),
                ]),
            )
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/moira"),
                prompt: "Recordatorios".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: true,
            })
            .unwrap()
    }

    fn phases_started(f: &Fixture, task: &Task) -> Vec<(Phase, String)> {
        f.hedy
            .task_events(&task.id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e {
                AgentEvent::PhaseStarted { phase, agent, .. } => Some((phase, agent)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn phases_hand_the_work_from_agent_to_agent() {
        let f = Fixture::new();
        let task = phased(&f);
        assert!(matches!(
            f.agents.last_request().backend,
            AgentBackend::ClaudeCode
        ));
        assert!(f.agents.last_request().plan_first);

        // Claude plans; on approval it is told to stop and Codex implements.
        f.agents.emit(AgentEvent::Plan {
            text: "1. Programar".into(),
            request_id: Some("p1".into()),
        });
        f.hedy.answer_plan(&task.id, "p1", true, None).unwrap();
        assert_eq!(f.agents.calls()[0], AgentCall::Answer("p1".into(), false));
        assert_eq!(
            f.agents.answer_messages()[0].as_deref(),
            Some(prompts::plan_handed_off().as_str())
        );
        let codex = f.agents.last_request();
        assert!(matches!(codex.backend, AgentBackend::Codex));
        assert_eq!(codex.phase, Phase::Implement);
        assert!(codex.prompt.contains("## Plan aprobado\n1. Programar"));
        assert_eq!(
            f.hedy.task(&task.id).unwrap().plan.as_deref(),
            Some("1. Programar")
        );

        // Codex finishes: Kimi writes the tests with the context so far.
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Codex implementó X".into(),
            duration_ms: None,
        });
        let kimi = f.agents.last_request();
        assert!(matches!(
            kimi.backend,
            AgentBackend::OpenAiCompatible { .. }
        ));
        assert_eq!(
            (kimi.phase, kimi.model.as_deref()),
            (Phase::Tests, Some("kimi-k3"))
        );
        assert!(
            kimi.prompt.contains("Codex implementó X") && kimi.prompt.contains("commit abc123")
        );

        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "6 tests".into(),
            duration_ms: None,
        });
        let task = f.hedy.task(&task.id).unwrap();
        assert_eq!(task.status, TaskStatus::Review);
        assert_eq!(
            phases_started(&f, &task),
            vec![
                (Phase::Plan, "Claude Opus".into()),
                (Phase::Implement, "Codex".into()),
                (Phase::Tests, "Kimi".into()),
            ]
        );
    }

    #[test]
    fn the_same_agent_plans_and_implements_in_one_session() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", true);
        f.agents.emit(AgentEvent::Plan {
            text: "1.".into(),
            request_id: Some("p1".into()),
        });
        f.hedy.answer_plan(&task.id, "p1", true, None).unwrap();
        assert_eq!(f.agents.calls(), vec![AgentCall::Answer("p1".into(), true)]);
        assert_eq!(f.agents.requests().len(), 1, "no new session");
        assert_eq!(f.hedy.task(&task.id).unwrap().stage, 1);
    }

    #[test]
    fn events_from_a_replaced_session_are_ignored() {
        let f = Fixture::new();
        let task = phased(&f);
        f.agents.emit(AgentEvent::Plan {
            text: "1.".into(),
            request_id: Some("p1".into()),
        });
        f.hedy.answer_plan(&task.id, "p1", true, None).unwrap();
        // Claude's turn ends after being told to stop; Codex keeps working.
        f.agents.emit_from(
            0,
            AgentEvent::Finished {
                ok: true,
                summary: "".into(),
                duration_ms: None,
            },
        );
        let task = f.hedy.task(&task.id).unwrap();
        assert_eq!((task.stage, task.status), (1, TaskStatus::Running));
        assert_eq!(f.agents.requests().len(), 2);
    }

    #[test]
    fn limits_warn_once_and_never_switch_accounts() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        let limit = |status: &str| AgentEvent::RateLimit {
            status: status.into(),
            utilization: Some(93.0),
            resets_at: Some(99),
        };
        f.agents.emit(limit("allowed_warning"));
        f.agents.emit(limit("allowed_warning"));
        f.agents.emit(limit("rejected"));
        let warnings: Vec<_> = f
            .hedy
            .task_events(&task.id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e {
                AgentEvent::LimitWarning { level, .. } => Some(level),
                _ => None,
            })
            .collect();
        assert_eq!(warnings, vec![LimitLevel::Warning, LimitLevel::Reached]);
        assert_eq!(f.agents.requests().len(), 1, "no automatic switch");
    }

    #[test]
    fn a_stage_that_cannot_start_fails_the_task() {
        let f = Fixture::new();
        let task = phased(&f);
        f.agents.emit(AgentEvent::Plan {
            text: "1. Hacer".into(),
            request_id: Some("p1".into()),
        });
        f.agents.fail_next_start();
        assert!(f.hedy.answer_plan(&task.id, "p1", true, None).is_err());
        let task = f.hedy.task(&task.id).unwrap();
        assert_eq!(task.status, TaskStatus::Failed, "not stuck as running");
        assert!(!f.hedy.is_running(&task.id));
        assert!(f.hedy.task_events(&task.id).unwrap().iter().any(
            |e| matches!(e, AgentEvent::Error { message } if message.contains("no se encontró"))
        ));
    }

    #[test]
    fn late_usage_is_billed_to_the_session_that_spent_it() {
        use crate::domain::UsageSample;
        let f = Fixture::new();
        let task = phased(&f);
        f.hedy.handoff_task(&task.id, "kimi", None).unwrap();
        // The replaced Claude session reports its last tokens.
        f.agents.emit_from(
            0,
            AgentEvent::Usage(UsageSample {
                input_tokens: 70,
                ..UsageSample::default()
            }),
        );
        let personal = f.hedy.account("personal").unwrap().id;
        let kimi = f.hedy.account("kimi").unwrap().id;
        assert_eq!(
            f.hedy.account_usage(&personal).unwrap().week.total_tokens(),
            70
        );
        assert_eq!(f.hedy.account_usage(&kimi).unwrap().week.total_tokens(), 0);
    }

    #[test]
    fn a_task_keeps_waiting_while_other_cards_are_open() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        f.agents
            .emit(permission("p1", "Bash", json!({"command": "pytest"})));
        f.agents
            .emit(permission("p2", "Bash", json!({"command": "ls"})));
        f.hedy
            .answer_permission(&task.id, "p1", Decision::Allow)
            .unwrap();
        assert_eq!(
            f.hedy.task(&task.id).unwrap().status,
            TaskStatus::WaitingApproval
        );
        f.hedy
            .answer_permission(&task.id, "p2", Decision::Allow)
            .unwrap();
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Running);
    }

    #[test]
    fn closed_tasks_cannot_be_discarded_and_phase_accounts_stay() {
        let f = Fixture::new();
        let task = phased(&f);
        assert!(
            matches!(
                f.hedy.remove_account("kimi"),
                Err(AppError::AccountInUse { .. })
            ),
            "kimi writes the tests of the project"
        );
        f.hedy.discard_task(&task.id).unwrap();
        assert!(matches!(
            f.hedy.discard_task(&task.id),
            Err(AppError::TaskClosed(_))
        ));
    }

    #[test]
    fn handoff_goes_to_another_provider_with_the_context() {
        let f = Fixture::new();
        let task = phased(&f);
        f.add_claude("Otra Claude");
        let options: Vec<_> = f
            .hedy
            .handoff_options(&task.id)
            .unwrap()
            .into_iter()
            .map(|a| a.id.to_string())
            .collect();
        assert_eq!(
            options,
            vec!["chatgpt", "kimi"],
            "never another Claude subscription"
        );
        assert!(matches!(
            f.hedy.handoff_task(&task.id, "otra-claude", None),
            Err(AppError::NotAHandoffCandidate(_))
        ));

        f.agents.emit(AgentEvent::Plan {
            text: "1. Hacer".into(),
            request_id: Some("p1".into()),
        });
        let moved = f
            .hedy
            .handoff_task(&task.id, "kimi", Some("kimi-k3".into()))
            .unwrap();
        assert_eq!((moved.stage, moved.account.as_str()), (0, "kimi"));
        let request = f.agents.last_request();
        assert!(request.prompt.contains("Claude Opus estaba trabajando"));
        assert!(request.prompt.contains("1. Hacer"));
        assert!(f.agents.calls().contains(&AgentCall::Stop));
    }

    #[test]
    fn protected_projects_keep_phases_on_their_account() {
        use crate::domain::{Assignment, PhasePlan};
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .set_setting(super::super::Setting::DefaultAccount, Some("personal"))
            .unwrap();
        f.hedy
            .use_account(&f.dir("/code/tienda"), "trabajo", Some(true))
            .unwrap();
        let err = f.hedy.set_project_phases(
            &f.dir("/code/tienda"),
            PhasePlan::from([(
                Phase::Tests,
                Assignment {
                    account: crate::domain::AccountId::new("personal").unwrap(),
                    model: None,
                },
            )]),
        );
        assert!(matches!(
            err,
            Err(AppError::Domain(
                crate::domain::DomainError::ProtectedProject { .. }
            ))
        ));
    }

    #[test]
    fn api_usage_is_priced_with_the_configured_price() {
        use crate::application::Setting;
        use crate::domain::UsageSample;
        let f = Fixture::new();
        f.add_kimi("Kimi");
        f.hedy.set_api_key("kimi", "sk").unwrap();
        f.hedy.use_account(&f.dir("/code/x"), "kimi", None).unwrap();
        assert!(
            f.hedy
                .set_setting(Setting::ApiPrice(Provider::Kimi), Some("caro"))
                .is_err()
        );
        f.hedy
            .set_setting(Setting::ApiPrice(Provider::Kimi), Some("1/4"))
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/x"),
                prompt: "hola".into(),
                mode: TaskMode::Ask,
                model: None,
                plan_first: false,
            })
            .unwrap();
        f.agents.emit(AgentEvent::Usage(UsageSample {
            input_tokens: 500_000,
            output_tokens: 250_000,
            ..UsageSample::default()
        }));
        let summary = f.hedy.usage_summary(7).unwrap();
        assert_eq!(summary.projects[0].usage.cost_usd, Some(1.5));
    }

    #[test]
    fn api_accounts_need_their_key() {
        let f = Fixture::new();
        f.add_kimi("Kimi");
        f.hedy.use_account(&f.dir("/code/x"), "kimi", None).unwrap();
        let start = || {
            f.hedy.start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/x"),
                prompt: "hola".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
            })
        };
        assert!(matches!(start(), Err(AppError::MissingApiKey(_))));
        f.hedy.set_api_key("kimi", "sk-1").unwrap();
        start().unwrap();
        let request = f.agents.last_request();
        assert!(
            matches!(&request.backend, AgentBackend::OpenAiCompatible { base_url, api_key }
            if base_url == "https://api.moonshot.ai/v1" && api_key.expose() == "sk-1")
        );
        assert!(
            !format!("{request:?}").contains("sk-1"),
            "keys never show in debug output"
        );
        assert_eq!(
            request.model.as_deref(),
            Some("kimi-k3.1-pro"),
            "API default model"
        );
    }

    #[test]
    fn acp_providers_run_their_cli_in_a_worktree() {
        let f = Fixture::new();
        let add = |name: &str, provider: Provider, default_folder: bool| {
            f.hedy.add_account(crate::application::NewAccount {
                name: name.into(),
                provider,
                api_key: false,
                default_folder,
                color: None,
            })
        };
        add("Gemini CLI", Provider::Gemini, false).unwrap();
        add("Kimi Code", Provider::Kimi, false).unwrap();
        // Antigravity always uses its default folder: one login per machine.
        add("Antigravity", Provider::Antigravity, false).unwrap();
        assert!(matches!(
            add("Otra Antigravity", Provider::Antigravity, false),
            Err(AppError::DefaultFolderTaken(_))
        ));

        for (account, args, model_flag) in [
            ("gemini-cli", vec!["--acp"], Some("-m")),
            ("kimi-code", vec!["acp"], None),
            ("antigravity", vec!["acp"], None),
        ] {
            let dir = f.dir(&format!("/code/{account}"));
            f.hedy.use_account(&dir, account, None).unwrap();
            // Even a question gets a worktree: these CLIs read without asking.
            let task = f
                .hedy
                .start_task(NewTask {
                    images: Vec::new(),
                    project: dir,
                    prompt: "¿Qué hace esto?".into(),
                    mode: TaskMode::Ask,
                    model: None,
                    plan_first: false,
                })
                .unwrap();
            assert!(task.workspace.is_some(), "{account}");
            let request = f.agents.last_request();
            assert!(
                matches!(&request.backend, AgentBackend::Acp { model_flag: m, .. } if *m == model_flag),
                "{account}"
            );
            assert_eq!(request.launch.args, args, "{account}");
        }
        let kimi = f.agents.requests();
        let kimi = &kimi[kimi.len() - 2];
        assert!(
            kimi.launch
                .env
                .iter()
                .any(|(var, _)| var == "KIMI_CODE_HOME"),
            "Kimi Code accounts live in their own folder"
        );
    }
}
