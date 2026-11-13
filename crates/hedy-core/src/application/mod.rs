//! Use cases. They depend only on [`ports`], never on concrete adapters.

mod accounts;
mod error;
mod launch;
pub mod ports;
mod projects;
mod settings;
mod status;
#[cfg(test)]
mod testing;

use std::sync::Arc;

pub use accounts::NewAccount;
pub use error::{AppError, AppResult};
pub use launch::Resolved;
pub use settings::Setting;
pub use status::{AccountStatus, Status};

use ports::{FileSystem, ProcessRunner, SecretStore, Store};

/// Application facade used by every driving adapter (CLI now, Tauri later).
#[derive(Clone)]
pub struct Hedy {
    store: Arc<dyn Store>,
    secrets: Arc<dyn SecretStore>,
    runner: Arc<dyn ProcessRunner>,
    fs: Arc<dyn FileSystem>,
}

impl Hedy {
    pub fn new(
        store: Arc<dyn Store>,
        secrets: Arc<dyn SecretStore>,
        runner: Arc<dyn ProcessRunner>,
        fs: Arc<dyn FileSystem>,
    ) -> Self {
        Self {
            store,
            secrets,
            runner,
            fs,
        }
    }
}
