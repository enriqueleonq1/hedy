//! `--long`: what helps a long task across sessions? (evals/experiments/long-tasks).
//! Each run is three sessions, each a new task (a new agent process, no
//! resume, nothing shared but the code), cut at a cap of actions. What a
//! session leaves is committed before the next starts. The user's messages are
//! the same in every arm; what the arm adds goes after them:
//!
//! - `solo`: nothing;
//! - `repeat`: the request, again, in every session;
//! - `doc`: a task document Hedy keeps: the request as written, the change once
//!   asked, and each session's progress;
//! - `flow`: as `solo`, each session run as the `essential` flow, under the
//!   same cap.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use hedy_core::experiments;

use crate::rescue::CloseOnDrop;
use crate::runner::{RemoveOnDrop, Runner, changes_since, git, head, prepare_repo, save_diff};
use crate::{Case, Layer, LongTrace, RunResult, Variant};

pub const ARMS: [&str; 4] = ["solo", "repeat", "doc", "flow"];
pub const SESSIONS: u32 = 3;
/// The template a `flow` session runs as (docs/decisions.md §51).
const FLOW: &str = "essential";

/// What one session left, for the task document.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub files: Vec<String>,
    pub tests_passed: bool,
    pub said: String,
}

/// The user's message in `session` (1–3), the same in every arm.
pub fn user_message(session: u32, prompt: &str, change: &str) -> String {
    match session {
        1 => format!("Implementa esto:\n\n{prompt}"),
        2 => "Continúa.".to_owned(),
        _ => format!("Cambia esto y termina: {change}"),
    }
}

/// The task document: the request as written, the change once asked, and
/// what each session left.
pub fn task_document(prompt: &str, change: Option<&str>, progress: &[Progress]) -> String {
    let mut doc = format!(
        "Documento de la tarea (lo mantiene Hedy; el pedido está tal como lo escribió el usuario).\n\n## Pedido\n\n{prompt}\n\n## Cambios pedidos después\n\n{}\n\n## Avance\n",
        change.unwrap_or("(ninguno)")
    );
    if progress.is_empty() {
        doc.push_str("\n(todavía nada)\n");
    }
    for (n, p) in progress.iter().enumerate() {
        let files = if p.files.is_empty() {
            "ninguno".to_owned()
        } else {
            p.files.join(", ")
        };
        let said: String = p.said.chars().take(1500).collect();
        doc.push_str(&format!(
            "\n### Sesión {}\n\nArchivos cambiados: {files}\nTests del proyecto: {}\nLo último que dijo la IA: {}\n",
            n + 1,
            if p.tests_passed { "pasan" } else { "fallan" },
            if said.trim().is_empty() { "(nada)" } else { said.trim() }
        ));
    }
    doc
}

/// What the agent gets in `session` with `arm`: the user's message, then
/// what the arm adds.
pub fn session_prompt(
    arm: &str,
    session: u32,
    prompt: &str,
    change: &str,
    progress: &[Progress],
) -> String {
    let message = user_message(session, prompt, change);
    if session == 1 {
        return message;
    }
    match arm {
        "repeat" => format!("{message}\n\nLo que había que hacer:\n\n{prompt}"),
        "doc" => {
            let change = (session == SESSIONS).then_some(change);
            format!("{message}\n\n{}", task_document(prompt, change, progress))
        }
        _ => message,
    }
}

/// The `REQS R01=1 R02=0 … REG=1` line a long case's check prints.
pub fn parse_reqs(output: &str) -> BTreeMap<String, bool> {
    output
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("REQS "))
        .map(|line| {
            line.split_whitespace()
                .filter_map(|pair| pair.split_once('='))
                .map(|(id, v)| (id.to_owned(), v == "1"))
                .collect()
        })
        .unwrap_or_default()
}

/// The files a patch touches.
fn files_in(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter_map(|l| l.strip_prefix("diff --git a/"))
        .filter_map(|l| l.split(" b/").next())
        .map(str::to_owned)
        .collect()
}

impl Runner {
    /// One run of `arm`: a result per session. A session that fails stops
    /// the run (its later sessions are not recorded).
    pub fn run_long(
        &self,
        case: &Case,
        layer: Layer,
        variant: &Variant,
        arm: &str,
        attempt: u32,
        cap: Option<u32>,
    ) -> Vec<RunResult> {
        let started_at = crate::runner::now();
        experiments::set(variant.experiments);
        let name = format!("{}+long:{arm}", variant.name);
        let dir = self.options.work_dir.join(format!(
            "{}-{layer}-long-{arm}-{attempt}-{started_at}",
            case.name
        ));
        let _dir = RemoveOnDrop(dir.clone());
        let mut results = Vec::new();
        let outcome = (|| -> Result<()> {
            let repo = prepare_repo(case, &dir)?;
            let done = self.prepare_project(case, layer, &repo).and_then(|_| {
                self.long_sessions(
                    case,
                    arm,
                    &repo,
                    cap,
                    &mut |n| {
                        self.new_result(case, layer, &name, attempt, started_at)
                            .with_session(n)
                    },
                    &mut results,
                )
            });
            self.forget_project(&repo);
            done
        })();
        if let Err(e) = outcome {
            let mut failed = self.new_result(case, layer, &name, attempt, started_at);
            failed.error = Some(format!("{e:#}"));
            failed.long = Some(LongTrace {
                session: results.len() as u32 + 1,
                reqs: BTreeMap::new(),
                tests_passed: false,
                reads_before_edit: None,
                stopped: false,
            });
            results.push(failed);
        }
        results
    }

    fn long_sessions(
        &self,
        case: &Case,
        arm: &str,
        repo: &Path,
        cap: Option<u32>,
        new: &mut dyn FnMut(u32) -> RunResult,
        results: &mut Vec<RunResult>,
    ) -> Result<()> {
        let spec = &case.spec;
        let change = spec.change.as_deref().context("el caso no tiene change")?;
        let changed = spec
            .check_changed
            .as_deref()
            .context("el caso no tiene checkChanged")?;
        let tests = spec.tests.as_deref().context("el caso no tiene tests")?;
        let mut progress = Vec::new();
        for session in 1..=SESSIONS {
            let mut out = new(session);
            let base = head(repo)?;
            let prompt = session_prompt(arm, session, &spec.prompt, change, &progress);
            let clock = Instant::now();
            let request = self.new_task(repo, &prompt);
            let task = match arm {
                "flow" => self.hedy.start_flow(request, FLOW)?,
                _ => self.hedy.start_task(request)?,
            };
            let close = CloseOnDrop(self, task.id.clone());
            let seen = self.drive(&task.id, Duration::from_secs(spec.timeout_secs), false, cap);
            out.duration_ms = clock.elapsed().as_millis() as u64;
            seen.add_to(&mut out);
            let work = self.hedy.task(&task.id)?.work_dir();
            let patch = changes_since(&work, &base)?;
            drop(close);
            if let Some(e) = &seen.error {
                results.push(out);
                bail!("sesión {session}: {e}");
            }
            // What the session left is the next one's start.
            if !patch.trim().is_empty() {
                let file = repo.join(".git").join("hedy-evals-session.patch");
                std::fs::write(&file, &patch)?;
                git(repo, &["apply", "--binary", &file.to_string_lossy()])?;
                std::fs::remove_file(&file)?;
                git(repo, &["add", "-A"])?;
                git(repo, &["commit", "-q", "-m", &format!("Sesión {session}")])?;
            }
            let tests_passed = self.in_copy(repo, None, tests)?.0;
            let check = if session == SESSIONS {
                changed
            } else {
                &spec.check
            };
            (out.passed, out.check_output) = self.in_copy(repo, Some(&case.checks()), check)?;
            out.long = Some(LongTrace {
                session,
                reqs: parse_reqs(&out.check_output),
                tests_passed,
                reads_before_edit: seen.reads_before_edit,
                stopped: seen.stopped,
            });
            if let Some(dir) = &self.options.diffs {
                let mut named = out.clone();
                named.variant = format!("{}-s{session}", out.variant);
                out.diff = Some(save_diff(dir, &named, &patch)?);
            }
            progress.push(Progress {
                files: files_in(&patch),
                tests_passed,
                said: seen.last_message.clone().unwrap_or_default(),
            });
            results.push(out);
        }
        Ok(())
    }
}

impl RunResult {
    fn with_session(mut self, session: u32) -> Self {
        self.long = Some(LongTrace {
            session,
            reqs: BTreeMap::new(),
            tests_passed: false,
            reads_before_edit: None,
            stopped: false,
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_arm_sends_the_same_user_message() {
        for arm in ARMS {
            let first = session_prompt(arm, 1, "R1. Algo.", "Otra cosa.", &[]);
            assert_eq!(first, "Implementa esto:\n\nR1. Algo.");
            for session in 2..=3 {
                let message = user_message(session, "R1. Algo.", "Otra cosa.");
                assert!(
                    session_prompt(arm, session, "R1. Algo.", "Otra cosa.", &[])
                        .starts_with(&message)
                );
            }
        }
        assert_eq!(session_prompt("solo", 2, "R1.", "x", &[]), "Continúa.");
        assert_eq!(
            session_prompt("flow", 3, "R1.", "x", &[]),
            "Cambia esto y termina: x"
        );
        assert!(session_prompt("repeat", 2, "R1. Algo.", "x", &[]).ends_with("R1. Algo."));
    }

    #[test]
    fn the_task_document_keeps_the_request_and_the_progress() {
        let progress = [Progress {
            files: vec!["gym/bookings.py".into()],
            tests_passed: false,
            said: "Hice R1 y R2.".into(),
        }];
        let second = session_prompt("doc", 2, "R1. Algo.", "Otra cosa.", &progress);
        assert!(second.starts_with("Continúa.\n\nDocumento de la tarea"));
        assert!(second.contains("## Pedido\n\nR1. Algo."));
        assert!(
            second.contains("(ninguno)"),
            "the change comes only in session 3"
        );
        assert!(second.contains("Archivos cambiados: gym/bookings.py"));
        assert!(second.contains("Tests del proyecto: fallan"));
        let third = session_prompt("doc", 3, "R1. Algo.", "Otra cosa.", &progress);
        assert!(third.contains("## Cambios pedidos después\n\nOtra cosa."));
        assert!(task_document("R1.", None, &[]).contains("(todavía nada)"));
    }

    #[test]
    fn reads_the_requirements_line() {
        let output = "Ran 3 tests\nOK\nREQS R01=1 R02=0 REG=1\n";
        let reqs = parse_reqs(output);
        assert_eq!(reqs.len(), 3);
        assert!(reqs["R01"]);
        assert!(!reqs["R02"]);
        assert!(parse_reqs("Traceback").is_empty());
    }

    #[test]
    fn lists_the_files_a_patch_touches() {
        let patch = "diff --git a/a.py b/a.py\n+x\ndiff --git a/tests/t.py b/tests/t.py\n";
        assert_eq!(files_in(patch), ["a.py", "tests/t.py"]);
    }
}
