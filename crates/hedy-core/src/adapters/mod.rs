//! Port implementations and the composition root.

mod claude;
mod fs;
mod git;
mod keyring;
mod paths;
mod process;
mod pty;
mod sqlite;

use std::sync::Arc;

pub use claude::ClaudeCodeLauncher;
pub use fs::LocalFileSystem;
pub use git::GitCli;
pub use keyring::KeyringSecretStore;
pub use paths::HedyPaths;
pub use process::SystemProcessRunner;
pub use pty::{PtyOutput, PtySession};
pub use sqlite::SqliteStore;

use crate::application::ports::{Clock, PortResult};
use crate::application::{Hedy, Ports};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }
}

/// Wires the real adapters together.
pub fn system(paths: &HedyPaths) -> PortResult<Hedy> {
    let store = SqliteStore::open(&paths.database())?;
    Ok(Hedy::new(Ports {
        store: Arc::new(store),
        secrets: Arc::new(KeyringSecretStore::default()),
        runner: Arc::new(SystemProcessRunner),
        fs: Arc::new(LocalFileSystem::new(paths.accounts_dir())),
        git: Arc::new(GitCli),
        agents: Arc::new(ClaudeCodeLauncher),
        clock: Arc::new(SystemClock),
    }))
}
