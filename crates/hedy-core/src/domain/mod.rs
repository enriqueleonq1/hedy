//! Pure domain: no I/O, no processes, no storage.

pub mod account;
pub mod activity;
pub mod agent;
pub mod ask;
pub mod attachment;
pub mod briefing;
pub mod carry;
pub mod code;
pub mod color;
pub mod command;
pub mod context;
pub mod diff;
pub mod env;
pub mod error;
pub mod flow;
pub mod flow_text;
pub mod launch;
pub mod login;
pub mod memory;
pub mod metrics;
pub mod permissions;
pub mod phase;
pub mod plan_usage;
pub mod policy;
pub mod project;
pub mod prompts;
pub mod provider;
pub mod queue;
pub mod read_guard;
pub mod recall;
pub mod role_text;
pub mod rule_check;
pub mod ship;
pub mod skill;
pub mod skill_check;
pub mod skill_import;
pub mod skill_trial;
pub mod sync;
pub mod task;
pub mod team;
pub mod template;
pub mod theme;
pub mod version;
pub mod working;

pub use account::{Account, AccountId, ConfigFolder, Credentials};
pub use activity::{Notice, NoticeKind, TaskState, task_state};
pub use agent::{
    AgentEvent, Decision, LimitLevel, PlanStep, ReviewWarning, StepStatus, UsageSample,
};
pub use attachment::{Attachment, ImageSupport, ImageType, PromptImage};
pub use carry::{ContextTurn, TaskOrigin};
pub use color::Color;
pub use command::{AgentCommand, CommandSource, KnownCommands};
pub use diff::{Commit, FileDiff};
pub use error::DomainError;
pub use launch::{LaunchPurpose, LaunchSpec};
pub use login::LoginState;
pub use memory::{Category, Note, NoteStatus};
pub use phase::{Assignment, Phase, PhasePlan, Stage};
pub use plan_usage::{
    LimitAlert, PlanUsage, UsageProblem, UsageSource, UsageWindow, WindowKind, short_names,
};
pub use project::Project;
pub use provider::Provider;
pub use queue::{HoldReason, QueuedMessage};
pub use ship::{AcceptMode, GhState, PullRequest, Shipment};
pub use skill::{Skill, SkillStatus};
pub use task::{
    LimitWait, MergeRecord, OutputFile, PermissionMode, Task, TaskId, TaskMode, TaskStatus,
    Workspace,
};
pub use template::{PromptTemplate, TemplateId};
pub use theme::{CustomTheme, ThemeBase, ThemeDraft, ThemeOrigin};
pub use working::{Preset, WorkingStyle};
