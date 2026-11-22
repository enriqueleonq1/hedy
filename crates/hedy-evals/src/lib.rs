//! Evals: real tasks, run with and without each of Hedy's layers (project
//! memory, code map) and experiments (variants), to measure whether one
//! improves the results (spec phase 6, docs/decisions.md §33). Each run is a
//! real agent session and spends tokens.
//!
//! A case is a folder with `case.json`, the starting `repo/` and a hidden
//! `check/` folder that is copied into the task's worktree only after the
//! agent finishes, so the agent never sees the checks.

pub mod case;
pub mod report;
pub mod runner;

pub use case::{Case, CaseSpec, Layer, NoteSpec, Variant, load_suite, select_cases};
pub use report::{BASE, Comparison, RunResult, StepTrace, Summary, Verdict, compare, summarize};
pub use runner::{RemoveOnDrop, Runner, RunnerOptions};
