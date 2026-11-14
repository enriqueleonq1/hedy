//! Pure domain: no I/O, no processes, no storage.

pub mod account;
pub mod agent;
pub mod color;
pub mod diff;
pub mod env;
pub mod error;
pub mod launch;
pub mod login;
pub mod memory;
pub mod metrics;
pub mod permissions;
pub mod phase;
pub mod policy;
pub mod project;
pub mod prompts;
pub mod provider;
pub mod task;

pub use account::{Account, AccountId, ConfigFolder, Credentials};
pub use agent::{AgentEvent, Decision, LimitLevel, PlanStep, StepStatus, UsageSample};
pub use color::Color;
pub use diff::{Commit, FileDiff};
pub use error::DomainError;
pub use launch::{LaunchPurpose, LaunchSpec};
pub use login::LoginState;
pub use memory::{Category, Note, NoteStatus};
pub use phase::{Assignment, Phase, PhasePlan, Stage};
pub use project::Project;
pub use provider::Provider;
pub use task::{Task, TaskId, TaskMode, TaskStatus, Workspace};
