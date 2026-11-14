//! Hedy core: accounts, projects and launching the official coding CLIs.
//!
//! Hexagonal layout:
//! - [`domain`]: pure types and rules, no I/O.
//! - [`application`]: use cases, written against the traits in [`application::ports`].
//! - [`adapters`]: implementations of those ports (SQLite, keyring, processes, files).

pub mod adapters;
pub mod application;
pub mod domain;
pub mod i18n;
