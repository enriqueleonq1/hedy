//! Ports: what the use cases need from the outside world. Adapters implement them;
//! tests use in-memory fakes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::domain::{
    Account, AccountId, AgentEvent, Commit, FileDiff, LaunchSpec, Project, Task, TaskId, TaskMode,
    UsageSample,
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
    pub utilization: Option<f64>,
    pub resets_at: Option<i64>,
    pub at: i64,
}

pub trait UsageRepository: Send + Sync {
    fn record_usage(&self, record: &UsageRecord) -> PortResult<()>;
    /// Sum of everything recorded for `account` since `since` (seconds).
    fn usage_since(&self, account: &AccountId, since: i64) -> PortResult<UsageSample>;
    fn set_rate_limit(&self, account: &AccountId, limit: &RateLimit) -> PortResult<()>;
    fn rate_limit(&self, account: &AccountId) -> PortResult<Option<RateLimit>>;
}

/// Persistent state. One adapter usually implements every repository.
pub trait Store:
    AccountRepository + ProjectRepository + SettingsRepository + TaskRepository + UsageRepository
{
}
impl<T> Store for T where
    T: AccountRepository
        + ProjectRepository
        + SettingsRepository
        + TaskRepository
        + UsageRepository
{
}

pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
}

/// API keys in the OS keyring. Never used for subscription logins.
pub trait SecretStore: Send + Sync {
    fn set_api_key(&self, account: &AccountId, key: &str) -> PortResult<()>;
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
    /// Starts without waiting and without a terminal (e.g. opening the editor).
    fn spawn_detached(&self, spec: &LaunchSpec) -> PortResult<()>;
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
}

/// Receives the agent's events, on the adapter's own thread.
pub type EventSink = Arc<dyn Fn(AgentEvent) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRequest {
    /// Program, account folder variable and working directory.
    pub launch: LaunchSpec,
    pub mode: TaskMode,
    /// Start in plan mode: the agent proposes a plan before changing anything.
    pub plan_first: bool,
    pub model: Option<String>,
    pub prompt: String,
    /// CLI session to continue.
    pub resume: Option<String>,
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
    fn create_dir_all(&self, path: &Path) -> PortResult<()>;
    fn exists(&self, path: &Path) -> bool;
    /// `link` → `target`.
    fn symlink_dir(&self, target: &Path, link: &Path) -> PortResult<()>;
    /// Writes `contents` unless the file already exists.
    fn write_if_missing(&self, path: &Path, contents: &str) -> PortResult<()>;
    /// Canonical form of `dir`.
    fn canonicalize(&self, dir: &Path) -> PortResult<PathBuf>;
    /// Root of the git repository containing `dir`, if any.
    fn git_root(&self, dir: &Path) -> Option<PathBuf>;
}
