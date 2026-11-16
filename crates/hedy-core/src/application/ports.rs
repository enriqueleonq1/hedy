//! Ports: what the use cases need from the outside world. Adapters implement them;
//! tests use in-memory fakes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::domain::code::{Blame, CommitInfo, FileBytes, LineMatch, PathChange, TextQuery};
use crate::domain::{
    Account, AccountId, AgentEvent, Category, Commit, FileDiff, HoldReason, ImageSupport,
    LaunchSpec, Note, NoteStatus, Notice, Phase, PlanUsage, Project, PromptImage, QueuedMessage,
    Skill, SkillStatus, Task, TaskId, TaskMode, UsageProblem, UsageSample, plan_usage::WallTime,
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

/// Health of the storage itself, for `hedy doctor`.
pub trait StoreHealth: Send + Sync {
    /// The folder holding the database, when it lives on disk.
    fn data_dir(&self) -> Option<PathBuf>;
    /// A quick integrity check (`PRAGMA quick_check` in SQLite). Empty when
    /// the database is healthy; otherwise, the problems found.
    fn integrity_problems(&self) -> PortResult<Vec<String>>;
}

/// Persistent state. One adapter usually implements every repository.
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
    /// Whether git ignores `path` (relative to `repo`).
    fn is_ignored(&self, repo: &Path, path: &str) -> bool;
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
        self.server == other.server
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
    /// Writes a file readable only by the user (its folder too), creating
    /// the folder when missing. Fails when the file already exists.
    fn write_private_file(&self, path: &Path, contents: &[u8]) -> PortResult<()>;
    /// A file's bytes; `None` when it does not exist.
    fn read_bytes(&self, path: &Path) -> PortResult<Option<Vec<u8>>>;
    /// Deletes a file; nothing happens when it does not exist.
    fn remove_file(&self, path: &Path) -> PortResult<()>;
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
