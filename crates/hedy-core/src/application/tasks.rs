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

/// Refuses a model name a CLI could read as one of its options.
pub(crate) fn check_model(model: Option<&str>) -> AppResult<()> {
    match model {
        Some(m) if !crate::domain::flow::valid_model(&Some(m.to_owned())) => {
            Err(AppError::InvalidModel(m.to_owned()))
        }
        _ => Ok(()),
    }
}
use crate::domain::carry::carried_prompt;
use crate::domain::permissions::BlockedFiles;
use crate::domain::permissions::{self, AllowRule, Verdict};
use crate::domain::phase::pipeline;
use crate::domain::prompts;
use crate::domain::skill_trial::TrialRef;
use crate::domain::task::{SHARED_DEPENDENCY_DIRS, branch_name, title_from_prompt};
use crate::domain::{
    Account, AccountId, AgentEvent, Attachment, Commit, Credentials, Decision, FileDiff,
    HoldReason, LaunchPurpose, LaunchSpec, LimitLevel, Notice, NoticeKind, PermissionMode, Phase,
    Project, PromptImage, Provider, Stage, Task, TaskId, TaskMode, TaskStatus, UsageSample,
    WorkingStyle, Workspace, policy,
};
use crate::experiments::{self, Experiment};
use crate::tr;

/// Worktrees live inside the repository, excluded locally from git.
const WORKTREES_DIR: &str = ".hedy/worktrees";
/// Tools the automatic mode lets through besides edits and commands: they
/// read, plan or start a subagent whose own tools ask again.
const AUTO_TOOLS: &[&str] = &[
    "Read",
    "Glob",
    "Grep",
    "LS",
    "NotebookRead",
    "TodoWrite",
    "Task",
    "BashOutput",
    "KillShell",
    "ToolSearch",
];
/// Hedy's own MCP servers (memory and code map): they read the project and
/// only propose notes.
const HEDY_MCP_PREFIXES: &[&str] = &[
    "mcp__hedy-memory__",
    "mcp__hedy-board__",
    "mcp__codebase-memory__",
];

/// A `.env` bigger than this is not copied into a task's copy: not settings.
const MAX_SHARED_ENV_BYTES: usize = 1024 * 1024;

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
    /// What the agent does without asking; `None`: the project's default.
    pub permissions: Option<PermissionMode>,
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
    /// The project's blocked files the agent started with (its deny rules
    /// and hook): a changed list starts it again on the next turn.
    blocked: BlockedFiles,
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
                    reads_pdf: provider.acp_reads_pdf(),
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
        self.start_task_traced(request, carry, None)
    }

    /// A run of a skill trial: marked before its worktree and its agent
    /// exist, so both leave the skill out when the run is without it. A run
    /// that never became a task leaves no mark (one that failed is found by
    /// its mark and discarded with its trial).
    pub(super) fn start_task_for_trial(&self, request: NewTask, run: TrialRef) -> AppResult<Task> {
        let id = self.new_task_id(self.clock.now_ms());
        self.mark_trial_run(&id, &run)?;
        let started = self.start_task_traced(request, None, Some((id.clone(), run)));
        if started.is_err() && self.task(&id).is_err() {
            self.forget_trial_run(&id);
        }
        started
    }

    fn start_task_traced(
        &self,
        request: NewTask,
        carry: Option<Carry>,
        trial: Option<(TaskId, TrialRef)>,
    ) -> AppResult<Task> {
        let project = self.project_at(&request.project)?;
        let main_account = project.account.clone().ok_or(AppError::NoAccountResolved)?;
        if request.prompt.trim().is_empty() {
            return Err(AppError::EmptyPrompt);
        }
        check_model(request.model.as_deref())?;
        let stages = pipeline(
            &main_account,
            &project.phases,
            request.mode,
            request.plan_first,
            request.model.as_deref(),
        );
        // Every agent of the pipeline must be allowed here and ready to work.
        let projects = self.projects()?;
        for (index, stage) in stages.iter().enumerate() {
            policy::check_launch(Some(&project), &stage.account, &projects)?;
            let account = self.account(stage.account.as_str())?;
            self.backend(&account)?;
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
        let id = match &trial {
            Some((id, _)) => id.clone(),
            None => self.new_task_id(now),
        };
        let mut title = title_from_prompt(&request.prompt);
        if let Some((_, run)) = &trial {
            let skill = &run.skill;
            let label = match run.with_skill {
                true => tr!("Prueba con «{skill}»", "Trial with «{skill}»"),
                false => tr!("Prueba sin «{skill}»", "Trial without «{skill}»"),
            };
            title = format!("{label} · {title}");
        }
        let (prompt, origin, images) = match carried {
            Some(carried) => (
                carried_prompt(&carried.origin.context, &request.prompt),
                Some(carried.origin),
                [request.images, carried.images].concat(),
            ),
            None => (request.prompt, None, request.images),
        };
        // Only a task that changes files gets a branch; a read-only one gets
        // a throwaway copy per turn when its agent needs it (`open_copy`).
        let workspace = if request.mode.changes_files() {
            Some(self.create_workspace(&project.path, &title, &id)?)
        } else {
            None
        };
        let first = stages[0].clone();
        let code_map = Some(!self.code_map_servers(&project.path)?.is_empty());
        let mut task = Task {
            id,
            project: project.path.clone(),
            mode: request.mode,
            title,
            prompt: prompt.clone(),
            account: first.account,
            model: first.model,
            workspace,
            scratch: None,
            permissions: request.permissions.unwrap_or(project.permission_mode),
            flow: None,
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
            code_map,
            images: Vec::new(),
            // One AI changing code: the project's stops, with the plan as
            // asked for this task.
            working: request.mode.changes_files().then(|| WorkingStyle {
                plan: request.plan_first,
                ..project.working.clone()
            }),
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
        let prompt = self.with_briefing(&task, prompt);
        self.run_stage(&mut task, 0, prompt, None)?;
        self.events.task_updated(&task);
        Ok(task)
    }

    /// With the `briefing` experiment, a project task's first prompt ends
    /// with what Hedy found locally for it (docs/decisions.md §33).
    pub(super) fn with_briefing(&self, task: &Task, prompt: String) -> String {
        if task.mode != TaskMode::Project || !experiments::on(Experiment::Briefing) {
            return prompt;
        }
        let Ok(account) = self.account(task.account.as_str()) else {
            return prompt;
        };
        match self.briefing(task, &account) {
            briefing if briefing.is_empty() => prompt,
            briefing => format!("{prompt}\n\n{briefing}"),
        }
    }

    /// A new, unique task id. The counter keeps ids unique when two tasks
    /// start in the same millisecond.
    pub(super) fn new_task_id(&self, now_ms: i64) -> TaskId {
        let seq = self.sessions.fetch_add(1, Ordering::Relaxed) % 256;
        TaskId::new(format!("t{now_ms:x}{seq:02x}"))
    }

    pub(super) fn create_workspace(
        &self,
        repo: &Path,
        title: &str,
        id: &TaskId,
    ) -> AppResult<Workspace> {
        let base_branch = self.git.current_branch(repo)?;
        let base_commit = self.git.head_commit(repo)?;
        let branch = branch_name(title, id);
        let worktree = repo.join(WORKTREES_DIR).join(id.as_str());
        self.git.ensure_excluded(repo, "/.hedy/")?;
        self.git
            .add_worktree(repo, &worktree, &branch, &base_commit)?;
        self.share_dependencies(repo, &worktree)?;
        self.share_env_files(repo, &worktree);
        self.install_skills_in(repo, &worktree, id);
        Ok(Workspace {
            worktree,
            branch,
            base_branch,
            base_commit,
        })
    }

    /// Codex and ACP agents read files without asking Hedy, so in a
    /// read-only task they work on a copy without a branch, where `.env` and
    /// friends do not exist. It is made when a turn starts and kept for the
    /// turn; any other agent works in the project itself.
    fn open_copy(&self, task: &mut Task, account: &Account) -> AppResult<()> {
        let needed = task.workspace.is_none()
            && !task.conversation
            && !task.mode.changes_files()
            && account.provider.always_in_worktree();
        if !needed {
            return self.close_copy(task);
        }
        if task
            .scratch
            .as_deref()
            .is_some_and(|copy| self.fs.exists(copy))
        {
            return Ok(());
        }
        let repo = task.project.clone();
        let copy = repo.join(WORKTREES_DIR).join(task.id.as_str());
        let base = self.git.head_commit(&repo)?;
        self.git.ensure_excluded(&repo, "/.hedy/")?;
        // One Hedy lost track of starts over; one deleted by hand is still
        // registered with git until pruned.
        self.git.remove_detached_worktree(&repo, &copy)?;
        self.git.add_detached_worktree(&repo, &copy, &base)?;
        self.share_dependencies(&repo, &copy)?;
        self.share_env_files(&repo, &copy);
        self.install_skills_in(&repo, &copy, &task.id);
        task.scratch = Some(copy);
        self.store.save_task(task)?;
        Ok(())
    }

    /// Deletes a read-only task's copy (see [`Hedy::open_copy`]): its turn is
    /// over. One that cannot be deleted stays recorded, for the next tidy.
    pub(super) fn close_copy(&self, task: &mut Task) -> AppResult<()> {
        let Some(copy) = task.scratch.clone() else {
            return Ok(());
        };
        self.git.remove_detached_worktree(&task.project, &copy)?;
        task.scratch = None;
        self.store.save_task(task)?;
        Ok(())
    }

    /// Why the task's permission mode lets a request through with no card
    /// (its name, for the history), when it does. Only in a task with a copy
    /// of its own, after Hedy's floor (protected files, `.git`, outside the
    /// copy) already answered: a write must land in the copy, also after
    /// links, and never in the agents' own settings; a command must not name
    /// a path outside the copy nor do something risky.
    fn allowed_by_mode(
        &self,
        task: &Task,
        tool: &str,
        input: &Value,
        dir: &Path,
    ) -> Option<String> {
        let mode = task.permissions;
        // A read-only step (a review, a flow's reader) never writes or runs
        // anything without a card, whatever the mode.
        if mode == PermissionMode::Manual
            || task.workspace.is_none()
            || current_stage(task).phase.read_only_in(task.mode)
        {
            return None;
        }
        let label = match mode {
            PermissionMode::Auto => tr!("Modo automático", "Automatic mode"),
            _ => tr!("Aceptar ediciones", "Accept edits"),
        };
        if permissions::is_write_tool(tool) {
            let paths = permissions::unattended_writes(tool, input, dir)?;
            return paths
                .iter()
                .all(|p| self.really_inside(p, dir))
                .then_some(label);
        }
        if mode != PermissionMode::Auto {
            return None;
        }
        match tool {
            "Bash" => {
                let command = input["command"].as_str()?;
                (!permissions::command_needs_card(command, dir)).then_some(label)
            }
            // Only what stays in the copy or in Hedy: an MCP tool of the
            // account (mail, chat, GitHub), the web or an ACP agent's "other"
            // tool keeps its card.
            _ if AUTO_TOOLS.contains(&tool)
                || HEDY_MCP_PREFIXES.iter().any(|p| tool.starts_with(p)) =>
            {
                Some(label)
            }
            _ => None,
        }
    }

    /// Hedy's own answer to a task's change of mode (from the composer).
    pub fn set_task_permissions(&self, id: &TaskId, mode: PermissionMode) -> AppResult<Task> {
        let mut task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        task.permissions = mode;
        self.store.save_task(&task)?;
        self.events.task_updated(&task);
        // Cards already waiting that the new mode lets through are answered.
        let pending: Vec<(String, String, Value)> = self
            .live
            .lock()
            .unwrap()
            .get(id)
            .map(|running| {
                running
                    .pending
                    .iter()
                    .map(|(request, (tool, input))| (request.clone(), tool.clone(), input.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let dir = task.work_dir();
        for (request, tool, input) in pending {
            if self.allowed_by_mode(&task, &tool, &input, &dir).is_some() {
                self.answer_permission(id, &request, Decision::Allow)?;
            }
        }
        Ok(task)
    }

    /// A read-only task's write: allowed with no card when every path (as
    /// the agent meant it, from its folder) lands in the task's own folder,
    /// also after links; refused anywhere else.
    fn output_write(&self, task: &Task, tool: &str, input: &Value) -> Verdict {
        let output = self.output_folder(&task.id);
        let dir = task.work_dir();
        let blocked = self.blocked_files_at(&task.project);
        if let Some(reason) = permissions::protected_file_reason(tool, input, &dir, &blocked) {
            return Verdict::Deny(reason);
        }
        let paths = permissions::written_paths(tool, input, &dir);
        // Both sides canonical (a data folder under a link, `/home` →
        // `/var/home`, still matches).
        let inside = !paths.is_empty() && paths.iter().all(|p| self.really_inside(p, &output));
        if inside {
            return Verdict::Allow;
        }
        let folder = output.display();
        Verdict::Deny(tr!(
            "En una pregunta no se cambia el proyecto: escribe el archivo en {folder}.",
            "A question does not change the project: write the file in {folder}."
        ))
    }

    /// Copies read-only tasks no longer need: a turn's copy left behind (the
    /// app closed mid-turn) and the worktree and `hedy/…` branch that read-only
    /// tasks got before they worked without a branch. Never a running task's,
    /// and never a branch of a task that changes files. How many it removed.
    pub fn tidy_read_only_copies(&self) -> usize {
        let Ok(projects) = self.projects() else {
            return 0;
        };
        let mut removed = 0;
        for project in projects {
            for task in self.tasks(&project.path).unwrap_or_default() {
                // The folder of a question that wrote nothing.
                if !self.is_running(&task.id) {
                    let _ = self.fs.remove_empty_dir(&self.output_folder(&task.id));
                }
                let mut task = {
                    // Read again just before: the user may have resumed it.
                    let _status = self.status_lock.lock().unwrap_or_else(|e| e.into_inner());
                    match self.task(&task.id) {
                        Ok(task)
                            if Self::left_by_a_question(&task) && !self.is_running(&task.id) =>
                        {
                            task
                        }
                        _ => continue,
                    }
                };
                if let Some(ws) = task.workspace.clone() {
                    let _ = self
                        .git
                        .remove_worktree(&task.project, &ws.worktree, &ws.branch);
                    // A branch git keeps (checked out elsewhere) stays; a
                    // worktree that is gone must not stay the task's folder.
                    if !self.fs.exists(&ws.worktree) {
                        task.workspace = None;
                        removed += usize::from(self.store.save_task(&task).is_ok());
                    }
                } else if self.close_copy(&mut task).is_ok() {
                    removed += 1;
                }
            }
        }
        removed
    }

    /// A read-only task, idle (in any process), with a worktree or a copy left.
    fn left_by_a_question(task: &Task) -> bool {
        !task.mode.changes_files()
            && !task.status.is_closed()
            && !matches!(
                task.status,
                TaskStatus::Running | TaskStatus::WaitingApproval
            )
            && (task.workspace.is_some() || task.scratch.is_some())
    }

    /// Copies the project's ignored `.env` files into a task's copy, so its
    /// tests and dev server find them, as in the project: never one the
    /// project keeps from the AI. Copies, not links: the agent's changes stay
    /// in the copy. One that cannot be copied is left out.
    pub(super) fn share_env_files(&self, repo: &Path, copy: &Path) {
        let blocked = self.blocked_files_at(repo);
        for file in self.git.ignored_env_files(repo).unwrap_or_default() {
            let target = copy.join(&file);
            let folder_there = target.parent().is_some_and(|p| self.fs.exists(p));
            if blocked.blocks_path(&file)
                || !folder_there
                || self.fs.exists(&target)
                || self.link_on_the_way(copy, &file)
            {
                continue;
            }
            // Private like the original should be, and never a dump.
            match self.fs.read_bytes(&repo.join(&file)) {
                Ok(Some(bytes)) if bytes.len() <= MAX_SHARED_ENV_BYTES => {
                    let _ = self.fs.write_private_file(&target, &bytes);
                }
                _ => {}
            }
        }
    }

    /// Whether a folder of `rel` inside `copy` is a link: a write there could
    /// land outside the copy.
    fn link_on_the_way(&self, copy: &Path, rel: &str) -> bool {
        let mut at = copy.to_path_buf();
        let parts: Vec<&str> = rel.split('/').collect();
        parts[..parts.len().saturating_sub(1)].iter().any(|part| {
            at.push(part);
            self.fs.is_symlink(&at)
        })
    }

    /// Takes the `.env` files a project now keeps from the AI out of its open
    /// tasks' copies, where Hedy put them: only a file still as copied.
    pub(super) fn remove_listed_env_files(&self, project: &Project) {
        let listed: Vec<String> = self
            .git
            .ignored_env_files(&project.path)
            .unwrap_or_default()
            .into_iter()
            .filter(|f| project.blocked_files.blocks_path(f))
            .collect();
        if listed.is_empty() {
            return;
        }
        for task in self.tasks(&project.path).unwrap_or_default() {
            if task.status.is_closed() {
                continue;
            }
            let copies = task
                .workspace
                .iter()
                .map(|w| w.worktree.clone())
                .chain(task.scratch.clone());
            for copy in copies {
                for file in &listed {
                    let original = self.fs.read_bytes(&project.path.join(file)).ok().flatten();
                    let there = self.fs.read_bytes(&copy.join(file)).ok().flatten();
                    if there.is_some() && there == original {
                        let _ = self.fs.remove_file(&copy.join(file));
                    }
                }
            }
        }
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
        let _ = self.close_copy(task);
        if self.task(&task.id).is_ok_and(|t| t.status.is_closed()) {
            return;
        }
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
        self.after_trial_run(&task.id);
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

    /// Starts the agent of a turn, in the task's copy when it needs one.
    fn spawn(
        &self,
        task: &mut Task,
        account: &Account,
        stage: &Stage,
        prompt: String,
        images: Vec<PromptImage>,
        resume: Option<String>,
    ) -> AppResult<()> {
        self.open_copy(task, account)?;
        let started = self.start_agent(task, account, stage, prompt, images, resume);
        if started.is_err() {
            let _ = self.close_copy(task);
        }
        started
    }

    fn start_agent(
        &self,
        task: &Task,
        account: &Account,
        stage: &Stage,
        mut prompt: String,
        mut images: Vec<PromptImage>,
        resume: Option<String>,
    ) -> AppResult<()> {
        // Whatever set it (a phase, a flow's seat, a handoff), a model never
        // reaches a CLI as one of its options.
        check_model(stage.model.as_deref())?;
        let (backend, launch) = self.backend(account)?;
        // An agent that takes no images or PDFs (a later phase, a handoff)
        // is told they exist instead of getting them.
        if !images.is_empty() && !self.agents.image_support(&backend).accepts() {
            let count = images.len();
            prompt.push_str("\n\n");
            prompt.push_str(&tr!(
                "(El usuario adjuntó {count} archivo(s) que este agente no puede ver.)",
                "(The user attached {count} file(s) this agent cannot see.)"
            ));
            images.clear();
        }
        if images.iter().any(|i| i.media_type.is_pdf())
            && !self.agents.pdf_support(&backend).accepts()
        {
            let names: Vec<_> = task
                .images
                .iter()
                .filter(|a| a.media_type.is_pdf())
                .map(|a| a.name.as_str())
                .collect();
            let names = names.join(", ");
            prompt.push_str("\n\n");
            prompt.push_str(&tr!(
                "(El usuario adjuntó PDF que este agente no puede leer: {names}.)",
                "(The user attached PDFs this agent cannot read: {names}.)"
            ));
            images.retain(|i| !i.media_type.is_pdf());
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
        let mut mcp = if task.conversation {
            Vec::new()
        } else {
            self.code_map_servers(&task.project)?
        };
        // A flow's board: a server for the CLIs, native tools for the API
        // agent (docs/decisions.md §42).
        let api = matches!(backend, AgentBackend::OpenAiCompatible { .. });
        let shared = task.flow.as_ref().is_some_and(|r| r.shares_board());
        let board = match (shared, api) {
            (true, true) => Some(super::ports::BoardLink(self.board_tools(&task.id)?)),
            _ => None,
        };
        if let (true, false, Some(command)) = (shared, api, &self.memory_server)
            && !command.board_args.is_empty()
        {
            mcp.push(super::ports::McpServer {
                name: "hedy-board".into(),
                command: command.program.clone(),
                args: command
                    .board_args
                    .iter()
                    .cloned()
                    .chain(["--task".into(), task.id.to_string()])
                    .collect(),
                env: Vec::new(),
            });
        }
        let number = self.sessions.fetch_add(1, Ordering::Relaxed) + 1;
        let hedy = self.clone();
        let id = task.id.clone();
        // Tokens belong to this session's account, even after a handoff.
        let spender = account.id.clone();
        let sink = Arc::new(move |event| hedy.on_agent_event(&id, number, &spender, event));
        let output = self.output_for(task, &backend)?;
        let blocked = self.blocked_files_at(&task.project);
        // Held while starting, so events that arrive early wait for the session.
        let mut live = self.live.lock().unwrap();
        if let Some(mut old) = live.remove(&task.id) {
            old.session.stop();
        }
        // Claude Code gets the skills as a plugin for this session only
        // (built under the lock, as refreshes are). Without it (a full disk)
        // it still reads them with skill_get.
        let plugins = match (&backend, &project) {
            (AgentBackend::ClaudeCode, Some(project)) => self
                .skills_plugin(&project.path, &task.id)
                .ok()
                .flatten()
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
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
                conversation: task.conversation,
                // A flow's role that may not run commands gets no shell.
                no_shell: task
                    .flow
                    .as_ref()
                    .and_then(|run| run.current_role())
                    .is_some_and(|role| !role.can.run),
                no_tools: false,
                plugins,
                output,
                blocked: blocked.clone(),
                board,
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
                blocked,
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
            // A flow's board lives as long as the task (rule 6: notes worth
            // keeping are proposed, never kept here). Cleared after the closed
            // status is saved: the board server checks it before posting.
            if status.is_closed() && task.flow.is_some() {
                self.store.save_task(task)?;
                self.store.clear_board(&task.id)?;
            }
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
        // What the account's CLI offers, not part of the conversation.
        if let AgentEvent::Commands { commands } = event {
            if let Err(error) = self.remember_commands(id, spender, commands) {
                let _ = self.record(
                    id,
                    AgentEvent::Error {
                        message: error.to_string(),
                    },
                );
            }
            return;
        }
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
        let mut task = self.task(id)?;
        let sample = self.priced(spender, sample)?;
        self.store.record_usage(&UsageRecord {
            at: self.now_secs(),
            account: spender.clone(),
            project: task.project.clone(),
            task: task.id.clone(),
            sample: sample.clone(),
        })?;
        // A flow keeps what each of its steps spent (docs/decisions.md §52).
        if let Some(run) = task.flow.as_mut() {
            run.spend(&sample);
            self.store.save_task(&task)?;
        }
        Ok(())
    }

    /// Records tokens spent outside any task (e.g. describing a theme): they
    /// count for the account only, under no project.
    pub(super) fn handle_account_usage(
        &self,
        spender: &AccountId,
        sample: &UsageSample,
    ) -> AppResult<()> {
        self.store.record_usage(&UsageRecord {
            at: self.now_secs(),
            account: spender.clone(),
            project: PathBuf::new(),
            task: TaskId::new(""),
            sample: self.priced(spender, sample)?,
        })?;
        Ok(())
    }

    /// API agents report tokens only; the cost comes from the user's price.
    fn priced(&self, spender: &AccountId, sample: &UsageSample) -> AppResult<UsageSample> {
        let mut sample = sample.clone();
        if sample.cost_usd.is_none()
            && let Ok(account) = self.account(spender.as_str())
            && matches!(account.credentials, Credentials::ApiKey)
            && let Some(price) = self.api_price(account.provider)?
        {
            sample.cost_usd = Some(price.cost(&sample));
        }
        Ok(sample)
    }

    fn handle_event(&self, id: &TaskId, event: AgentEvent) -> AppResult<()> {
        let mut task = self.task(id)?;
        // A late event of a task closed meanwhile (another process discarded
        // it) is kept, but never reopens it; its agent here is stopped (a
        // permission it asks for would wait forever).
        if task.status.is_closed() {
            self.stop_session(id);
            return self.record(id, event);
        }
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
                // A question writes only in its own folder, outside the
                // project, and needs nobody to approve that.
                let output_write = !task.mode.changes_files() && permissions::is_write_tool(tool);
                let dir = task.work_dir();
                let blocked = self.blocked_files_at(&task.project);
                let verdict = match permissions::evaluate(
                    tool,
                    input,
                    &dir,
                    &running.rules,
                    &blocked,
                ) {
                    _ if output_write => self.output_write(&task, tool, input),
                    Verdict::Deny(reason) => Verdict::Deny(reason),
                    // Written as inside, but a link (a shared `node_modules`)
                    // takes it out.
                    _ if permissions::written_paths(tool, input, &dir)
                        .iter()
                        .any(|p| !self.really_inside(p, &dir)) =>
                    {
                        let folder = dir.display();
                        Verdict::Deny(tr!(
                            "Hedy no permite cambios fuera de la carpeta de la tarea ({folder}): esa ruta pasa por un enlace que sale de ella.",
                            "Hedy does not allow changes outside the task folder ({folder}): that path goes through a link out of it."
                        ))
                    }
                    verdict => verdict,
                };
                // What the task's mode lets through without a card.
                let by_mode = match verdict {
                    Verdict::Ask => self.allowed_by_mode(&task, tool, input, &dir),
                    _ => None,
                };
                let verdict = if by_mode.is_some() {
                    Verdict::Allow
                } else {
                    verdict
                };
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
                        let reason = if output_write {
                            tr!("En la carpeta de la tarea", "In the task's folder")
                        } else if let Some(mode) = by_mode {
                            mode
                        } else {
                            "Permitido en este chat".into()
                        };
                        return self.record(id, resolved(Decision::Allow, Some(reason)));
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
                // A flow decides what comes after a role (docs/decisions.md §42).
                if task.flow.is_some() && (*ok || limited) {
                    if self.task(id)?.status.is_closed() {
                        return Ok(());
                    }
                    if limited {
                        // Paused by the account's limit (rule 3: never another
                        // account): what it said waits for the role's end.
                        if let Some(run) = task.flow.as_mut() {
                            let said = summary.trim();
                            if !said.is_empty() {
                                run.partial = Some(said.to_owned());
                            }
                        }
                        self.store.save_task(&task)?;
                        self.set_status(&mut task, TaskStatus::Review)?;
                        self.notify(&task, NoticeKind::LimitReached);
                        return Ok(());
                    }
                    return self.flow_role_done(&mut task, summary);
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
                // Hedy runs the task's checks; a first failure goes back to
                // the same session (docs/decisions.md §60).
                if *ok
                    && !limited
                    && current_stage(&task).phase != Phase::Plan
                    && self.trial_of(id).is_none()
                    && self.run_task_checks(&mut task)?
                {
                    return Ok(());
                }
                // Then the review with a clean context, when it has one.
                if *ok
                    && !limited
                    && current_stage(&task).phase != Phase::Plan
                    && self.trial_of(id).is_none()
                {
                    self.run_task_review(&task)?;
                }
                // A flow set to follow a task's end takes it on from here.
                if *ok
                    && !limited
                    && current_stage(&task).phase != Phase::Plan
                    && self.trial_of(id).is_none()
                    && self.flow_after_task(&mut task)?
                {
                    return Ok(());
                }
                // The copy goes with the turn, and so does its agent: a
                // process left in a deleted folder breaks on the next turn,
                // which resumes the session in a new copy instead.
                if task.scratch.is_some() {
                    self.stop_session(id);
                }
                // The turn ends even if its copy cannot go now.
                let _ = self.close_copy(&mut task);
                let (status, notice) = if *ok {
                    let changes = task.mode.changes_files();
                    (TaskStatus::Review, NoticeKind::Finished { changes })
                } else {
                    (TaskStatus::Failed, NoticeKind::Failed)
                };
                let changed = {
                    let _status = self.status_lock.lock().unwrap_or_else(|e| e.into_inner());
                    // Closed meanwhile (discarded elsewhere): it stays closed.
                    // (The event is already recorded.)
                    if self.task(id)?.status.is_closed() {
                        return Ok(());
                    }
                    let changed = task.status != status;
                    self.set_status(&mut task, status)?;
                    changed
                };
                if changed {
                    self.notify(&task, notice);
                }
                self.after_trial_run(id);
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

    /// Where `path` goes after following links (from the deepest part that
    /// exists) is inside `dir`. A dangling link is not.
    pub(super) fn really_inside(&self, path: &Path, dir: &Path) -> bool {
        let Ok(root) = self.fs.canonicalize(dir) else {
            return false;
        };
        path.ancestors()
            .find(|p| self.fs.exists(p) || self.fs.is_symlink(p))
            .is_some_and(|p| {
                self.fs
                    .canonicalize(p)
                    .is_ok_and(|real| real.starts_with(&root))
            })
    }

    /// The tool and input of a request still waiting for an answer.
    pub(super) fn pending_request(&self, id: &TaskId, request_id: &str) -> Option<(String, Value)> {
        self.live
            .lock()
            .unwrap()
            .get(id)?
            .pending
            .get(request_id)
            .cloned()
    }

    /// The card still waits for the user (not answered by a rule, by the
    /// app or by an ended session).
    pub fn is_permission_pending(&self, id: &TaskId, request_id: &str) -> bool {
        self.live
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|running| running.pending.contains_key(request_id))
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

    /// Declines every permission card the task waits on, telling the agent
    /// `reason`, and lets it go on. For a driver that cannot ask anyone (a
    /// question from the terminal). Returns the declined requests.
    pub fn decline_open_requests(&self, id: &TaskId, reason: &str) -> AppResult<Vec<String>> {
        let declined = self.with_session(id, |running| {
            let mut open: Vec<String> = running.pending.keys().cloned().collect();
            open.sort();
            for request_id in &open {
                running.session.answer(request_id, false, Some(reason))?;
                running.pending.remove(request_id);
            }
            Ok(open)
        })?;
        for request_id in &declined {
            self.record(
                id,
                AgentEvent::PermissionResolved {
                    request_id: request_id.clone(),
                    decision: Decision::Deny,
                    automatic: true,
                    reason: Some(reason.to_owned()),
                },
            )?;
        }
        let mut task = self.task(id)?;
        if !declined.is_empty() && task.status == TaskStatus::WaitingApproval {
            self.set_status(&mut task, TaskStatus::Running)?;
        }
        Ok(declined)
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
        // The agent may have finished meanwhile: only a waiting task runs
        // again, checked and changed under the same lock as a turn's end.
        let _status = self.status_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut task = self.task(id)?;
        if task.status != TaskStatus::WaitingApproval {
            return Ok(());
        }
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
        self.push_to_session(task, text, images)
    }

    /// Sends `text` to the task's agent, in its running session or resuming
    /// it, and sets the task running. What sent it records it.
    pub(super) fn push_to_session(
        &self,
        task: &mut Task,
        text: &str,
        images: Vec<PromptImage>,
    ) -> AppResult<()> {
        let id = &task.id.clone();
        // An agent started before the project's list changed keeps its old
        // rules: it resumes its session with the new ones instead.
        let blocked = self.blocked_files_at(&task.project);
        let stale = self
            .live
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|running| running.blocked != blocked);
        if stale {
            self.stop_session(id);
        }
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
        // A trial run at a limit is void anyway: no tokens elsewhere for it.
        self.refuse_trial_run(id)?;
        check_model(model.as_deref())?;
        let mut task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        // A flow waiting for the user has no role at work to hand over.
        if task
            .flow
            .as_ref()
            .and_then(|r| r.current())
            .is_some_and(|n| matches!(n.kind, crate::domain::flow::NodeKind::Approval))
        {
            return Err(AppError::FlowAtApproval(id.clone()));
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
        let prompt = match task
            .flow
            .as_ref()
            .and_then(|r| Some((r, r.current_role()?)))
        {
            // A flow's role keeps its part and what the roles before it said.
            Some((run, role)) => {
                let handed = prompts::handoff_prompt(&task.prompt, None, &previous_label, &changed);
                let tools = run.shares_board() && self.board_reaches(&target);
                let role = role.clone();
                format!(
                    "{}\n\n{handed}",
                    self.flow_role_prompt(&task, &role, tools)?
                )
            }
            None => prompts::handoff_prompt(
                &task.prompt,
                task.plan.as_deref(),
                &previous_label,
                &changed,
            ),
        };
        let index = task.stage;
        // Waiting for the old account's limit makes no sense any more.
        task.wait = None;
        self.run_stage(&mut task, index, prompt, None)?;
        self.set_status(&mut task, TaskStatus::Running)?;
        Ok(task)
    }

    /// Stops the agent; the worktree stays for review.
    pub fn stop_task(&self, id: &TaskId) -> AppResult<()> {
        self.trial_run_stopped(id);
        self.stop_session(id);
        // A flow's check (its tests) stops too.
        self.runner.stop_checks(id.as_str());
        self.hold_queue(id, HoldReason::Stopped);
        let mut task = self.task(id)?;
        let _ = self.close_copy(&mut task);
        if !task.status.is_closed() && task.status != TaskStatus::Failed {
            self.set_status(&mut task, TaskStatus::Review)?;
        }
        self.after_trial_run(id);
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
        self.runner.stop_checks(id.as_str());
        if let Some(ws) = &task.workspace {
            self.git
                .remove_worktree(&task.project, &ws.worktree, &ws.branch)?;
        }
        let _ = self.close_copy(&mut task);
        let _ = self.remove_outputs(&task.id);
        self.forget_skills_plugin(&task.id);
        self.set_status(&mut task, TaskStatus::Discarded)?;
        self.clear_queue(id)?;
        // A trial run discarded by the user voids its trial.
        self.after_trial_run(id);
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
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/agenda"),
                prompt: prompt.into(),
                mode: TaskMode::Project,
                model: Some("opus".into()),
                plan_first,
                permissions: None,
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
        assert!(ws.worktree.starts_with("/code/agenda/.hedy/worktrees"));
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
        f.fs.add_existing("/code/agenda/node_modules");
        f.fs.add_existing("/code/agenda/vendor");
        f.git.ignore("node_modules");
        let task = new_task(&f, "Hola", false);
        let wt = task.workspace.unwrap().worktree;
        assert_eq!(
            f.fs.symlinks(),
            vec![(f.dir("/code/agenda/node_modules"), wt.join("node_modules"))]
        );
        assert_eq!(f.git.excluded(), vec!["/.hedy/", "/node_modules"]);
    }

    #[test]
    fn protected_files_and_outside_writes_are_denied_without_asking() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        // A project that keeps `.env` from the AI.
        f.hedy
            .set_project_blocked_files(&f.dir("/code/agenda"), &[".env".into()])
            .unwrap();
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
    fn a_remembered_write_never_covers_agent_settings_nor_a_way_out() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        let wt = task.workspace.clone().unwrap().worktree;
        f.fs.add_existing(wt.to_str().unwrap());
        // A dependency folder shared with the project, as a link.
        crate::application::ports::FileSystem::symlink_dir(
            &*f.fs,
            Path::new("/code/agenda/node_modules"),
            &wt.join("node_modules"),
        )
        .unwrap();
        f.fs.add_existing(wt.join("node_modules").to_str().unwrap());
        let write = |id: &str, path: &str| {
            f.agents
                .emit(permission(id, "Write", json!({"file_path": wt.join(path)})));
        };
        write("r1", "src/a.py");
        f.hedy
            .answer_permission(&task.id, "r1", Decision::AllowForTask)
            .unwrap();
        write("r2", "src/b.py");
        write("r3", "node_modules/pkg/index.js");
        write("r4", ".claude/settings.json");
        assert_eq!(
            f.agents.calls(),
            vec![
                AgentCall::Answer("r1".into(), true),
                AgentCall::Answer("r2".into(), true),
                AgentCall::Answer("r3".into(), false),
            ],
            "the link is denied, the settings wait for the user"
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
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        let task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/agenda"),
                prompt: "¿Dónde se crean las citas?".into(),
                mode: TaskMode::Ask,
                model: None,
                plan_first: true,
                permissions: None,
            })
            .unwrap();
        assert!(task.workspace.is_none());
        let request = f.agents.last_request();
        assert_eq!(request.launch.cwd, Some(f.dir("/code/agenda")));
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
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        let assign = |account: &str, model: Option<&str>| Assignment {
            account: crate::domain::AccountId::new(account).unwrap(),
            model: model.map(str::to_owned),
        };
        f.hedy
            .set_project_phases(
                &f.dir("/code/agenda"),
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
                project: f.dir("/code/agenda"),
                prompt: "Recordatorios".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: true,
                permissions: None,
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
    fn an_answer_after_the_turn_ended_never_resumes_the_task() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        f.agents
            .emit(permission("p1", "Bash", json!({"command": "pytest"})));
        assert!(f.hedy.is_permission_pending(&task.id, "p1"));
        // The turn ends before the user answers (a race in the app or CLI).
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "listo".into(),
            duration_ms: None,
        });
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
        let _ = f.hedy.answer_permission(&task.id, "p1", Decision::Allow);
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
    }

    #[test]
    fn a_late_event_never_reopens_a_task_closed_elsewhere() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        use crate::application::ports::TaskRepository as _;
        // Another process (the app) discarded it while this one ran it.
        let mut closed = f.hedy.task(&task.id).unwrap();
        closed.status = TaskStatus::Discarded;
        f.store.save_task(&closed).unwrap();
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "listo".into(),
            duration_ms: None,
        });
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Discarded);
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
                permissions: None,
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
                permissions: None,
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
    fn acp_providers_run_their_cli_in_a_copy() {
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
            // Even a question gets a copy: these CLIs read without asking.
            let task = f
                .hedy
                .start_task(NewTask {
                    images: Vec::new(),
                    project: dir.clone(),
                    prompt: "¿Qué hace esto?".into(),
                    mode: TaskMode::Ask,
                    model: None,
                    plan_first: false,
                    permissions: None,
                })
                .unwrap();
            let copy = dir.join(".hedy/worktrees").join(task.id.as_str());
            assert!(task.workspace.is_none(), "{account}: no branch");
            assert_eq!(task.scratch.as_ref(), Some(&copy), "{account}");
            let request = f.agents.last_request();
            assert_eq!(request.launch.cwd.as_ref(), Some(&copy), "{account}");
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

    #[test]
    fn read_only_tasks_with_codex_get_a_copy_per_turn_without_a_branch() {
        let f = Fixture::new();
        f.add_codex("Codex");
        let dir = f.dir("/code/agenda");
        f.hedy.use_account(&dir, "codex", None).unwrap();
        let task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: dir.clone(),
                prompt: "¿Dónde se crean las citas?".into(),
                mode: TaskMode::Ask,
                model: None,
                plan_first: false,
                permissions: None,
            })
            .unwrap();
        let copy = dir.join(".hedy/worktrees").join(task.id.as_str());
        let detached = format!("detached {} at abc123", copy.display());
        assert!(task.workspace.is_none(), "no branch for a question");
        assert_eq!(task.scratch.as_ref(), Some(&copy));
        assert_eq!(f.git.worktrees(), vec![detached.clone()]);
        assert_eq!(f.git.excluded(), vec!["/.hedy/"]);
        assert_eq!(f.agents.last_request().launch.cwd, Some(copy.clone()));

        // The turn ends: the copy goes, and so does its agent.
        f.agents.emit(AgentEvent::SessionStarted {
            session_id: "s-1".into(),
            model: "m".into(),
        });
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "En citas.py".into(),
            duration_ms: None,
        });
        let task = f.hedy.task(&task.id).unwrap();
        assert_eq!((task.status, task.scratch), (TaskStatus::Review, None));
        assert!(
            !f.hedy.is_running(&task.id),
            "no process left in a deleted folder"
        );
        let removed = format!("remove detached {}", copy.display());
        // Pruned before it is made, then removed.
        assert_eq!(f.git.log(), vec![removed.clone(), removed.clone()]);

        // A follow-up resumes the session in a new copy at the same path.
        f.hedy.send_message(&task.id, "¿y las borra?").unwrap();
        assert_eq!(f.git.worktrees(), vec![detached.clone(), detached.clone()]);
        assert_eq!(f.hedy.task(&task.id).unwrap().scratch, Some(copy.clone()));
        let request = f.agents.last_request();
        assert_eq!(request.resume.as_deref(), Some("s-1"));
        assert_eq!(request.launch.cwd, Some(copy.clone()));
        f.hedy.stop_task(&task.id).unwrap();
        assert_eq!(f.hedy.task(&task.id).unwrap().scratch, None);
        f.hedy.send_message(&task.id, "sigue").unwrap();
        assert_eq!(f.agents.last_request().launch.cwd, Some(copy.clone()));
        f.hedy.discard_task(&task.id).unwrap();
        assert_eq!(f.hedy.task(&task.id).unwrap().scratch, None);
        assert!(
            f.git.log().iter().all(|l| !l.starts_with("remove hedy/")),
            "no branch to delete"
        );
    }

    #[test]
    fn tidy_removes_what_read_only_tasks_left_and_nothing_else() {
        use crate::application::ports::TaskRepository;
        let f = Fixture::new();
        let change = new_task(&f, "Recordatorios", false);
        let ws = |id: &str| Workspace {
            worktree: format!("/code/agenda/.hedy/worktrees/{id}").into(),
            branch: format!("hedy/pregunta-{id}"),
            base_branch: "main".into(),
            base_commit: "abc123".into(),
        };
        let mut old = change.clone();
        old.id = TaskId::new("t-old");
        old.mode = TaskMode::Ask;
        old.status = TaskStatus::Review;
        old.workspace = Some(ws("t-old"));
        let mut left = old.clone();
        left.id = TaskId::new("t-left");
        left.workspace = None;
        left.scratch = Some("/code/agenda/.hedy/worktrees/t-left".into());
        let mut running = old.clone();
        running.id = TaskId::new("t-running");
        running.status = TaskStatus::Running;
        running.workspace = Some(ws("t-running"));
        // Waiting on a card, perhaps in the `hedy` CLI: its copy is in use.
        let mut waiting = left.clone();
        waiting.id = TaskId::new("t-waiting");
        waiting.status = TaskStatus::WaitingApproval;
        for task in [&old, &left, &running, &waiting] {
            f.store.save_task(task).unwrap();
        }

        assert_eq!(f.hedy.tidy_read_only_copies(), 2);
        assert_eq!(
            f.git.log(),
            vec![
                "remove hedy/pregunta-t-old".to_owned(),
                "remove detached /code/agenda/.hedy/worktrees/t-left".to_owned(),
            ]
        );
        assert_eq!(f.hedy.task(&old.id).unwrap().workspace, None);
        assert_eq!(f.hedy.task(&left.id).unwrap().scratch, None);
        assert!(f.hedy.task(&running.id).unwrap().workspace.is_some());
        assert!(f.hedy.task(&waiting.id).unwrap().scratch.is_some());
        assert!(f.hedy.task(&change.id).unwrap().workspace.is_some());
        assert_eq!(f.hedy.tidy_read_only_copies(), 0);
    }

    #[test]
    fn a_copy_gets_the_ignored_env_files_the_project_lets_the_ai_read() {
        use crate::application::ports::FileSystem;
        let f = Fixture::new();
        new_task(&f, "Hola", false);
        let repo = f.dir("/code/agenda");
        let copy = repo.join(".hedy/worktrees/t1");
        f.git.ignore(".env");
        f.git.ignore("apps/web/.env.local");
        f.git.ignore("node_modules");
        for file in [".env", "apps/web/.env.local"] {
            f.fs.write_private_file(&repo.join(file), b"KEY=1").unwrap();
        }
        f.fs.add_existing(copy.to_str().unwrap());
        f.fs.add_existing(copy.join("apps/web").to_str().unwrap());

        // Free by default, as in Claude Code: both go.
        f.hedy.share_env_files(&repo, &copy);
        for file in [".env", "apps/web/.env.local"] {
            assert_eq!(
                f.fs.read_bytes(&copy.join(file)).unwrap(),
                Some(b"KEY=1".to_vec())
            );
        }

        // A project that keeps `.env` from the AI: that one stays out.
        let other = repo.join(".hedy/worktrees/t2");
        f.fs.add_existing(other.to_str().unwrap());
        f.fs.add_existing(other.join("apps/web").to_str().unwrap());
        f.hedy
            .set_project_blocked_files(&repo, &[".env".into()])
            .unwrap();
        f.hedy.share_env_files(&repo, &other);
        assert_eq!(f.fs.read_bytes(&other.join(".env")).unwrap(), None);
        assert!(
            f.fs.read_bytes(&other.join("apps/web/.env.local"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn a_project_list_refuses_folders_and_catch_alls_and_leaves_open_copies() {
        use crate::application::ports::FileSystem;
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        let repo = f.dir("/code/agenda");
        for pattern in ["config/", "secrets/*", "*", "*.*", "a\\b"] {
            assert!(
                matches!(
                    f.hedy.set_project_blocked_files(&repo, &[pattern.into()]),
                    Err(AppError::InvalidBlockedPattern(_))
                ),
                "{pattern}"
            );
        }
        let project = f
            .hedy
            .set_project_blocked_files(&repo, &["**/.env".into(), "*.pem".into()])
            .unwrap();
        assert_eq!(project.blocked_files.patterns(), [".env", "*.pem"]);

        // The task's copy holds the `.env` Hedy copied, and one the agent changed.
        let ws = task.workspace.unwrap().worktree;
        f.hedy.set_project_blocked_files(&repo, &[]).unwrap();
        f.git.ignore(".env");
        f.git.ignore("api/.env");
        for (file, bytes) in [(".env", b"A=1".as_slice()), ("api/.env", b"B=1".as_slice())] {
            f.fs.write_private_file(&repo.join(file), bytes).unwrap();
        }
        f.fs.write_private_file(&ws.join(".env"), b"A=1").unwrap();
        f.fs.write_private_file(&ws.join("api/.env"), b"B=2")
            .unwrap();
        f.hedy
            .set_project_blocked_files(&repo, &[".env".into()])
            .unwrap();
        assert_eq!(
            f.fs.read_bytes(&ws.join(".env")).unwrap(),
            None,
            "taken out"
        );
        assert_eq!(
            f.fs.read_bytes(&ws.join("api/.env")).unwrap(),
            Some(b"B=2".to_vec()),
            "the agent's own file stays"
        );
    }

    #[test]
    fn a_changed_list_restarts_the_agent_on_its_next_turn() {
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
        f.hedy
            .set_project_blocked_files(&f.dir("/code/agenda"), &[".env".into()])
            .unwrap();
        f.hedy.send_message(&task.id, "sigue").unwrap();
        let request = f.agents.last_request();
        assert_eq!(request.resume.as_deref(), Some("s-1"));
        assert_eq!(request.blocked.patterns(), [".env"]);
        // Unchanged: the same session takes the message.
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: None,
        });
        f.hedy.send_message(&task.id, "y más").unwrap();
        assert_eq!(
            f.agents.calls().last(),
            Some(&AgentCall::Send("y más".into()))
        );
    }

    #[test]
    fn permission_modes_let_through_what_they_say_and_never_the_floor() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        assert_eq!(
            task.permissions,
            PermissionMode::Manual,
            "the project's default"
        );
        let ws = task.workspace.clone().unwrap().worktree;
        f.fs.add_existing(ws.to_str().unwrap());
        let file = |name: &str| ws.join(name).to_string_lossy().into_owned();
        let ask = |mode: PermissionMode| {
            f.hedy.set_task_permissions(&task.id, mode).unwrap();
            let before = f.agents.calls().len();
            let requests = [
                ("w1", "Write", json!({"file_path": file("src/app.py")})),
                ("w2", "Edit", json!({"file_path": file(".git/config")})),
                (
                    "w3",
                    "Write",
                    json!({"file_path": file(".claude/settings.json")}),
                ),
                ("w4", "Write", json!({"file_path": "/home/e/fuera.txt"})),
                ("b1", "Bash", json!({"command": "npm test"})),
                ("b2", "Bash", json!({"command": "git push origin main"})),
                ("b3", "Bash", json!({"command": "rm -rf ~/proyectos"})),
            ];
            for (id, tool, input) in requests {
                f.agents.emit(permission(id, tool, input));
            }
            f.agents.calls()[before..].to_vec()
        };
        let answered = |calls: &[AgentCall], id: &str| {
            calls.iter().find_map(|c| match c {
                AgentCall::Answer(r, allow) if r == id => Some(*allow),
                _ => None,
            })
        };
        // Manual: only the floor answers (denied), everything else waits.
        let calls = ask(PermissionMode::Manual);
        assert_eq!(answered(&calls, "w1"), None);
        assert_eq!(answered(&calls, "w2"), Some(false));
        assert_eq!(answered(&calls, "w4"), Some(false));
        assert_eq!(answered(&calls, "b1"), None);
        // Accept edits: the edit in the copy goes; settings and commands wait.
        let calls = ask(PermissionMode::AcceptEdits);
        assert_eq!(answered(&calls, "w1"), Some(true));
        assert_eq!(
            answered(&calls, "w3"),
            None,
            "the agents' settings always ask"
        );
        assert_eq!(answered(&calls, "b1"), None);
        // Automatic: commands too, but not a risky one or one outside.
        let calls = ask(PermissionMode::Auto);
        assert_eq!(answered(&calls, "w1"), Some(true));
        assert_eq!(answered(&calls, "w2"), Some(false), ".git stays denied");
        assert_eq!(answered(&calls, "w3"), None);
        assert_eq!(answered(&calls, "w4"), Some(false));
        assert_eq!(answered(&calls, "b1"), Some(true));
        assert_eq!(answered(&calls, "b2"), None);
        assert_eq!(answered(&calls, "b3"), None);
        let events = f.hedy.task_events(&task.id).unwrap();
        assert!(events.iter().any(|e| matches!(e,
            AgentEvent::PermissionResolved { automatic: true, reason: Some(r), .. }
                if r == "Modo automático" || r == "Automatic mode")));
    }

    #[test]
    fn a_code_task_keeps_its_projects_way_of_working_with_the_plan_it_asked_for() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let dir = f.dir("/code/agenda");
        f.hedy.use_account(&dir, "personal", None).unwrap();
        let style = crate::domain::Preset::AfterOk.apply(&WorkingStyle {
            e2e_command: Some(" npm run e2e ".into()),
            ..Default::default()
        });
        let project = f.hedy.set_project_working(&dir, style).unwrap();
        assert_eq!(project.working.e2e_command.as_deref(), Some("npm run e2e"));
        let start = |mode: TaskMode, plan_first: bool| {
            f.hedy
                .start_task(NewTask {
                    images: Vec::new(),
                    project: dir.clone(),
                    prompt: "Cambia algo".into(),
                    mode,
                    model: None,
                    plan_first,
                    permissions: None,
                })
                .unwrap()
        };
        // Right away for once: no plan, the project's checks all the same.
        let quick = start(TaskMode::Project, false).working.unwrap();
        assert!(!quick.plan && quick.tests && quick.e2e);
        assert!(start(TaskMode::Project, true).working.unwrap().plan);
        assert_eq!(start(TaskMode::Ask, false).working, None);
        // A new style reaches new tasks only.
        f.hedy
            .set_project_working(&dir, WorkingStyle::default())
            .unwrap();
        assert!(!start(TaskMode::Project, false).working.unwrap().tests);
    }

    #[test]
    fn a_question_in_the_real_folder_keeps_its_cards_whatever_the_mode() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let dir = f.dir("/code/agenda");
        f.hedy.use_account(&dir, "personal", None).unwrap();
        f.hedy
            .set_project_permission_mode(&dir, PermissionMode::Auto)
            .unwrap();
        let task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: dir.clone(),
                prompt: "¿Qué hace esto?".into(),
                mode: TaskMode::Ask,
                model: None,
                plan_first: false,
                permissions: None,
            })
            .unwrap();
        assert_eq!(
            task.permissions,
            PermissionMode::Auto,
            "the project's default"
        );
        f.agents
            .emit(permission("b1", "Bash", json!({"command": "npm test"})));
        assert!(f.agents.calls().is_empty(), "no copy: the card stays");
        // A task can start with another mode than the project's.
        let other = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: dir,
                prompt: "Cambia algo".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
                permissions: Some(PermissionMode::Manual),
            })
            .unwrap();
        assert_eq!(other.permissions, PermissionMode::Manual);
    }

    #[test]
    fn automatic_mode_keeps_cards_for_outside_tools_and_answers_waiting_ones() {
        let f = Fixture::new();
        let task = new_task(&f, "Hola", false);
        let ws = task.workspace.clone().unwrap().worktree;
        f.fs.add_existing(ws.to_str().unwrap());
        // Waiting in manual mode.
        let edit = ws.join("src/app.py").to_string_lossy().into_owned();
        f.agents
            .emit(permission("w1", "Write", json!({"file_path": edit})));
        f.agents
            .emit(permission("x1", "mcp__gmail__send", json!({"to": "a@b.c"})));
        assert!(f.agents.calls().is_empty());
        // Switching answers the edit, not the mail.
        f.hedy
            .set_task_permissions(&task.id, PermissionMode::Auto)
            .unwrap();
        assert_eq!(f.agents.calls(), vec![AgentCall::Answer("w1".into(), true)]);
        for (id, tool) in [
            ("x2", "WebFetch"),
            ("x3", "Tool"),
            ("x4", "mcp__slack__post"),
        ] {
            f.agents.emit(permission(id, tool, json!({})));
        }
        f.agents.emit(permission(
            "m1",
            "mcp__hedy-memory__memory_search",
            json!({}),
        ));
        f.agents.emit(permission("t1", "TodoWrite", json!({})));
        assert_eq!(
            f.agents.calls()[1..],
            [
                AgentCall::Answer("m1".into(), true),
                AgentCall::Answer("t1".into(), true)
            ]
        );
    }
}
