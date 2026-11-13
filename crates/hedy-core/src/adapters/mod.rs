//! Port implementations and the composition root.

mod fs;
mod keyring;
mod paths;
mod process;
mod sqlite;

use std::sync::Arc;

pub use fs::LocalFileSystem;
pub use keyring::KeyringSecretStore;
pub use paths::HedyPaths;
pub use process::SystemProcessRunner;
pub use sqlite::SqliteStore;

use crate::application::Hedy;
use crate::application::ports::PortResult;

/// Wires the real adapters together.
pub fn system(paths: &HedyPaths) -> PortResult<Hedy> {
    let store = SqliteStore::open(&paths.database())?;
    Ok(Hedy::new(
        Arc::new(store),
        Arc::new(KeyringSecretStore::default()),
        Arc::new(SystemProcessRunner),
        Arc::new(LocalFileSystem::new(paths.accounts_dir())),
    ))
}
