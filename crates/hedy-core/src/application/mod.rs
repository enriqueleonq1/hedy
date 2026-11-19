//! Use cases. They depend only on [`ports`], never on concrete adapters.

mod accounts;
mod activity;
mod ask;
mod attachments;
mod briefing;
mod carry;
mod code;
mod code_edit;
mod commands;
mod commit;
mod conversations;
mod detect;
mod doctor;
mod error;
mod files;
mod launch;
mod memory;
mod memory_home;
mod metrics;
mod one_shot;
mod plan_usage;
pub mod ports;
mod projects;
mod queue;
mod review;
mod settings;
mod ship;
mod skills;
mod status;
mod sync;
mod tasks;
mod templates;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod testing_code;
mod themes;
mod waits;

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub use accounts::NewAccount;
pub use activity::{MAX_FINISHED, RECENT_HOURS, TaskActivity};
pub use ask::{AskScope, MemoryAnswer};
pub use attachments::NewImage;
pub use carry::{Carry, ConversationContext};
pub use code::{
    BASE_REV, CodeCopy, CodeDiff, CodeTree, CopyInfo, FileContent, FileHistory, HistoryCommit,
    MAX_DIFF_CONTEXT, TaskTouch,
};
pub use code_edit::{Saved, WriteOutcome};
pub use commit::StagedSuggestion;
pub use conversations::NewConversation;
pub use detect::DetectedCli;
pub use doctor::{Check, CheckArea, CheckLevel, DoctorReport};
pub use error::{AppError, AppResult, UndoBlocked};
pub use files::MAX_PROJECT_FILES;
pub use launch::{Resolved, RunLimitWarning};
pub use memory::{
    MemoryServerCommand, MemoryView, Proposal, format_note, format_note_page, format_notes,
    format_topics,
};
pub use memory_home::MemoryHomeStatus;
pub use metrics::{ProjectStatus, TestRun, UsageSummary};
pub use plan_usage::{LimitCheck, PLAN_USAGE_MIN_INTERVAL_SECS, PlanUsageState};
pub use projects::NewProject;
pub use review::AcceptPreview;
pub use settings::{CUSTOM_THEME_PREFIX, DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, Setting, THEMES};
pub use ship::{
    FolderStatus, NewPullRequest, PullRequestDraft, Pulled, Pushed, ShipError, ShipState,
};
pub use skills::{
    SkillChange, SkillDraft, SkillFiles, SkillProposal, SkillsView, format_skill,
    format_skill_page, format_skills,
};
pub use status::{AccountStatus, AccountUsage, Status};
pub use sync::{
    SyncDevice, SyncReport, SyncStatus, TeamInvite, TeamJoin, TeamNote, TeamPerson, TeamStatus,
    TeamSuggestion, TeamSummary, TeamWriteOutcome,
};
pub use tasks::NewTask;
pub use templates::{NewTemplate, TemplateScope};
pub use themes::NewTheme;

use crate::domain::TaskId;
use ports::{
    AgentLauncher, Clock, CodeHost, CodeReader, CodeWriter, FileSystem, Git, NoTaskEvents,
    ProcessRunner, SecretStore, SourceControl, Store, SyncCrypto, SyncRemote, TaskEvents,
    ThemeStore, UsageProbe,
};

/// Everything the use cases need from the outside world.
pub struct Ports {
    pub store: Arc<dyn Store>,
    pub secrets: Arc<dyn SecretStore>,
    pub runner: Arc<dyn ProcessRunner>,
    pub fs: Arc<dyn FileSystem>,
    pub git: Arc<dyn Git>,
    pub code: Arc<dyn CodeReader>,
    pub writer: Arc<dyn CodeWriter>,
    pub agents: Arc<dyn AgentLauncher>,
    pub clock: Arc<dyn Clock>,
    pub usage: Arc<dyn UsageProbe>,
    pub source: Arc<dyn SourceControl>,
    pub host: Arc<dyn CodeHost>,
    pub themes: Arc<dyn ThemeStore>,
    pub sync_remote: Arc<dyn SyncRemote>,
    pub sync_crypto: Arc<dyn SyncCrypto>,
}

/// Application facade used by every driving adapter (the CLI and the app).
#[derive(Clone)]
pub struct Hedy {
    store: Arc<dyn Store>,
    secrets: Arc<dyn SecretStore>,
    runner: Arc<dyn ProcessRunner>,
    fs: Arc<dyn FileSystem>,
    git: Arc<dyn Git>,
    code: Arc<dyn CodeReader>,
    writer: Arc<dyn CodeWriter>,
    agents: Arc<dyn AgentLauncher>,
    clock: Arc<dyn Clock>,
    usage: Arc<dyn UsageProbe>,
    source: Arc<dyn SourceControl>,
    host: Arc<dyn CodeHost>,
    themes: Arc<dyn ThemeStore>,
    sync_remote: Arc<dyn SyncRemote>,
    sync_crypto: Arc<dyn SyncCrypto>,
    events: Arc<dyn TaskEvents>,
    /// Running agent sessions, by task.
    live: Arc<Mutex<HashMap<TaskId, tasks::LiveTask>>>,
    /// Numbers sessions, so late events from a replaced session are ignored.
    sessions: Arc<AtomicU64>,
    /// Serializes changes to message queues with their delivery.
    queue_lock: Arc<Mutex<()>>,
    /// One personal sync at a time (the timer, "Sincronizar ahora").
    sync_lock: Arc<Mutex<()>>,
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
            code: ports.code,
            writer: ports.writer,
            agents: ports.agents,
            clock: ports.clock,
            usage: ports.usage,
            source: ports.source,
            host: ports.host,
            themes: ports.themes,
            sync_remote: ports.sync_remote,
            sync_crypto: ports.sync_crypto,
            events: Arc::new(NoTaskEvents),
            live: Arc::default(),
            sessions: Arc::default(),
            queue_lock: Arc::default(),
            sync_lock: Arc::default(),
            memory_server: None,
        }
    }

    /// Where task events go besides storage (the app's UI).
    pub fn with_task_events(mut self, events: Arc<dyn TaskEvents>) -> Self {
        self.events = events;
        self
    }
}
