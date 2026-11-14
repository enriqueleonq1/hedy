//! Use cases. They depend only on [`ports`], never on concrete adapters.

mod accounts;
mod activity;
mod detect;
mod error;
mod launch;
mod memory;
mod metrics;
pub mod ports;
mod projects;
mod settings;
mod status;
mod tasks;
#[cfg(test)]
mod testing;

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub use accounts::NewAccount;
pub use activity::{MAX_FINISHED, RECENT_HOURS, TaskActivity};
pub use detect::DetectedCli;
pub use error::{AppError, AppResult};
pub use launch::Resolved;
pub use memory::{MemoryServerCommand, MemoryView, format_notes};
pub use metrics::{ProjectStatus, TestRun, UsageSummary};
pub use settings::{Setting, THEMES};
pub use status::{AccountStatus, AccountUsage, Status};
pub use tasks::NewTask;

use crate::domain::TaskId;
use ports::{
    AgentLauncher, Clock, FileSystem, Git, NoTaskEvents, ProcessRunner, SecretStore, Store,
    TaskEvents,
};

/// Everything the use cases need from the outside world.
pub struct Ports {
    pub store: Arc<dyn Store>,
    pub secrets: Arc<dyn SecretStore>,
    pub runner: Arc<dyn ProcessRunner>,
    pub fs: Arc<dyn FileSystem>,
    pub git: Arc<dyn Git>,
    pub agents: Arc<dyn AgentLauncher>,
    pub clock: Arc<dyn Clock>,
}

/// Application facade used by every driving adapter (the CLI and the app).
#[derive(Clone)]
pub struct Hedy {
    store: Arc<dyn Store>,
    secrets: Arc<dyn SecretStore>,
    runner: Arc<dyn ProcessRunner>,
    fs: Arc<dyn FileSystem>,
    git: Arc<dyn Git>,
    agents: Arc<dyn AgentLauncher>,
    clock: Arc<dyn Clock>,
    events: Arc<dyn TaskEvents>,
    /// Running agent sessions, by task.
    live: Arc<Mutex<HashMap<TaskId, tasks::LiveTask>>>,
    /// Numbers sessions, so late events from a replaced session are ignored.
    sessions: Arc<AtomicU64>,
    /// How to start the memory MCP server for CLI agents.
    memory_server: Option<MemoryServerCommand>,
}

impl Hedy {
    pub fn new(ports: Ports) -> Self {
        Self {
            store: ports.store,
            secrets: ports.secrets,
            runner: ports.runner,
            fs: ports.fs,
            git: ports.git,
            agents: ports.agents,
            clock: ports.clock,
            events: Arc::new(NoTaskEvents),
            live: Arc::default(),
            sessions: Arc::default(),
            memory_server: None,
        }
    }

    /// Where task events go besides storage (the app's UI).
    pub fn with_task_events(mut self, events: Arc<dyn TaskEvents>) -> Self {
        self.events = events;
        self
    }
}
