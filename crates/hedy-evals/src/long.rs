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
//!
//! The confirmation test (evals/experiments/confirmation, [`Plan::UntilDone`])
//! cuts each session at a budget of tokens, asks the change with the second
//! session and goes on until a session after it ends by itself. Its arms:
//!
//! - `today`: what Hedy sends today when a task goes on in another session
//!   (the handoff to another agent: the request, the files changed so far);
//! - `request`: the request as written, then «Continúa.»;
//! - `doc`: the task document, as above;
//! - `flow`: as `today`, each session run as the `essential` flow.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use hedy_core::domain::{ConfigFolder, Credentials, Provider, UsageSample, prompts};
use hedy_core::experiments;

use crate::rescue::CloseOnDrop;
use crate::runner::{Cut, RemoveOnDrop, Runner, changes_since, git, head, prepare_repo, save_diff};
use crate::{Case, Layer, LongTrace, RunResult, Variant};

pub const ARMS: [&str; 4] = ["solo", "repeat", "doc", "flow"];
/// The confirmation test's arms ([`Plan::UntilDone`]).
pub const CONFIRM_ARMS: [&str; 4] = ["today", "request", "doc", "flow"];
pub const SESSIONS: u32 = 3;
/// With [`Plan::UntilDone`], the session whose message asks the change.
pub const CHANGE_SESSION: u32 = 2;

/// How a `--long` run goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Test C: three sessions, the change asked in the third.
    Three,
    /// The change asked in the second session; the run ends when a session
    /// from then on ends by itself, or after `max` sessions.
    UntilDone { max: u32 },
}

/// How a session is cut.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// Actions per session.
    pub cap: Option<u32>,
    /// Tokens at input price per session ([`at_input_price`]).
    pub budget: Option<u64>,
}

/// Tokens weighed by their price, counted as input tokens: Claude's output
/// costs 5×, a cache read 0.1× and a cache write 1.25× an input token. A
/// session's budget is counted in these, the same for every model.
pub fn at_input_price(usage: &UsageSample) -> u64 {
    usage.input_tokens
        + 5 * usage.output_tokens
        + usage.cache_read_tokens / 10
        + usage.cache_write_tokens * 5 / 4
}
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

/// The user's change, asked with the second session in every arm.
pub fn change_message(change: &str) -> String {
    format!("Además, un cambio al pedido: {change}")
}

/// What the agent gets in `session` with `arm` ([`Plan::UntilDone`]): the
/// request alone in the first; then what the arm sends to go on, with the
/// change after it in [`CHANGE_SESSION`]. `agent`: who worked before, as
/// Hedy names it in a handoff.
pub fn continue_prompt(
    arm: &str,
    session: u32,
    prompt: &str,
    change: &str,
    progress: &[Progress],
    agent: &str,
) -> String {
    if session == 1 {
        return prompt.to_owned();
    }
    let asked = (session >= CHANGE_SESSION).then_some(change);
    let base = match arm {
        "request" => format!("{prompt}\n\nContinúa."),
        "doc" => format!("Continúa.\n\n{}", task_document(prompt, asked, progress)),
        // Hedy's handoff (`Hedy::handoff_task`): its files are those changed
        // since the task started.
        _ => {
            let mut files: Vec<String> = progress
                .iter()
                .flat_map(|p| p.files.iter().cloned())
                .collect();
            files.sort();
            files.dedup();
            prompts::handoff_prompt(prompt, None, agent, &files)
        }
    };
    if session == CHANGE_SESSION {
        format!("{base}\n\n{}", change_message(change))
    } else {
        base
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

/// What the sessions logged in `folder` spent: each assistant message's
/// usage, once (a message is logged again as it streams). A stopped agent never
/// reports its turn's usage, but Claude Code logs every message as it goes.
pub fn logged_usage(folder: &Path) -> UsageSample {
    let mut by_message: BTreeMap<String, UsageSample> = BTreeMap::new();
    let logs = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"));
    for log in logs {
        let Ok(text) = std::fs::read_to_string(&log) else {
            continue;
        };
        for line in text.lines() {
            let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if entry["type"] != "assistant" {
                continue;
            }
            let message = &entry["message"];
            let usage = &message["usage"];
            let n = |k: &str| usage[k].as_u64().unwrap_or(0);
            let id = message["id"]
                .as_str()
                .map_or_else(|| entry["uuid"].to_string(), str::to_owned);
            by_message.insert(
                id,
                UsageSample {
                    input_tokens: n("input_tokens"),
                    output_tokens: n("output_tokens"),
                    cache_read_tokens: n("cache_read_input_tokens"),
                    cache_write_tokens: n("cache_creation_input_tokens"),
                    cost_usd: None,
                },
            );
        }
    }
    let mut total = UsageSample::default();
    for usage in by_message.values() {
        total.add(usage);
    }
    total
}

/// Where Claude Code logs the sessions it ran in `work`, for the account's
/// folder (`None`: `~/.claude`): its path with every other character a `-`.
pub fn session_logs(folder: Option<&Path>, work: &Path) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let folder = match folder {
        Some(f) => f.to_owned(),
        None => home?.join(".claude"),
    };
    let name: String = work
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(folder.join("projects").join(name))
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
    #[allow(clippy::too_many_arguments)]
    pub fn run_long(
        &self,
        case: &Case,
        layer: Layer,
        variant: &Variant,
        arm: &str,
        attempt: u32,
        plan: Plan,
        limits: Limits,
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
                    plan,
                    limits,
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

    #[allow(clippy::too_many_arguments)]
    fn long_sessions(
        &self,
        case: &Case,
        arm: &str,
        repo: &Path,
        plan: Plan,
        limits: Limits,
        new: &mut dyn FnMut(u32) -> RunResult,
        results: &mut Vec<RunResult>,
    ) -> Result<()> {
        let spec = &case.spec;
        let change = spec.change.as_deref().context("el caso no tiene change")?;
        let changed = match plan {
            Plan::Three => spec
                .check_changed
                .as_deref()
                .context("el caso no tiene checkChanged")?,
            // Its check judges the change with the rest.
            Plan::UntilDone { .. } => &spec.check,
        };
        let tests = spec.tests.as_deref().context("el caso no tiene tests")?;
        let sessions = match plan {
            Plan::Three => SESSIONS,
            Plan::UntilDone { max } => max,
        };
        let agent = Provider::Claude.agent_label(self.options.model.as_deref());
        let start = head(repo)?;
        let mut progress = Vec::new();
        for session in 1..=sessions {
            let mut out = new(session);
            let base = head(repo)?;
            let prompt = match plan {
                Plan::Three => session_prompt(arm, session, &spec.prompt, change, &progress),
                Plan::UntilDone { .. } => {
                    continue_prompt(arm, session, &spec.prompt, change, &progress, &agent)
                }
            };
            let clock = Instant::now();
            let request = self.new_task(repo, &prompt);
            let task = match arm {
                "flow" => self.hedy.start_flow(request, FLOW)?,
                _ => self.hedy.start_task(request)?,
            };
            let close = CloseOnDrop(self, task.id.clone());
            let work = self.hedy.task(&task.id)?.work_dir();
            let logs = session_logs(self.account_folder()?.as_deref(), &work);
            let cut = Cut {
                actions: limits.cap,
                tokens: limits.budget.zip(logs.clone()).map(|(b, l)| (l, b)),
            };
            let seen = self.drive(
                &task.id,
                Duration::from_secs(spec.timeout_secs),
                false,
                &cut,
            );
            out.duration_ms = clock.elapsed().as_millis() as u64;
            seen.add_to(&mut out);
            // Cut before its turn ended: what it spent is only in its logs
            // (no cost there; the analysis prices the tokens).
            if seen.stopped
                && let Some(logs) = &logs
            {
                let logged = logged_usage(logs);
                if logged.total_tokens() > out.tokens {
                    out.tokens = logged.total_tokens();
                    out.input_tokens = logged.input_tokens;
                    out.output_tokens = logged.output_tokens;
                    out.cache_read_tokens = logged.cache_read_tokens;
                    out.cache_write_tokens = logged.cache_write_tokens;
                    out.cost_usd = None;
                }
            }
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
            let check = if session == SESSIONS && plan == Plan::Three {
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
            // A session that ends by itself after the change ends the run.
            let last = session == sessions
                || matches!(plan, Plan::UntilDone { .. })
                    && session >= CHANGE_SESSION
                    && !seen.stopped;
            if let Some(dir) = &self.options.diffs {
                let mut named = out.clone();
                named.variant = format!("{}-s{session}", out.variant);
                out.diff = Some(save_diff(dir, &named, &patch)?);
                // The run's whole change, beside its last session's.
                if last && matches!(plan, Plan::UntilDone { .. }) {
                    named.variant = format!("{}-final", out.variant);
                    let whole = changes_since(repo, &start)?;
                    save_diff(dir, &named, &whole)?;
                }
            }
            progress.push(Progress {
                files: files_in(&patch),
                tests_passed,
                said: seen.last_message.clone().unwrap_or_default(),
            });
            results.push(out);
            if last {
                break;
            }
        }
        Ok(())
    }
}

impl Runner {
    /// The account's own folder (`None`: the CLI's default one).
    fn account_folder(&self) -> Result<Option<std::path::PathBuf>> {
        let account = self.hedy.account(self.options.account.as_str())?;
        Ok(match account.credentials {
            Credentials::Subscription(ConfigFolder::Managed(path)) => Some(path),
            _ => None,
        })
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
    fn the_change_comes_with_the_second_session_and_only_the_document_keeps_it() {
        hedy_core::i18n::set_language(hedy_core::i18n::Language::Spanish);
        let progress = [Progress {
            files: vec!["lib/a.py".into()],
            tests_passed: true,
            said: "Hice la mitad.".into(),
        }];
        for arm in CONFIRM_ARMS {
            assert_eq!(
                continue_prompt(arm, 1, "Haz X.", "Y.", &[], "Claude Sonnet"),
                "Haz X."
            );
            let second = continue_prompt(arm, 2, "Haz X.", "Y.", &progress, "Claude Sonnet");
            assert!(second.ends_with(&change_message("Y.")), "{arm}");
            assert!(
                second.contains("Haz X."),
                "{arm}: every arm brings the request"
            );
            let third = continue_prompt(arm, 3, "Haz X.", "Y.", &progress, "Claude Sonnet");
            assert_eq!(third.contains("Y."), arm == "doc", "{arm}");
        }
        let today = continue_prompt("today", 3, "Haz X.", "Y.", &progress, "Claude Sonnet");
        assert!(today.starts_with("## Pedido original\nHaz X."));
        assert!(today.contains("Claude Sonnet estaba trabajando"));
        assert!(today.contains("lib/a.py"));
        assert_eq!(
            continue_prompt("request", 3, "Haz X.", "Y.", &progress, "Claude Sonnet"),
            "Haz X.\n\nContinúa."
        );
    }

    #[test]
    fn a_budget_weighs_tokens_by_their_price() {
        let usage = UsageSample {
            input_tokens: 10,
            output_tokens: 100,
            cache_read_tokens: 1000,
            cache_write_tokens: 400,
            cost_usd: None,
        };
        assert_eq!(at_input_price(&usage), 10 + 500 + 100 + 500);
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
    fn adds_each_logged_message_once() {
        let tmp = tempfile::tempdir().unwrap();
        let line = |id: &str, out: u64| {
            format!(
                r#"{{"type":"assistant","message":{{"id":"{id}","usage":{{"input_tokens":2,"output_tokens":{out},"cache_read_input_tokens":100,"cache_creation_input_tokens":10}}}}}}"#
            )
        };
        let log = [
            r#"{"type":"user","message":{"content":"hola"}}"#.to_owned(),
            line("m1", 5),
            line("m1", 7),
            line("m2", 3),
            "not json".to_owned(),
        ]
        .join("\n");
        std::fs::write(tmp.path().join("s.jsonl"), log).unwrap();
        std::fs::write(tmp.path().join("notes.txt"), line("m3", 99)).unwrap();
        let usage = logged_usage(tmp.path());
        assert_eq!(usage.output_tokens, 10, "m1 counted once, as last logged");
        assert_eq!(usage.cache_read_tokens, 200);
        assert_eq!(usage.cost_usd, None);
        assert_eq!(logged_usage(&tmp.path().join("none")).total_tokens(), 0);
    }

    #[test]
    fn finds_where_a_folder_is_logged() {
        let logs = session_logs(
            Some(Path::new("/acc")),
            Path::new("/tmp/hedy-evals-1/gym-1/.hedy/worktrees/t1"),
        );
        assert_eq!(
            logs.unwrap(),
            Path::new("/acc/projects/-tmp-hedy-evals-1-gym-1--hedy-worktrees-t1")
        );
    }

    #[test]
    fn lists_the_files_a_patch_touches() {
        let patch = "diff --git a/a.py b/a.py\n+x\ndiff --git a/tests/t.py b/tests/t.py\n";
        assert_eq!(files_in(patch), ["a.py", "tests/t.py"]);
    }
}
