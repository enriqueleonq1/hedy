//! Pure domain: no I/O, no processes, no storage.

pub mod account;
pub mod color;
pub mod env;
pub mod error;
pub mod launch;
pub mod login;
pub mod policy;
pub mod project;
pub mod provider;

pub use account::{Account, AccountId, ConfigFolder, Credentials};
pub use color::Color;
pub use error::DomainError;
pub use launch::{LaunchPurpose, LaunchSpec};
pub use login::LoginState;
pub use project::Project;
pub use provider::Provider;
