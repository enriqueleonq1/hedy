//! `--rescue`: does running the project's tests after the agent and sending a
//! failure back make the result better? (evals/experiments/rescue). Each run
//! is one first attempt, shared by four arms so they pair:
//!
//! - `base`: the first attempt, as Hedy works today;
//! - `rescue:same`: when the tests fail, their output goes back to the same
//!   session, once;
//! - `rescue:fresh`: the same, in a new session that starts from the first
//!   attempt's code;
//! - `rescue:retry` (the control, at the same spend): a second attempt from
//!   the start without feedback, delivered only if it passes the tests.
//!
//! When the tests pass after the first attempt, the four arms are that attempt.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use hedy_core::domain::TaskId;
use hedy_core::experiments;

use crate::runner::{
    Observed, RemoveOnDrop, Runner, changes_since, copy_dir, git, prepare_repo, run_command,
    save_diff,
};
use crate::{Case, Layer, RescueTrace, RunResult, Variant};

pub const SAME: &str = "rescue:same";
pub const FRESH: &str = "rescue:fresh";
pub const RETRY: &str = "rescue:retry";

/// What the same session gets when the tests fail.
pub fn same_session_message(command: &str, output: &str) -> String {
    format!(
        "Corrí los tests del proyecto (`{command}`) y fallan:\n\n```\n{output}\n```\n\nCorrígelo."
    )
}

/// What a new session gets: the request, then the failure.
pub fn fresh_session_prompt(prompt: &str, command: &str, output: &str) -> String {
    format!(
        "{prompt}\n\nYa hubo un intento y sus cambios están en el código. Corrí los tests del proyecto (`{command}`) y fallan:\n\n```\n{output}\n```\n\nCorrígelo."
    )
}

/// The project's own test files: every `test*.py` in the case's repo.
pub fn test_files(repo: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.starts_with("test")
                && name.ends_with(".py")
                && let Ok(relative) = path.strip_prefix(root)
            {
                out.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    let mut out = Vec::new();
    walk(repo, repo, &mut out);
    out.sort();
    out
}

/// Whether `patch` removes or changes a line of one of `tests` (or the whole
/// file). Lines only added are new tests and fine; so are changed imports and
/// blank lines, which a new test needs.
pub fn edits_tests(patch: &str, tests: &[String]) -> bool {
    let mut current = false;
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("diff --git a/") {
            let file = rest.split(" b/").next().unwrap_or("");
            current = tests.iter().any(|t| t == file);
            continue;
        }
        if !current {
            continue;
        }
        if line.starts_with("deleted file mode") || line.starts_with("rename from") {
            return true;
        }
        if line.starts_with("---") {
            continue;
        }
        if let Some(removed) = line.strip_prefix('-') {
            let removed = removed.trim();
            if !(removed.is_empty()
                || removed.starts_with("import ")
                || removed.starts_with("from "))
            {
                return true;
            }
        }
    }
    false
}

/// One attempt's end: what it spent and did, and how its code fares.
struct Attempt {
    seen: Observed,
    ms: u64,
    patch: String,
    /// The project's tests and the hidden check, each in its own copy.
    tests: (bool, String),
    hidden: (bool, String),
}

impl Attempt {
    fn failed(&self) -> bool {
        self.seen.error.is_some()
    }
}

/// Adds `attempt` to `arm` and makes it what the arm delivers.
fn deliver(arm: &mut RunResult, attempt: &Attempt) {
    attempt.seen.add_to(arm);
    arm.duration_ms += attempt.ms;
    arm.passed = !attempt.failed() && attempt.hidden.0;
    arm.check_output = attempt.hidden.1.clone();
}

/// Adds `attempt`'s spend to `arm` without delivering it.
fn spend(arm: &mut RunResult, attempt: &Attempt) {
    let error = arm.error.clone();
    attempt.seen.add_to(arm);
    arm.error = error;
    arm.duration_ms += attempt.ms;
}

impl Runner {
    /// The four arms of one run (see the module), in order: base, same,
    /// fresh, retry. Whatever goes wrong fails the arms it touches only.
    pub fn run_rescue(
        &self,
        case: &Case,
        layer: Layer,
        variant: &Variant,
        attempt: u32,
    ) -> Vec<RunResult> {
        let started_at = crate::runner::now();
        experiments::set(variant.experiments);
        let names = [
            variant.name.clone(),
            format!("{}+{SAME}", variant.name),
            format!("{}+{FRESH}", variant.name),
            format!("{}+{RETRY}", variant.name),
        ];
        let mut arms: Vec<RunResult> = names
            .iter()
            .map(|n| self.new_result(case, layer, n, attempt, started_at))
            .collect();
        let dir = self.options.work_dir.join(format!(
            "{}-{layer}-rescue-{attempt}-{started_at}",
            case.name
        ));
        let _dir = RemoveOnDrop(dir.clone());
        if let Err(e) = self.rescue_in(case, layer, &dir, &mut arms) {
            for arm in &mut arms {
                if arm.rescue.is_none() {
                    arm.passed = false;
                    arm.error.get_or_insert_with(|| format!("{e:#}"));
                }
            }
        }
        arms
    }

    fn rescue_in(
        &self,
        case: &Case,
        layer: Layer,
        dir: &Path,
        arms: &mut [RunResult],
    ) -> Result<()> {
        let command = case
            .spec
            .tests
            .as_deref()
            .with_context(|| format!("{} no dice cómo correr sus tests (tests)", case.name))?;
        let tests = test_files(&case.repo());
        let prompt = &case.spec.prompt;

        // The first attempt, whose session the same arm goes on with.
        let repo = prepare_repo(case, &dir.join("first"))?;
        let (first, same) = self.in_project(case, layer, &repo, |runner, base| {
            let clock = Instant::now();
            let task = runner.hedy.start_task(runner.new_task(&repo, prompt))?;
            let close = CloseOnDrop(runner, task.id.clone());
            let first = runner.attempt(case, &task.id, base, command, clock, false)?;
            if first.failed() || first.tests.0 {
                return Ok((first, None));
            }
            let clock = Instant::now();
            runner
                .hedy
                .send_message(&task.id, &same_session_message(command, &first.tests.1))?;
            let same = runner.attempt(case, &task.id, base, command, clock, true);
            drop(close);
            Ok((first, Some(same)))
        })?;
        for arm in arms.iter_mut() {
            deliver(arm, &first);
        }
        let trace = |arm: &RunResult, delivered: &Attempt, second: bool| RescueTrace {
            first_passed: arms_first_passed(&first),
            tests_failed: !first.failed() && !first.tests.0,
            tests_passed: delivered.tests.0,
            tests_edited: edits_tests(&delivered.patch, &tests),
            second: second && arm.error.is_none(),
        };
        let mut patches = vec![first.patch.clone(); 4];
        for (i, arm) in arms.iter_mut().enumerate() {
            arm.rescue = Some(trace(arm, &first, false));
            if first.failed() {
                patches[i].clear();
            }
        }
        if let Some(same) = same {
            match same {
                Ok(same) => {
                    deliver(&mut arms[1], &same);
                    arms[1].rescue = Some(trace(&arms[1], &same, true));
                    patches[1] = same.patch;
                }
                Err(e) => fail(&mut arms[1], e),
            }

            // A new session on the first attempt's code, told what failed.
            let fresh = (|| -> Result<Attempt> {
                let repo = prepare_repo(case, &dir.join("fresh"))?;
                let original = head(&repo)?;
                let patch = repo.join(".hedy-evals-first.patch");
                std::fs::write(&patch, &first.patch)?;
                git(&repo, &["apply", "--binary", &patch.to_string_lossy()])?;
                std::fs::remove_file(&patch)?;
                git(&repo, &["add", "-A"])?;
                git(&repo, &["commit", "-q", "-m", "Intento anterior"])?;
                let prompt = fresh_session_prompt(prompt, command, &first.tests.1);
                self.in_project(case, layer, &repo, |runner, _| {
                    let clock = Instant::now();
                    let task = runner.hedy.start_task(runner.new_task(&repo, &prompt))?;
                    let _close = CloseOnDrop(runner, task.id.clone());
                    runner.attempt(case, &task.id, &original, command, clock, false)
                })
            })();
            match fresh {
                Ok(fresh) => {
                    deliver(&mut arms[2], &fresh);
                    arms[2].rescue = Some(trace(&arms[2], &fresh, true));
                    patches[2] = fresh.patch;
                }
                Err(e) => fail(&mut arms[2], e),
            }

            // The control: start over, keep the second only if its tests pass.
            let retry = (|| -> Result<Attempt> {
                let repo = prepare_repo(case, &dir.join("retry"))?;
                self.in_project(case, layer, &repo, |runner, base| {
                    let clock = Instant::now();
                    let task = runner.hedy.start_task(runner.new_task(&repo, prompt))?;
                    let _close = CloseOnDrop(runner, task.id.clone());
                    runner.attempt(case, &task.id, base, command, clock, false)
                })
            })();
            match retry {
                Ok(retry) if !retry.failed() && retry.tests.0 => {
                    deliver(&mut arms[3], &retry);
                    arms[3].rescue = Some(trace(&arms[3], &retry, true));
                    patches[3] = retry.patch;
                }
                Ok(retry) => spend(&mut arms[3], &retry),
                Err(e) => fail(&mut arms[3], e),
            }
        }
        if let Some(diffs) = &self.options.diffs {
            for (arm, patch) in arms.iter_mut().zip(&patches) {
                arm.diff = Some(save_diff(diffs, arm, patch)?);
            }
        }
        Ok(())
    }

    /// Runs `work` on `repo` as a project of this run (`base` is its HEAD),
    /// and forgets the project after.
    fn in_project<T>(
        &self,
        case: &Case,
        layer: Layer,
        repo: &Path,
        work: impl FnOnce(&Self, &str) -> Result<T>,
    ) -> Result<T> {
        let result = (|| {
            self.prepare_project(case, layer, repo)?;
            let base = head(repo)?;
            work(self, &base)
        })();
        self.forget_project(repo);
        result
    }

    /// Waits for the agent's turn, then takes its code and runs the project's
    /// tests and the hidden check on it, each in a copy: the worktree stays
    /// as the agent left it, for a turn more.
    fn attempt(
        &self,
        case: &Case,
        id: &TaskId,
        base: &str,
        command: &str,
        clock: Instant,
        resumed: bool,
    ) -> Result<Attempt> {
        let seen = self.drive(id, Duration::from_secs(case.spec.timeout_secs), resumed);
        let ms = clock.elapsed().as_millis() as u64;
        let work = self.hedy.task(id)?.work_dir();
        let patch = changes_since(&work, base)?;
        let (tests, hidden) = match seen.error {
            Some(_) => ((false, String::new()), (false, String::new())),
            None => (
                self.in_copy(&work, None, command)?,
                self.in_copy(&work, Some(&case.checks()), &case.spec.check)?,
            ),
        };
        Ok(Attempt {
            seen,
            ms,
            patch,
            tests,
            hidden,
        })
    }

    /// Runs `command` on a copy of `work` (with `checks` added, if any).
    fn in_copy(&self, work: &Path, checks: Option<&Path>, command: &str) -> Result<(bool, String)> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let copy = self.options.work_dir.join(format!("copy-{stamp}"));
        let _copy = RemoveOnDrop(copy.clone());
        copy_dir(work, &copy)?;
        if let Some(checks) = checks.filter(|c| c.is_dir()) {
            copy_dir(checks, &copy)?;
        }
        run_command(&copy, command)
    }
}

fn arms_first_passed(first: &Attempt) -> bool {
    !first.failed() && first.hidden.0
}

fn fail(arm: &mut RunResult, e: anyhow::Error) {
    arm.passed = false;
    arm.error = Some(format!("{e:#}"));
}

fn head(repo: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .context("no se pudo ejecutar git")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Discards a task however its attempt ends.
struct CloseOnDrop<'a>(&'a Runner, TaskId);

impl Drop for CloseOnDrop<'_> {
    fn drop(&mut self) {
        self.0.close_task(&self.1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TESTS: &[&str] = &["tests/test_a.py"];

    fn files() -> Vec<String> {
        TESTS.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn adding_tests_and_imports_is_fine() {
        let patch = "diff --git a/tests/test_a.py b/tests/test_a.py\n--- a/tests/test_a.py\n+++ b/tests/test_a.py\n@@ -1,3 +1,6 @@\n-from app import a\n+from app import a, b\n \n+    def test_b(self):\n+        self.assertTrue(b())\n";
        assert!(!edits_tests(patch, &files()));
    }

    #[test]
    fn changing_or_removing_a_test_is_not() {
        let changed = "diff --git a/tests/test_a.py b/tests/test_a.py\n--- a/tests/test_a.py\n+++ b/tests/test_a.py\n@@ -4 +4 @@\n-        self.assertEqual(a(), 1)\n+        self.assertEqual(a(), 2)\n";
        assert!(edits_tests(changed, &files()));
        let deleted = "diff --git a/tests/test_a.py b/tests/test_a.py\ndeleted file mode 100644\n";
        assert!(edits_tests(deleted, &files()));
        // The same lines in another file are the agent's code.
        let other = changed.replace("tests/test_a.py", "app/a.py");
        assert!(!edits_tests(&other, &files()));
    }

    #[test]
    fn finds_the_projects_test_files() {
        let tmp = tempfile::tempdir().unwrap();
        for path in [
            "tests/test_a.py",
            "tests/__init__.py",
            "app/a.py",
            "test_root.py",
        ] {
            let path = tmp.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        assert_eq!(test_files(tmp.path()), ["test_root.py", "tests/test_a.py"]);
    }

    #[test]
    fn the_messages_carry_the_command_and_the_failure() {
        let same = same_session_message("python3 -m unittest", "FAILED (failures=1)");
        assert!(same.contains("`python3 -m unittest`") && same.contains("FAILED (failures=1)"));
        let fresh = fresh_session_prompt("Arregla X.", "python3 -m unittest", "FAILED");
        assert!(fresh.starts_with("Arregla X.\n\nYa hubo un intento"));
        assert!(fresh.contains("FAILED"));
    }
}
