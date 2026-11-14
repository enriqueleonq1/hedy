//! Ports: what the use cases need from the outside world. Adapters implement them;
//! tests use in-memory fakes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::domain::{
    Account, AccountId, AgentEvent, Category, Commit, FileDiff, LaunchSpec, Note, NoteStatus,
    Phase, Project, Task, TaskId, TaskMode, UsageSample,
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
    fn append_event(&self, id: &TaskId, at: i64, event: &AgentEvent) -> PortResult<()>;
    fn events(&self, id: &TaskId) -> PortResult<Vec<AgentEvent>>;
    /// Events with the time they were recorded (seconds).
    fn timed_events(&self, id: &TaskId) -> PortResult<Vec<(i64, AgentEvent)>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageRecord {
    pub at: i64,
    pub account: AccountId,
    pub project: PathBuf,
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

pub trait UsageRepository: Send + Sync {
    fn record_usage(&self, record: &UsageRecord) -> PortResult<()>;
    /// Sum of everything recorded for `account` since `since` (seconds).
    fn usage_since(&self, account: &AccountId, since: i64) -> PortResult<UsageSample>;
    /// Totals per project since `since`.
    fn usage_by_project(&self, since: i64) -> PortResult<Vec<(PathBuf, UsageSample)>>;
    /// Every record of a project since `since`, oldest first, as (time, usage).
    fn usage_points(&self, project: &Path, since: i64) -> PortResult<Vec<(i64, UsageSample)>>;
    fn set_rate_limit(&self, account: &AccountId, limit: &RateLimit) -> PortResult<()>;
    fn rate_limit(&self, account: &AccountId) -> PortResult<Option<RateLimit>>;
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
        + StoreHealth
{
}

pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
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

/// The project's memory for one agent run: an MCP server for CLIs, tools for APIs.
#[derive(Clone)]
pub struct MemoryLink {
    pub server: Option<McpServer>,
    pub tools: Arc<dyn MemoryTools>,
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
}

/// A running agent process.
pub trait AgentSession: Send {
    /// A follow-up message from the user.
    fn send(&mut self, text: &str) -> PortResult<()>;
    /// Answers a permission (or plan) request.
    fn answer(&mut self, request_id: &str, allow: bool, message: Option<&str>) -> PortResult<()>;
    fn stop(&mut self);
}

/// Lets a driving adapter (the app) follow tasks live.
pub trait TaskEvents: Send + Sync {
    fn event(&self, task: &TaskId, event: &AgentEvent);
    fn task_updated(&self, task: &Task);
}

pub struct NoTaskEvents;

impl TaskEvents for NoTaskEvents {
    fn event(&self, _: &TaskId, _: &AgentEvent) {}
    fn task_updated(&self, _: &Task) {}
}

pub trait FileSystem: Send + Sync {
    /// Folder for a new Hedy-managed account.
    fn account_folder(&self, id: &AccountId) -> PathBuf;
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
    /// Canonical form of `dir`.
    fn canonicalize(&self, dir: &Path) -> PortResult<PathBuf>;
    /// Root of the git repository containing `dir`, if any.
    fn git_root(&self, dir: &Path) -> Option<PathBuf>;
}
