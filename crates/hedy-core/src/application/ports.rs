//! Ports: what the use cases need from the outside world. Adapters implement them;
//! tests use in-memory fakes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::domain::code::{Blame, CommitInfo, FileBytes, LineMatch, PathChange, TextQuery};
use crate::domain::recall::{NoteUse, PageRequest};
use crate::domain::sync::{
    Change, DeviceId, EpochInfo, EpochMember, ItemKind, MemoryHome, NoteBody, Scope, SkillBody,
    Stamp, Version,
};
use crate::domain::{
    Account, AccountId, AgentEvent, Category, Commit, FileDiff, HoldReason, ImageSupport,
    KnownCommands, LaunchSpec, Note, NoteStatus, Notice, Phase, PlanUsage, Project, PromptImage,
    QueuedMessage, Skill, SkillStatus, Task, TaskId, TaskMode, UsageProblem, UsageSample,
    flow::{Flow, RoleDef},
    plan_usage::WallTime,
    theme::CustomTheme,
};

/// Failure in an adapter. The message is user-facing (Spanish).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct PortError {
    pub message: String,
}

impl PortError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub type PortResult<T> = Result<T, PortError>;

pub trait AccountRepository: Send + Sync {
    fn list_accounts(&self) -> PortResult<Vec<Account>>;
    fn save_account(&self, account: &Account) -> PortResult<()>;
    fn delete_account(&self, id: &AccountId) -> PortResult<()>;
}

pub trait ProjectRepository: Send + Sync {
    fn list_projects(&self) -> PortResult<Vec<Project>>;
    fn save_project(&self, project: &Project) -> PortResult<()>;
    fn delete_project(&self, path: &Path) -> PortResult<()>;
}

pub trait SettingsRepository: Send + Sync {
    fn get_setting(&self, key: &str) -> PortResult<Option<String>>;
    fn set_setting(&self, key: &str, value: Option<&str>) -> PortResult<()>;
}

pub trait TaskRepository: Send + Sync {
    fn save_task(&self, task: &Task) -> PortResult<()>;
    fn task(&self, id: &TaskId) -> PortResult<Option<Task>>;
    /// Newest first.
    fn tasks(&self, project: &Path) -> PortResult<Vec<Task>>;
    /// Every project's tasks that are still running or waiting, plus those
    /// created or with events since `since` (seconds). Newest first.
    fn recent_tasks(&self, since: i64) -> PortResult<Vec<Task>>;
    fn append_event(&self, id: &TaskId, at: i64, event: &AgentEvent) -> PortResult<()>;
    fn events(&self, id: &TaskId) -> PortResult<Vec<AgentEvent>>;
    /// Events with the time they were recorded (seconds).
    fn timed_events(&self, id: &TaskId) -> PortResult<Vec<(i64, AgentEvent)>>;
    /// Tasks whose user chose to wait for a limit to renew (any age).
    fn waiting_tasks(&self) -> PortResult<Vec<Task>>;
    /// Every conversation (task outside any project). Newest first.
    fn conversations(&self) -> PortResult<Vec<Task>>;
    /// The tasks that continue conversation `id` (`origin.conversation`).
    /// Newest first.
    fn continuations(&self, conversation: &TaskId) -> PortResult<Vec<Task>>;
    /// Deletes the task with its events and queue. Usage records stay: the
    /// tokens were spent.
    fn delete_task(&self, id: &TaskId) -> PortResult<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub at: i64,
    pub account: AccountId,
    /// Empty for tokens spent outside any task (they count for the account
    /// only).
    pub project: PathBuf,
    /// Empty outside any task.
    pub task: TaskId,
    pub sample: UsageSample,
}

/// Last rate-limit status a CLI reported for an account.
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimit {
    pub status: String,
    /// Percentage used (0–100), when the CLI reports one.
    pub utilization: Option<f64>,
    pub resets_at: Option<i64>,
    pub at: i64,
}

/// The last reading of an account's plan usage, kept so screens show it at
/// once, and when Hedy last tried (successful or not).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanUsageRecord {
    /// The last successful reading; kept when a later attempt fails.
    pub usage: Option<PlanUsage>,
    /// Unix seconds.
    pub attempted_at: i64,
    /// Why the last attempt gave nothing; `None` when it worked.
    pub problem: Option<UsageProblem>,
}

pub trait UsageRepository: Send + Sync {
    fn record_usage(&self, record: &UsageRecord) -> PortResult<()>;
    /// Sum of everything recorded for `account` since `since` (seconds).
    fn usage_since(&self, account: &AccountId, since: i64) -> PortResult<UsageSample>;
    /// Totals per project and account since `since`.
    fn usage_by_project(&self, since: i64) -> PortResult<Vec<(PathBuf, AccountId, UsageSample)>>;
    /// Every record of a project since `since`, oldest first, as (time, usage).
    fn usage_points(&self, project: &Path, since: i64) -> PortResult<Vec<(i64, UsageSample)>>;
    /// What each task of a project spent, all its records together.
    fn usage_by_task(&self, project: &Path) -> PortResult<Vec<(TaskId, UsageSample)>>;
    fn set_rate_limit(&self, account: &AccountId, limit: &RateLimit) -> PortResult<()>;
    fn rate_limit(&self, account: &AccountId) -> PortResult<Option<RateLimit>>;
    fn set_plan_usage(&self, account: &AccountId, record: &PlanUsageRecord) -> PortResult<()>;
    fn plan_usage(&self, account: &AccountId) -> PortResult<Option<PlanUsageRecord>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNote {
    /// `None`: a global note.
    pub project: Option<PathBuf>,
    pub text: String,
    pub category: Category,
    pub author: String,
    pub status: NoteStatus,
    pub created_at: i64,
    pub topic: Option<String>,
}

/// A newer version of an approved note (from a proposal the user approved):
/// the note keeps its id, scope, creation date and uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRevision {
    pub text: String,
    pub category: Category,
    pub author: String,
    pub topic: Option<String>,
    pub updated_at: i64,
}

pub trait MemoryRepository: Send + Sync {
    fn add_note(&self, note: &NewNote) -> PortResult<Note>;
    fn note(&self, id: i64) -> PortResult<Option<Note>>;
    fn update_note(&self, id: i64, text: &str, category: Category) -> PortResult<()>;
    fn revise_note(&self, id: i64, revision: &NoteRevision) -> PortResult<()>;
    fn set_note_status(&self, id: i64, status: NoteStatus) -> PortResult<()>;
    fn delete_note(&self, id: i64) -> PortResult<()>;
    /// The project's notes, plus global ones when `with_global`. `None`: only global.
    fn notes(&self, project: Option<&Path>, with_global: bool) -> PortResult<Vec<Note>>;
    /// Full-text search, best matches first.
    fn search_notes(
        &self,
        project: Option<&Path>,
        with_global: bool,
        query: &str,
    ) -> PortResult<Vec<Note>>;
    fn record_uses(&self, ids: &[i64], task: Option<&TaskId>, at: i64) -> PortResult<()>;
    fn notes_used_by(&self, task: &TaskId) -> PortResult<Vec<Note>>;
    /// How the tasks of `project` used each note they read, by note id.
    fn note_use_stats(&self, project: &Path) -> PortResult<Vec<(i64, NoteUse)>>;
    /// The user said both notes of a pair are valid.
    fn dismiss_pair(&self, a: i64, b: i64) -> PortResult<()>;
    fn dismissed_pairs(&self) -> PortResult<Vec<(i64, i64)>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSkill {
    /// `None`: a global skill.
    pub project: Option<PathBuf>,
    pub name: String,
    pub description: String,
    pub body: String,
    pub author: String,
    pub status: SkillStatus,
    pub created_at: i64,
    pub source_task: Option<TaskId>,
}

/// A newer version of a skill (an approved update or the user's edit): it
/// keeps its id, creation date and source task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRevision {
    pub project: Option<PathBuf>,
    pub name: String,
    pub description: String,
    pub body: String,
    pub author: String,
    pub updated_at: i64,
}

pub trait SkillRepository: Send + Sync {
    fn add_skill(&self, skill: &NewSkill) -> PortResult<Skill>;
    fn skill(&self, id: i64) -> PortResult<Option<Skill>>;
    fn revise_skill(&self, id: i64, revision: &SkillRevision) -> PortResult<()>;
    fn set_skill_status(&self, id: i64, status: SkillStatus) -> PortResult<()>;
    fn delete_skill(&self, id: i64) -> PortResult<()>;
    /// The project's skills, plus global ones when `with_global`; `None`:
    /// only global. Sorted by name, then id.
    fn skills(&self, project: Option<&Path>, with_global: bool) -> PortResult<Vec<Skill>>;
}

/// Messages written while an agent works, waiting for the end of its turn.
pub trait QueueRepository: Send + Sync {
    fn add_queued(&self, task: &TaskId, text: &str, at: i64) -> PortResult<QueuedMessage>;
    /// The task's queue, oldest first.
    fn queued(&self, task: &TaskId) -> PortResult<Vec<QueuedMessage>>;
    fn queued_message(&self, id: i64) -> PortResult<Option<QueuedMessage>>;
    fn update_queued(&self, id: i64, text: &str) -> PortResult<()>;
    fn delete_queued(&self, ids: &[i64]) -> PortResult<()>;
    /// `Some`: holds every message of the task that is not held yet (keeping
    /// the reason of those that are). `None`: releases them all.
    fn hold_queue(&self, task: &TaskId, reason: Option<HoldReason>) -> PortResult<()>;
}

/// A stored template row: a template of the user, or an edited (or deleted)
/// built-in example. Built-ins that were never touched have no row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateRecord {
    /// 0 for a new row.
    pub id: i64,
    pub project: Option<PathBuf>,
    pub name: String,
    pub text: String,
    pub builtin: Option<String>,
    /// A built-in example the user deleted.
    pub deleted: bool,
}

pub trait TemplateRepository: Send + Sync {
    /// Every row, oldest first.
    fn template_records(&self) -> PortResult<Vec<TemplateRecord>>;
    /// Inserts (id 0) or updates the row. Returns its id.
    fn save_template(&self, record: &TemplateRecord) -> PortResult<i64>;
    fn delete_template(&self, id: i64) -> PortResult<()>;
    /// Forgets every change to the built-in examples.
    fn reset_builtin_templates(&self) -> PortResult<()>;
}

/// A saved flow of the user: global (`project: None`) or for one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowRecord {
    /// 0 for a new row.
    pub id: i64,
    pub project: Option<PathBuf>,
    pub flow: Flow,
    pub updated_at: i64,
}

/// A role of the user's library: global or for one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleRecord {
    /// 0 for a new row.
    pub id: i64,
    pub project: Option<PathBuf>,
    pub role: RoleDef,
    pub updated_at: i64,
}

/// A flow run's blackboard (docs/decisions.md §42): a table of its own,
/// since the board server writes it from another process.
pub trait BoardRepository: Send + Sync {
    /// The task's entries, oldest first.
    fn board(&self, task: &TaskId) -> PortResult<Vec<crate::domain::flow::BoardEntry>>;
    fn post_board(&self, task: &TaskId, entry: &crate::domain::flow::BoardEntry) -> PortResult<()>;
    /// Deletes the task's board (the run is over).
    fn clear_board(&self, task: &TaskId) -> PortResult<()>;
}

/// The board an agent of a flow reads and writes: through Hedy (the API
/// agent) or the board MCP server (CLI agents). Every answer is text.
pub trait BoardTools: Send + Sync {
    /// The latest of each section the current role sees, or one section's history.
    fn read(&self, section: Option<&str>) -> String;
    fn post(&self, section: &str, text: &str) -> String;
}

/// A flow's board for one agent run (the API agent's native tools).
#[derive(Clone)]
pub struct BoardLink(pub Arc<dyn BoardTools>);

impl std::fmt::Debug for BoardLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BoardLink")
    }
}

/// Two links are the same board when they share its tools.
impl PartialEq for BoardLink {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for BoardLink {}

/// The orchestrator's flows and roles (docs/decisions.md §42).
pub trait FlowRepository: Send + Sync {
    /// Every saved flow, oldest first.
    fn flow_records(&self) -> PortResult<Vec<FlowRecord>>;
    /// Inserts (id 0) or updates the row. Returns its id.
    fn save_flow(&self, record: &FlowRecord) -> PortResult<i64>;
    fn delete_flow(&self, id: i64) -> PortResult<()>;
    /// Every saved role, oldest first.
    fn role_records(&self) -> PortResult<Vec<RoleRecord>>;
    /// Inserts (id 0) or updates the row. Returns its id.
    fn save_role(&self, record: &RoleRecord) -> PortResult<i64>;
    fn delete_role(&self, id: i64) -> PortResult<()>;
}

/// The slash commands each account's CLI last reported (docs/decisions.md §32).
pub trait CommandRepository: Send + Sync {
    /// Replaces the list of `account` for `project` (`None`: outside any
    /// project, e.g. a conversation).
    fn set_agent_commands(
        &self,
        account: &AccountId,
        project: Option<&Path>,
        known: &KnownCommands,
    ) -> PortResult<()>;
    /// Every list stored for `account`, with its project, newest first.
    fn agent_commands(
        &self,
        account: &AccountId,
    ) -> PortResult<Vec<(Option<PathBuf>, KnownCommands)>>;
}

/// Health of the storage itself, for `hedy doctor`.
pub trait StoreHealth: Send + Sync {
    /// The folder holding the database, when it lives on disk.
    fn data_dir(&self) -> Option<PathBuf>;
    /// A quick integrity check (`PRAGMA quick_check` in SQLite). Empty when
    /// the database is healthy; otherwise, the problems found.
    fn integrity_problems(&self) -> PortResult<Vec<String>>;
}

/// Persistent state. One adapter usually implements every repository.
/// A conflict waiting for the user: two versions of a note or skill made
/// apart on two PCs (docs/decisions.md §34).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConflict {
    pub id: i64,
    pub kind: ItemKind,
    pub uid: String,
    /// The version that won (the newer), in use on every PC.
    pub kept: Version,
    /// The other one, offered to the user.
    pub other: Version,
    pub at: i64,
}

/// A local item synced as `uid`, and the version its row reflects (its
/// base): what tells an edit made here from one made elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncLink {
    pub uid: String,
    /// A note's or template's row id, a theme's id.
    pub local_id: String,
    pub base_stamp: Stamp,
    /// `None`: the row was linked to a deletion (never kept).
    pub base_body: Option<String>,
}

/// The local side of sync (docs/decisions.md §34, §35): the replica of every
/// synced item as last known, how far each other PC was read, and
/// conflicts, kept apart per `space` (`""` for personal sync, `team:<id>`
/// for a team); plus the links between personal sync's items and the rows
/// here.
pub trait SyncRepository: Send + Sync {
    fn sync_item(
        &self,
        space: &str,
        kind: ItemKind,
        uid: &str,
    ) -> PortResult<Option<(Scope, Version)>>;
    fn sync_items(&self, space: &str) -> PortResult<Vec<Change>>;
    fn put_sync_item(&self, space: &str, change: &Change) -> PortResult<()>;
    fn sync_link(&self, kind: ItemKind, uid: &str) -> PortResult<Option<SyncLink>>;
    /// The link of a local item (a note's or template's row id, a theme's
    /// id).
    fn sync_link_of(&self, kind: ItemKind, local_id: &str) -> PortResult<Option<SyncLink>>;
    fn sync_links(&self, kind: ItemKind) -> PortResult<Vec<SyncLink>>;
    /// Links `uid` to the local item, which now reflects `base`. A local id
    /// linked to another uid is moved.
    fn link_sync_item(
        &self,
        kind: ItemKind,
        uid: &str,
        local_id: &str,
        base: &Version,
    ) -> PortResult<()>;
    fn unlink_sync_item(&self, kind: ItemKind, uid: &str) -> PortResult<()>;
    /// The last batch read from `device` (0: none).
    fn sync_cursor(&self, space: &str, device: &DeviceId) -> PortResult<u64>;
    fn set_sync_cursor(&self, space: &str, device: &DeviceId, seq: u64) -> PortResult<()>;
    fn add_sync_conflict(
        &self,
        space: &str,
        kind: ItemKind,
        uid: &str,
        kept: &Version,
        other: &Version,
        at: i64,
    ) -> PortResult<i64>;
    fn sync_conflicts(&self, space: &str) -> PortResult<Vec<SyncConflict>>;
    fn delete_sync_conflict(&self, id: i64) -> PortResult<()>;
    /// Forgets everything a space kept here (replica, cursors, conflicts,
    /// and personal sync's links): this PC leaves it. Notes and the rest
    /// stay.
    fn clear_sync_state(&self, space: &str) -> PortResult<()>;
    /// `None`: not decided yet (its memory stays on this PC).
    fn memory_home(&self, project: &Path) -> PortResult<Option<MemoryHome>>;
    fn set_memory_home(&self, project: &Path, home: Option<&MemoryHome>) -> PortResult<()>;
    /// Writes a note that came from another PC: `id` to replace one, `None`
    /// to add it. Returns its id.
    fn write_synced_note(
        &self,
        id: Option<i64>,
        project: Option<&Path>,
        body: &NoteBody,
    ) -> PortResult<i64>;
    /// Likewise for a skill.
    fn write_synced_skill(
        &self,
        id: Option<i64>,
        project: Option<&Path>,
        body: &SkillBody,
    ) -> PortResult<i64>;
}

pub trait Store:
    AccountRepository
    + ProjectRepository
    + SettingsRepository
    + TaskRepository
    + UsageRepository
    + MemoryRepository
    + SkillRepository
    + QueueRepository
    + TemplateRepository
    + FlowRepository
    + BoardRepository
    + CommandRepository
    + SyncRepository
    + StoreHealth
{
}
impl<T> Store for T where
    T: AccountRepository
        + ProjectRepository
        + SettingsRepository
        + TaskRepository
        + UsageRepository
        + MemoryRepository
        + SkillRepository
        + QueueRepository
        + TemplateRepository
        + FlowRepository
        + BoardRepository
        + CommandRepository
        + SyncRepository
        + StoreHealth
{
}

pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
    /// Unix seconds of the first moment matching `wall` (a date and time in
    /// a time zone, the year left out) from an hour before `after` (Unix
    /// seconds). `None` when the zone is unknown or the date does not exist.
    fn resolve_wall_time(&self, _wall: &WallTime, _after: i64) -> Option<i64> {
        None
    }
}

/// API keys in the OS keyring. Never used for subscription logins.
pub trait SecretStore: Send + Sync {
    fn set_api_key(&self, account: &AccountId, key: &str) -> PortResult<()>;
    /// Read only to call the provider's API; never logged or shown.
    fn api_key(&self, account: &AccountId) -> PortResult<Option<String>>;
    fn has_api_key(&self, account: &AccountId) -> PortResult<bool>;
    fn delete_api_key(&self, account: &AccountId) -> PortResult<()>;
    /// This PC's keys for a sync space (docs/decisions.md §34, §35: `""`
    /// for personal sync, `team:<id>` for a team), as the vault encodes
    /// them. Read only to sync; never logged or shown.
    fn sync_keys(&self, space: &str) -> PortResult<Option<String>>;
    fn set_sync_keys(&self, space: &str, keys: &str) -> PortResult<()>;
    fn delete_sync_keys(&self, space: &str) -> PortResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
}

/// Starts official CLIs. Implementations must drop every variable for which
/// [`crate::domain::env::is_blocked`] is true before applying `spec.env`.
pub trait ProcessRunner: Send + Sync {
    /// Runs attached to the user's terminal and waits. Returns the exit code.
    fn run_interactive(&self, spec: &LaunchSpec) -> PortResult<Option<i32>>;
    /// Runs without a terminal and captures stdout.
    fn run_captured(&self, spec: &LaunchSpec) -> PortResult<CapturedOutput>;
    /// Like [`ProcessRunner::run_captured`], but kills the process and fails
    /// when it takes longer than `timeout` (e.g. `--version` checks).
    fn run_captured_within(
        &self,
        spec: &LaunchSpec,
        timeout: std::time::Duration,
    ) -> PortResult<CapturedOutput>;
    /// Runs a check Hedy itself starts in a task's copy (a skill trial's test
    /// command), which runs code an AI wrote: only the variables a build
    /// needs (`PATH`, `HOME`, locale; never tokens or keys), in a process
    /// group of its own, killed whole on timeout or by
    /// [`ProcessRunner::stop_checks`].
    /// `group` names whose check it is (a trial), for
    /// [`ProcessRunner::stop_checks`]. On Linux with bubblewrap it runs
    /// isolated: no network, no session bus, the system read-only, the home
    /// folder empty but for the tools' caches, `reads` (the project the copy
    /// links to) read-only and only the copy writable; killed with Hedy.
    fn run_check(
        &self,
        spec: &LaunchSpec,
        timeout: std::time::Duration,
        group: &str,
        reads: &[std::path::PathBuf],
    ) -> PortResult<CapturedOutput>;
    /// Kills the checks of `group` still running (its trial was cancelled).
    fn stop_checks(&self, group: &str);
    /// This process, told apart from a later one that reuses its pid.
    /// `None` where it cannot be told.
    fn this_process(&self) -> Option<String> {
        None
    }
    /// Whether the process [`ProcessRunner::this_process`] named is still
    /// running.
    fn process_alive(&self, _process: &str) -> bool {
        true
    }
    /// Whether checks run isolated here (see [`ProcessRunner::run_check`]).
    fn isolates_checks(&self) -> bool;
    /// Names (never values) of the variables in Hedy's own environment that
    /// launched CLIs do not get, because [`crate::domain::env::is_blocked`].
    fn blocked_env_vars(&self) -> Vec<String>;
    /// Starts without waiting and without a terminal (e.g. opening the editor).
    fn spawn_detached(&self, spec: &LaunchSpec) -> PortResult<()>;
    /// Whether `program` (a name on `PATH` or a path) is installed.
    fn is_installed(&self, program: &str) -> bool;
    /// Replaces the current process when the platform allows it. Only returns on error.
    fn exec(&self, spec: &LaunchSpec) -> PortError;
}

/// Reads plan usage from sources that need more than one captured command
/// (a JSON-RPC conversation). Only official, read-only methods that never
/// send a message to the model.
pub trait UsageProbe: Send + Sync {
    /// Starts `spec` (`codex app-server --listen stdio://` for the account),
    /// does the `initialize` handshake, calls `account/rateLimits/read` and
    /// returns its `result` as JSON text. Fails after `timeout`.
    fn codex_rate_limits(
        &self,
        spec: &LaunchSpec,
        timeout: std::time::Duration,
    ) -> PortResult<String>;
    /// `GET {base_url}/users/me/balance` with the API key (Moonshot). Returns
    /// the body. The key is only sent in the header, never logged.
    fn moonshot_balance(
        &self,
        base_url: &str,
        api_key: &Secret,
        timeout: std::time::Duration,
    ) -> PortResult<String>;
}

/// Result of merging a task branch into its base branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    Merged,
    /// The main checkout is on another branch.
    NotOnBase {
        current: String,
    },
    /// The main checkout has uncommitted changes.
    Dirty,
    /// Merge conflicts; the merge was aborted.
    Conflict,
}

pub trait Git: Send + Sync {
    fn current_branch(&self, repo: &Path) -> PortResult<String>;
    fn head_commit(&self, repo: &Path) -> PortResult<String>;
    /// Adds `pattern` to `.git/info/exclude` (never to the team's `.gitignore`).
    fn ensure_excluded(&self, repo: &Path, pattern: &str) -> PortResult<()>;
    fn add_worktree(&self, repo: &Path, path: &Path, branch: &str, base: &str) -> PortResult<()>;
    /// A worktree at `base` without a branch (`--detach`), for a read-only task.
    fn add_detached_worktree(&self, repo: &Path, path: &Path, base: &str) -> PortResult<()>;
    /// Removes a worktree added with [`Git::add_detached_worktree`].
    fn remove_detached_worktree(&self, repo: &Path, path: &Path) -> PortResult<()>;
    /// Whether git ignores `path` (relative to `repo`).
    fn is_ignored(&self, repo: &Path, path: &str) -> bool;
    /// The `.env` files git ignores in `repo` (`.env`, `.env.local`,
    /// `apps/web/.env`), relative to it; not inside ignored folders.
    fn ignored_env_files(&self, repo: &Path) -> PortResult<Vec<String>>;
    /// Every change in the worktree (committed or not) since `base`.
    fn diff(&self, worktree: &Path, base: &str) -> PortResult<Vec<FileDiff>>;
    /// Commits everything; `false` when there was nothing to commit.
    fn commit_all(&self, worktree: &Path, message: &str) -> PortResult<bool>;
    fn merge(&self, repo: &Path, base_branch: &str, branch: &str) -> PortResult<MergeOutcome>;
    /// Removes the worktree and deletes its branch.
    fn remove_worktree(&self, repo: &Path, path: &Path, branch: &str) -> PortResult<()>;
    fn recent_commits(&self, repo: &Path, limit: usize) -> PortResult<Vec<Commit>>;
    /// Files with uncommitted changes in the main checkout (untracked included).
    fn uncommitted_changes(&self, repo: &Path) -> PortResult<usize>;
    /// The commit `rev` (a branch, `HEAD`...) points to.
    fn rev_parse(&self, repo: &Path, rev: &str) -> PortResult<String>;
    /// Remote-tracking branches that contain `commit`: it was pushed.
    fn remote_branches_containing(&self, repo: &Path, commit: &str) -> PortResult<Vec<String>>;
    /// Moves the current branch back to `commit` like `git reset --keep`:
    /// files with local changes it would overwrite make it fail and nothing
    /// changes; other local changes and untracked files stay as they are.
    fn reset_keep(&self, repo: &Path, commit: &str) -> PortResult<()>;
    /// Makes `dir` a new repository with one empty commit (`message`), so
    /// tasks can fork worktrees from it right away. Keeps the user's default
    /// branch name and identity.
    fn init_repo(&self, dir: &Path, message: &str) -> PortResult<()>;
    /// The repository's files, relative to `repo`: tracked ones and
    /// untracked ones git does not ignore (`.gitignore`, `.git/info/exclude`).
    fn list_files(&self, repo: &Path) -> PortResult<Vec<String>>;
    /// What is staged in `repo` (`git diff --cached`); nothing is staged here.
    fn staged_diff(&self, repo: &Path) -> PortResult<Vec<FileDiff>>;
    /// Commits in `repo`: a range (`main..HEAD`), or one commit's own change
    /// (`a1b2c3`: against its first parent, or nothing for a first
    /// commit). Read only.
    fn revs_diff(&self, repo: &Path, revs: &str) -> PortResult<Vec<FileDiff>>;
    /// The main checkout of the repository `repo` belongs to: `repo` itself,
    /// or the one a worktree was added from.
    fn main_root(&self, repo: &Path) -> PortResult<PathBuf>;
    /// What is not committed yet in `repo`, staged or not, new files not
    /// added yet included (protected ones only named, never read).
    /// Also how many new files were left out, past the most read.
    fn work_diff(&self, repo: &Path) -> PortResult<(Vec<FileDiff>, usize)>;
    /// `git commit -F <file with message>` (`-e` too when `edit`: git opens
    /// the user's editor) on what is staged, with the user's own hooks and
    /// signing settings, and git's output on the terminal (for `hedy commit`).
    fn commit_staged(&self, repo: &Path, message: &str, edit: bool) -> PortResult<()>;
}

/// Receives the agent's events, on the adapter's own thread.
pub type EventSink = Arc<dyn Fn(AgentEvent) + Send + Sync>;

/// A secret that never shows up in logs or debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// A stdio MCP server to register for one agent run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The memory tools an agent calls: through Hedy (the API agent) or the
/// memory MCP server (CLI agents). Every answer is text for the agent.
pub trait MemoryTools: Send + Sync {
    /// Approved notes matching `query` (all of them when empty), compact: id,
    /// category, topic and a short preview of each.
    fn search(&self, query: &str) -> String;
    /// One page of [`search`](Self::search), best notes first, ending with
    /// how to ask for more when there are. Fakes may ignore the page.
    fn search_page(&self, query: &str, page: PageRequest) -> String {
        let _ = page;
        self.search(query)
    }
    /// One approved note in full, by id or by topic.
    fn get(&self, id: Option<i64>, topic: Option<&str>) -> String;
    /// The topics of the approved notes, to propose changes under them.
    fn topics(&self) -> String;
    /// Proposes a note for the user to approve. Under the topic of an
    /// approved note, it proposes a change to that note.
    fn propose(&self, text: &str, category: &str, topic: Option<&str>) -> String;
}

/// The learned skills an agent reads and proposes: through Hedy (the API
/// agent) or the memory MCP server (CLI agents). Every answer is text.
pub trait SkillTools: Send + Sync {
    /// Approved skills in scope, one compact line each: name, scope and
    /// when to use it.
    fn list(&self) -> String;
    /// One page of [`list`](Self::list). Fakes may ignore the page.
    fn list_page(&self, page: PageRequest) -> String {
        let _ = page;
        self.list()
    }
    /// One approved skill in full, by name.
    fn get(&self, name: &str) -> String;
    /// Proposes a skill for the user to approve. With the name of an
    /// approved skill, it proposes an update of that skill.
    fn propose(&self, name: &str, description: &str, body: &str) -> String;
}

/// The project's memory for one agent run: an MCP server for CLIs, tools for APIs.
#[derive(Clone)]
pub struct MemoryLink {
    pub server: Option<McpServer>,
    pub tools: Arc<dyn MemoryTools>,
    /// The learned skills, served next to the memory (same server).
    pub skills: Option<Arc<dyn SkillTools>>,
    /// The team layer, for a project of a team (docs/decisions.md §35):
    /// always in the agent's context, next to the memory guidance.
    pub team: Option<String>,
}

impl std::fmt::Debug for MemoryLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryLink")
            .field("server", &self.server)
            .finish_non_exhaustive()
    }
}

impl PartialEq for MemoryLink {
    fn eq(&self, other: &Self) -> bool {
        self.server == other.server && self.team == other.team
    }
}

impl Eq for MemoryLink {}

/// Which adapter runs the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentBackend {
    ClaudeCode,
    Codex,
    /// A chat-completions API (Kimi, Gemini); Hedy runs the tools itself.
    OpenAiCompatible {
        base_url: String,
        api_key: Secret,
    },
    /// An official CLI that speaks the Agent Client Protocol (Gemini CLI,
    /// Kimi Code, Antigravity). `launch` already holds its ACP arguments.
    Acp {
        /// For messages: "Gemini CLI".
        name: String,
        /// The CLI's flag to pick a model, if it has one.
        model_flag: Option<&'static str>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRequest {
    pub backend: AgentBackend,
    /// Program, account folder variable and working directory (for APIs,
    /// only the working directory is used).
    pub launch: LaunchSpec,
    pub mode: TaskMode,
    pub phase: Phase,
    /// Start in plan mode: the agent proposes a plan before changing anything.
    pub plan_first: bool,
    pub model: Option<String>,
    pub prompt: String,
    /// Images that go with the prompt (only when the backend takes them).
    pub images: Vec<PromptImage>,
    /// CLI session to continue.
    pub resume: Option<String>,
    /// The project's shared memory, unless turned off for this account.
    pub memory: Option<MemoryLink>,
    /// Other MCP servers to register (the code map).
    pub mcp: Vec<McpServer>,
    /// A conversation (a question outside any project): the user comes and
    /// goes, so caches that outlive a pause pay off.
    pub conversation: bool,
    /// Only the tools that read files (no shell, no subagents): a shell can
    /// read a protected file without naming it (`find … | xargs cat`), so a
    /// request that only needs to read goes without one.
    pub no_shell: bool,
    /// No tools at all, no MCP server and no skill: a request that carries
    /// someone else's text (a skill, a change) and only needs to answer.
    pub no_tools: bool,
    /// Claude Code plugins for this session only (`--plugin-dir`): Hedy's
    /// approved skills, never written where the user's own `claude` reads.
    pub plugins: Vec<PathBuf>,
    /// A read-only task's own folder outside the project: the only place it
    /// may write (a document or a page it was asked for).
    pub output: Option<PathBuf>,
    /// The project's files the agent may not read or touch.
    pub blocked: crate::domain::permissions::BlockedFiles,
    /// A flow's board, for the API agent's native tools (CLI agents get the
    /// board server among `mcp`).
    pub board: Option<BoardLink>,
}

impl AgentRequest {
    /// The memory server (when the agent reads the memory) and the extra ones,
    /// such as the code map.
    pub fn mcp_servers(&self) -> Vec<&McpServer> {
        self.memory
            .iter()
            .filter_map(|m| m.server.as_ref())
            .chain(&self.mcp)
            .collect()
    }
}

pub trait AgentLauncher: Send + Sync {
    fn start(&self, request: AgentRequest, events: EventSink) -> PortResult<Box<dyn AgentSession>>;
    /// Whether the backend takes images with a prompt, through the CLI's
    /// (or API's) official input.
    fn image_support(&self, _backend: &AgentBackend) -> ImageSupport {
        ImageSupport::No
    }
}

/// A running agent process.
pub trait AgentSession: Send {
    /// A follow-up message from the user, with its images (only sent when
    /// [`AgentLauncher::image_support`] allows them).
    fn send(&mut self, text: &str, images: &[PromptImage]) -> PortResult<()>;
    /// Answers a permission (or plan) request.
    fn answer(&mut self, request_id: &str, allow: bool, message: Option<&str>) -> PortResult<()>;
    fn stop(&mut self);
}

/// Lets a driving adapter (the app) follow tasks live.
pub trait TaskEvents: Send + Sync {
    fn event(&self, task: &TaskId, event: &AgentEvent);
    fn task_updated(&self, task: &Task);
    /// The task needs the user, finished, failed or hit a limit, and the user
    /// wants to hear about it (`ui.notifications`). The adapter decides how
    /// (the app: a desktop notification while its window is not focused).
    fn notice(&self, _task: &Task, _notice: &Notice) {}
    /// The task's queue of messages changed (added, edited, removed, held,
    /// released or delivered). `queue` is what is left, oldest first.
    fn queue_changed(&self, _task: &TaskId, _queue: &[QueuedMessage]) {}
}

pub struct NoTaskEvents;

impl TaskEvents for NoTaskEvents {
    fn event(&self, _: &TaskId, _: &AgentEvent) {}
    fn task_updated(&self, _: &Task) {}
}

/// The user's custom themes, one shareable file each.
pub trait ThemeStore: Send + Sync {
    /// Every readable, valid theme; a broken one is skipped, not an error.
    fn list(&self) -> PortResult<Vec<CustomTheme>>;
    fn get(&self, id: &str) -> PortResult<Option<CustomTheme>>;
    /// Creates the theme or replaces the one with its id.
    fn save(&self, theme: &CustomTheme) -> PortResult<()>;
    /// Nothing happens when there is no such theme.
    fn delete(&self, id: &str) -> PortResult<()>;
    /// An existing folder (created when missing) where an agent asked to
    /// describe a theme can run: outside any project.
    fn work_dir(&self) -> PortResult<PathBuf>;
}

pub trait FileSystem: Send + Sync {
    /// Folder for a new Hedy-managed account.
    fn account_folder(&self, id: &AccountId) -> PathBuf;
    /// Where conversations outside any project get their scratch folders.
    fn chats_dir(&self) -> PathBuf;
    /// Where images attached to messages are kept, one folder per task.
    /// Never inside a repository.
    fn attachments_dir(&self) -> PathBuf;
    /// Where read-only tasks write what they are asked for (documents,
    /// pages), one folder per task. Never inside a repository.
    fn outputs_dir(&self) -> PathBuf;
    /// The user's home folder, if known.
    fn home_dir(&self) -> Option<PathBuf>;
    /// Creates `path` (and missing parents). Fails when `path` already
    /// exists: nothing is ever overwritten.
    fn create_new_dir(&self, path: &Path) -> PortResult<()>;
    /// Deletes a folder and everything in it. Only for folders Hedy created.
    fn remove_dir_all(&self, path: &Path) -> PortResult<()>;
    /// Creates `path` (and parents), readable only by the user: account
    /// folders hold CLI logins.
    fn create_private_dir(&self, path: &Path) -> PortResult<()>;
    fn exists(&self, path: &Path) -> bool;
    /// Whether `path` itself is a symbolic link (not followed).
    fn is_symlink(&self, path: &Path) -> bool;
    /// The entries of a folder; empty when it does not exist.
    fn entries(&self, dir: &Path) -> PortResult<Vec<PathBuf>>;
    /// Whether files can be created in `dir` (writes and removes a probe file).
    fn check_writable(&self, dir: &Path) -> PortResult<()>;
    /// `link` → `target`.
    fn symlink_dir(&self, target: &Path, link: &Path) -> PortResult<()>;
    /// Writes `contents` unless the file already exists.
    fn write_if_missing(&self, path: &Path, contents: &str) -> PortResult<()>;
    /// A text file's contents; `None` when it does not exist.
    fn read_text(&self, path: &Path) -> PortResult<Option<String>>;
    /// Creates or replaces a file, creating its parent folders.
    fn write_text(&self, path: &Path, contents: &str) -> PortResult<()>;
    /// Creates a file (and its parent folders) only when nothing is at
    /// `path`, not even a link: `false` when something is.
    fn create_text(&self, path: &Path, contents: &str) -> PortResult<bool>;
    /// A regular file's text, at most `max_bytes`; `None` when it does not
    /// exist. Fails for a link, a device or a pipe, a bigger file or one that
    /// is not UTF-8: someone else's folder can hold any of them.
    fn read_regular_text(&self, path: &Path, max_bytes: u64) -> PortResult<Option<String>>;
    /// Writes a file readable only by the user (its folder too), creating
    /// the folder when missing. Fails when the file already exists.
    fn write_private_file(&self, path: &Path, contents: &[u8]) -> PortResult<()>;
    /// A file's bytes; `None` when it does not exist.
    fn read_bytes(&self, path: &Path) -> PortResult<Option<Vec<u8>>>;
    /// The regular files under `dir` (links and what they lead to left out),
    /// relative to it with `/`, and their sizes. Empty when `dir` is missing.
    fn files_beneath(&self, dir: &Path) -> PortResult<Vec<(String, u64)>>;
    /// Copies a regular file to `to`, which must not exist yet: nothing is
    /// ever overwritten.
    fn copy_to_new(&self, from: &Path, to: &Path) -> PortResult<()>;
    /// Deletes a file; nothing happens when it does not exist.
    fn remove_file(&self, path: &Path) -> PortResult<()>;
    /// Creates or replaces `rel` inside `root` (making its folders), never
    /// through a link: not at the file, not at any folder of `rel`, even one
    /// swapped in meanwhile (each step is opened from the one before).
    fn write_text_beneath(&self, root: &Path, rel: &Path, contents: &str) -> PortResult<()>;
    /// Deletes `rel` inside `root` the same way (nothing when it is missing).
    fn remove_file_beneath(&self, root: &Path, rel: &Path) -> PortResult<()>;
    /// Deletes the folder `rel` inside `root` the same way, only when it is
    /// empty (nothing when it is missing or holds anything; an error when a
    /// link is on the way).
    fn remove_empty_dir_beneath(&self, root: &Path, rel: &Path) -> PortResult<()>;
    /// Deletes a folder only when it exists and is empty.
    fn remove_empty_dir(&self, path: &Path) -> PortResult<()>;
    /// Canonical form of `dir`.
    fn canonicalize(&self, dir: &Path) -> PortResult<PathBuf>;
    /// Root of the git repository containing `dir`, if any.
    fn git_root(&self, dir: &Path) -> Option<PathBuf>;
}

/// Read-only access to a copy of the code and its git history, for code
/// navigation and review. `dir` is where git runs: the user's folder or a
/// task's worktree. `rev: None` reads the working files in `dir`; `Some`
/// reads the commit (or tree) `rev` instead. Paths are relative to `dir`,
/// already checked by the use case ([`crate::domain::code::normalize_code_path`]).
/// Nothing here writes to the repository (not even the index).
pub trait CodeReader: Send + Sync {
    /// Every file in the commit `rev` (`git ls-tree -r`).
    fn files_at(&self, dir: &Path, rev: &str) -> PortResult<Vec<String>>;
    /// Untracked files git ignores, in the working copy; whole ignored
    /// folders are collapsed into one entry ending with `/`.
    fn ignored_files(&self, dir: &Path) -> PortResult<Vec<String>>;
    /// What changed from `base` to `to` (a commit), or, with `to: None`, to
    /// the working files: tracked changes (staged or not) plus untracked
    /// files git does not ignore (`ChangeKind::Untracked`).
    fn changes(&self, dir: &Path, base: &str, to: Option<&str>) -> PortResult<Vec<PathChange>>;
    /// The first `max` bytes of a file and its size; `None` when it does not
    /// exist there. Working files are read through symbolic links only when
    /// they stay inside `dir` (otherwise an error).
    fn read_file(
        &self,
        dir: &Path,
        rev: Option<&str>,
        path: &str,
        max: u64,
    ) -> PortResult<Option<FileBytes>>;
    /// A file's size; `None` when it does not exist there.
    fn file_size(&self, dir: &Path, rev: Option<&str>, path: &str) -> PortResult<Option<u64>>;
    /// The unified diff of one file from `base` to `to` (or the working
    /// file), with `context` lines around changes; `old_path` is the other
    /// side of a rename. `None` when nothing changed.
    fn diff_file(
        &self,
        dir: &Path,
        base: &str,
        to: Option<&str>,
        path: &str,
        old_path: Option<&str>,
        context: u32,
    ) -> PortResult<Option<FileDiff>>;
    /// Lines matching `query` (`git grep`; untracked files included when
    /// reading working files; binary files skipped), never in files whose
    /// name matches one of `exclude` (shell globs on the file name) nor
    /// under `exclude_dirs`. At most `max` lines; `true` when there were more.
    fn grep(
        &self,
        dir: &Path,
        rev: Option<&str>,
        query: &TextQuery,
        exclude: &[&str],
        exclude_dirs: &[&str],
        max: usize,
    ) -> PortResult<(Vec<(String, LineMatch)>, bool)>;
    /// Commits that touched the file, newest first, following renames
    /// (`git log --follow`), from `rev` or `HEAD`. With `lines` (1-based,
    /// inclusive, as in that commit), only those that touched that range
    /// (`git log -L`).
    fn file_log(
        &self,
        dir: &Path,
        rev: Option<&str>,
        path: &str,
        lines: Option<(u32, u32)>,
        max: usize,
    ) -> PortResult<Vec<CommitInfo>>;
    /// Who wrote each line (`git blame --porcelain`) of the working file or
    /// of `rev`; with `lines`, only that range.
    fn blame(
        &self,
        dir: &Path,
        rev: Option<&str>,
        path: &str,
        lines: Option<(u32, u32)>,
    ) -> PortResult<Blame>;
}

/// What is at a path of a copy's working files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    File,
    Folder,
    /// A symbolic link (to anything); renamed or deleted as the link itself.
    Link,
}

/// Light editing of a copy's working files (the user's folder or a task's
/// worktree), kept apart from [`CodeReader`], which never writes. Paths are
/// relative to `dir`, already checked by the use case; the adapter still
/// refuses anything whose parent folder resolves outside `dir`. Nothing here
/// touches git (not even the index).
pub trait CodeWriter: Send + Sync {
    /// What is at `path`, without following a final symbolic link; `None`
    /// when nothing is there.
    fn kind(&self, dir: &Path, path: &str) -> PortResult<Option<PathKind>>;
    /// Replaces an existing regular file's contents, keeping its
    /// permissions (through a temporary file renamed over it).
    fn replace_file(&self, dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()>;
    /// Creates a file (and missing parent folders); fails when anything is
    /// already there: nothing is overwritten.
    fn create_file(&self, dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()>;
    /// Creates a folder (and missing parents); fails when it exists.
    fn create_folder(&self, dir: &Path, path: &str) -> PortResult<()>;
    /// Moves `from` to `to` (missing parent folders created); fails when
    /// `to` exists, unless it is `from` itself (a change of case).
    fn rename(&self, dir: &Path, from: &str, to: &str) -> PortResult<()>;
    /// Copies a regular file with its permissions; fails when `to` exists.
    fn copy_file(&self, dir: &Path, from: &str, to: &str) -> PortResult<()>;
    /// Every path inside a folder (files, folders and links, not followed),
    /// relative to `dir`, at most `max`; `true` when there were more.
    fn walk(&self, dir: &Path, folder: &str, max: usize) -> PortResult<(Vec<String>, bool)>;
    /// Moves a file or folder to the system's trash (recoverable there).
    fn trash(&self, dir: &Path, path: &str) -> PortResult<()>;
}

/// A branch's remote-tracking branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    /// The remote ("origin").
    pub remote: String,
    /// The branch on the remote ("main").
    pub branch: String,
    /// Commits the local branch has that the upstream does not.
    pub ahead: u32,
    /// Commits the upstream has that the local branch does not.
    pub behind: u32,
}

impl Upstream {
    /// "origin/main".
    pub fn name(&self) -> String {
        format!("{}/{}", self.remote, self.branch)
    }
}

/// The main folder's state and its remote, through the system's git with
/// the user's own credential helpers: Hedy stores nothing. Never forces.
pub trait SourceControl: Send + Sync {
    /// Branch, upstream, ahead/behind and uncommitted files of the folder.
    fn folder(&self, repo: &Path) -> PortResult<crate::domain::ship::GitFolder>;
    /// What `branch` tracks, if anything.
    fn upstream(&self, repo: &Path, branch: &str) -> PortResult<Option<Upstream>>;
    fn remotes(&self, repo: &Path) -> PortResult<Vec<String>>;
    /// The remote's URL (only its host is ever shown).
    fn remote_url(&self, repo: &Path, remote: &str) -> PortResult<Option<String>>;
    /// The remote's default branch as git knows it locally
    /// (`refs/remotes/<remote>/HEAD`, else its `main` or `master`).
    fn default_branch(&self, repo: &Path, remote: &str) -> Option<String>;
    /// `git push [-u] <remote> <branch>:<remote_branch>`, never forced. The
    /// error carries git's own message.
    fn push(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
        remote_branch: &str,
        set_upstream: bool,
    ) -> PortResult<()>;
    /// `git pull --ff-only` on the current branch. The error carries git's
    /// own message.
    fn pull_ff_only(&self, repo: &Path) -> PortResult<()>;
    /// Removes a task's worktree but keeps its branch (and its commits).
    fn remove_worktree_keep_branch(&self, repo: &Path, path: &Path) -> PortResult<()>;
    /// A worktree on an existing branch.
    fn attach_worktree(&self, repo: &Path, path: &Path, branch: &str) -> PortResult<()>;
}

/// What GitHub CLI is asked to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestSpec {
    pub title: String,
    pub body: String,
    pub base: String,
    pub head: String,
    pub draft: bool,
}

/// Pull requests through GitHub CLI (`gh`), with the user's own login.
/// Hedy never reads its token.
pub trait CodeHost: Send + Sync {
    fn installed(&self) -> bool;
    /// Whether `gh auth status --hostname <host>` succeeds.
    fn logged_in(&self, repo: &Path, host: &str) -> bool;
    /// The repository's default branch as the host tells it.
    fn default_branch(&self, repo: &Path) -> Option<String>;
    /// `gh pr create`; returns what it printed (the link). The error
    /// carries gh's own message.
    fn create_pull_request(&self, repo: &Path, spec: &PullRequestSpec) -> PortResult<String>;
}

/// Held while one sync operation runs; dropping it lets the next one in.
pub struct SyncLock {
    _held: Box<dyn Send>,
}

impl SyncLock {
    pub fn new(held: impl Send + 'static) -> Self {
        Self {
            _held: Box::new(held),
        }
    }
}

/// The sync repo's working copy (docs/decisions.md §34), in `dir`. Paths are
/// relative to the repo and made by `domain::sync::layout`. Any git server
/// works: the user's own credentials, no token kept by Hedy.
pub trait SyncRemote: Send + Sync {
    /// Waits for, then holds, the lock next to `dir`: one sync operation
    /// at a time on this PC, also across processes (the app and `hedy`).
    fn lock(&self, dir: &Path) -> PortResult<SyncLock>;
    /// Whether anyone can read `url` without credentials (a public repo).
    fn is_public(&self, url: &str) -> bool;
    /// This machine's own name, for the list of PCs.
    fn device_name(&self) -> String;
    /// Clones `url` into `dir`, or opens the clone already there.
    fn open(&self, url: &str, dir: &Path) -> PortResult<()>;
    /// Brings in what other PCs pushed.
    fn pull(&self, dir: &Path) -> PortResult<()>;
    /// Files under `prefix`, sorted.
    fn files(&self, dir: &Path, prefix: &str) -> PortResult<Vec<String>>;
    fn read(&self, dir: &Path, path: &str) -> PortResult<Option<Vec<u8>>>;
    fn write(&self, dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()>;
    fn remove(&self, dir: &Path, path: &str) -> PortResult<()>;
    /// Commits what changed (if anything) and pushes it; when another PC
    /// pushed first, pulls and tries again.
    fn push(&self, dir: &Path, message: &str) -> PortResult<()>;
    /// Forgets everything not pushed (the clone is made again on the next
    /// `open`): a setup or join the repo refused.
    fn discard_local(&self, dir: &Path) -> PortResult<()>;
}

/// The sync vault's cryptography (`adapters::vault`), with keys and epoch
/// files as opaque text: secrets never take another shape here.
pub trait SyncCrypto: Send + Sync {
    /// `bytes` random bytes, as lowercase hex.
    fn random_hex(&self, bytes: usize) -> PortResult<String>;
    /// New keys for this PC.
    fn new_member(&self) -> PortResult<String>;
    /// A new recovery key: (its 12 words, its keys).
    fn new_recovery(&self) -> PortResult<(String, String)>;
    /// The recovery key's keys from its words.
    fn recover(&self, words: &str) -> PortResult<String>;
    /// A member's public keys (as epoch files list them).
    fn member_public(&self, secrets: &str) -> PortResult<String>;
    /// A new epoch with a new key for `members` (of a team's vault, or a
    /// personal one), signed by `signer`: (the epoch file, its key).
    fn new_epoch(
        &self,
        epoch: u32,
        team: bool,
        members: &[EpochMember],
        signer_id: &str,
        signer: &str,
    ) -> PortResult<(String, String)>;
    /// Whether `public` is a member's public keys as epoch files list
    /// them (both keys well formed).
    fn check_member_public(&self, public: &str) -> PortResult<()>;
    /// Whether `file` is epoch 1, signed by the recovery key `recovery`.
    fn verify_first_epoch(&self, file: &str, recovery: &str) -> PortResult<()>;
    /// Whether `next` may follow `previous`.
    fn verify_next_epoch(&self, previous: &str, next: &str) -> PortResult<()>;
    /// An epoch file's number, kind and members.
    fn epoch_members(&self, file: &str) -> PortResult<EpochInfo>;
    /// The epoch's key, for member `id`.
    fn open_epoch(&self, file: &str, id: &str, secrets: &str) -> PortResult<String>;
    fn seal(&self, key: &str, path: &str, plaintext: &[u8]) -> PortResult<Vec<u8>>;
    /// Opens with whichever of `keys` sealed it.
    fn open(&self, keys: &[String], path: &str, sealed: &[u8]) -> PortResult<Vec<u8>>;
    /// The sealed name of a project in the repo.
    fn project_id(&self, key: &str, project_key: &str) -> PortResult<String>;
    /// SHA-256 of `bytes`, as lowercase hex: what pins a verified epoch.
    fn digest(&self, bytes: &[u8]) -> String;
    /// Signs `bytes` with a member's signing key.
    fn sign(&self, secrets: &str, bytes: &[u8]) -> PortResult<String>;
    /// Whether `signature` is `public`'s over `bytes`.
    fn verify(&self, public: &str, bytes: &[u8], signature: &str) -> PortResult<()>;

    // Team invites (`adapters::invite`). `context` names what the invite is
    // for (the team repo), the same on both PCs. Invite keys are text of
    // their own: they seal and open with `invite_seal` and `invite_open`
    // only, never as a vault key.

    /// A new code for invite number `nameplate` (`7-tigre-lima-nube-mar`).
    fn invite_code(&self, nameplate: u32) -> PortResult<String>;
    /// A code as typed: (the invite's number, the code in the form both PCs
    /// use); `None` when it is not one (a mistyped word), so a typo never
    /// burns an invite.
    fn parse_invite_code(&self, typed: &str) -> Option<(u32, String)>;
    /// The inviting PC's side: (its secret, to keep until the invite is
    /// used; its message, for the invite file).
    fn invite_start(&self, code: &str, context: &str) -> PortResult<(String, String)>;
    /// The inviting PC reads the joiner's message: the key they share. With
    /// a wrong code it is another key, which opens nothing the joiner sealed.
    /// Each call is one guess: burn the invite before making it.
    fn invite_accept(
        &self,
        secret: &str,
        code: &str,
        context: &str,
        message: &str,
    ) -> PortResult<String>;
    /// The joiner's side: (its message, the key it shares if the code was
    /// right).
    fn invite_join(&self, code: &str, context: &str, message: &str)
    -> PortResult<(String, String)>;
    fn invite_seal(&self, key: &str, path: &str, plaintext: &[u8]) -> PortResult<Vec<u8>>;
    fn invite_open(&self, key: &str, path: &str, sealed: &[u8]) -> PortResult<Vec<u8>>;
}
