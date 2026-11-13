//! Ports: what the use cases need from the outside world. Adapters implement them;
//! tests use in-memory fakes.

use std::path::{Path, PathBuf};

use crate::domain::{Account, AccountId, LaunchSpec, Project};

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
}

pub trait SettingsRepository: Send + Sync {
    fn get_setting(&self, key: &str) -> PortResult<Option<String>>;
    fn set_setting(&self, key: &str, value: Option<&str>) -> PortResult<()>;
}

/// Persistent state. One adapter usually implements all three repositories.
pub trait Store: AccountRepository + ProjectRepository + SettingsRepository {}
impl<T: AccountRepository + ProjectRepository + SettingsRepository> Store for T {}

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
    /// Replaces the current process when the platform allows it. Only returns on error.
    fn exec(&self, spec: &LaunchSpec) -> PortError;
}

pub trait FileSystem: Send + Sync {
    /// Folder for a new Hedy-managed account.
    fn account_folder(&self, id: &AccountId) -> PathBuf;
    fn create_dir_all(&self, path: &Path) -> PortResult<()>;
    /// Writes `contents` unless the file already exists.
    fn write_if_missing(&self, path: &Path, contents: &str) -> PortResult<()>;
    /// Canonical form of `dir`.
    fn canonicalize(&self, dir: &Path) -> PortResult<PathBuf>;
    /// Root of the git repository containing `dir`, if any.
    fn git_root(&self, dir: &Path) -> Option<PathBuf>;
}
