//! The checks of a task's way of working (docs/decisions.md §60): once the AI
//! is done with a turn that changes code, Hedy runs the project's tests and
//! the user's end-to-end check in the task's copy, isolated, at no cost in
//! tokens. A first failure goes back to the same session; after that, the
//! task is ready for review with what failed.

use crate::domain::{AgentEvent, Task, TaskMode, TaskStatus};
use crate::tr;

use super::{AppResult, Hedy};

/// Characters of a check's output kept in its event.
const OUTPUT_KEPT: usize = 4_000;

/// How long the clean-context review may take.
const REVIEW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(240);

impl Hedy {
    /// Runs the task's checks. `true`: the turn's end is not this one's to
    /// finish (a failure went back to its agent, or the task was stopped or
    /// closed meanwhile).
    pub(super) fn run_task_checks(&self, task: &mut Task) -> AppResult<bool> {
        let Some(working) = task.working.clone() else {
            return Ok(false);
        };
        let Some(ws) = task.workspace.clone() else {
            return Ok(false);
        };
        if task.mode != TaskMode::Project || task.flow.is_some() {
            return Ok(false);
        }
        let mut checks: Vec<(&str, String)> = Vec::new();
        // Tests Hedy cannot find are skipped quietly: the project's settings
        // say none was found, and a command can be set there.
        if working.tests
            && let Some(command) = working.tests_command.clone().or_else(|| {
                let files = self.git.list_files(&ws.worktree).ok()?;
                crate::domain::briefing::test_command(&files)
            })
        {
            checks.push(("tests", command));
        }
        if working.e2e
            && let Some(command) = working.e2e_command.clone()
        {
            checks.push(("e2e", command));
        }
        if checks.is_empty() {
            return Ok(false);
        }
        // They run code an AI wrote: only where they run isolated.
        if !self.runner.isolates_checks() {
            let reason = tr!(
                "Hedy no corre las comprobaciones sin aislarlas: instala bubblewrap (bwrap), o córrelas tú en la copia.",
                "Hedy does not run the checks without isolating them: install bubblewrap (bwrap), or run them yourself in the copy."
            );
            self.record(&task.id, AgentEvent::ChecksSkipped { reason })?;
            return Ok(false);
        }
        let retried = self.checks_failed_since_user(task)?;
        let reads: Vec<_> = std::iter::once(".git")
            .chain(crate::domain::task::SHARED_DEPENDENCY_DIRS.iter().copied())
            .map(|name| task.project.join(name))
            .filter(|path| self.fs.exists(path))
            .collect();
        let mut failed = Vec::new();
        for (check, command) in checks {
            let spec = crate::domain::LaunchSpec {
                program: "sh".into(),
                // Hedy's check leaves the copy as the AI left it: no Python
                // bytecode in the change (other tools' caches are ignored files).
                args: vec![
                    "-c".into(),
                    format!("export PYTHONDONTWRITEBYTECODE=1; {command}"),
                ],
                env: Vec::new(),
                cwd: Some(ws.worktree.clone()),
            };
            let ran = self.runner.run_check(
                &spec,
                super::flow_runs::FLOW_TESTS_TIMEOUT,
                task.id.as_str(),
                &reads,
            );
            // Stopped, discarded or accepted meanwhile: nothing more.
            if self.stopped_meanwhile_checks(task)? {
                return Ok(true);
            }
            let out = match ran {
                Ok(out) => out,
                // A command that does not start or end is the user's to fix,
                // not the AI's.
                Err(e) => {
                    let reason = tr!(
                        "`{command}` no se pudo correr: {e}",
                        "`{command}` could not run: {e}"
                    );
                    self.record(&task.id, AgentEvent::ChecksSkipped { reason })?;
                    continue;
                }
            };
            let passed = out.exit_code == Some(0);
            let output = tail(&out.stdout);
            self.record(
                &task.id,
                AgentEvent::CheckRan {
                    check: check.to_owned(),
                    command: command.clone(),
                    passed,
                    output: output.clone(),
                },
            )?;
            if !passed {
                failed.push(format!("`{command}`\n{output}"));
            }
        }
        // A read-only last step (an AI review) cannot fix anything.
        let writes = task
            .stages
            .get(task.stage)
            .is_some_and(|s| !s.phase.read_only_in(task.mode));
        if failed.is_empty() || retried || !writes {
            return Ok(false);
        }
        let prompt = format!(
            "{}\n\n{}",
            tr!(
                "Hedy corrió las comprobaciones del proyecto y fallaron. Arréglalo y vuelve a correrlas antes de terminar:",
                "Hedy ran the project's checks and they failed. Fix it and run them again before you finish:"
            ),
            failed.join("\n\n")
        );
        self.push_to_session(task, &prompt, Vec::new())?;
        Ok(true)
    }

    /// The review with a clean context (docs/decisions.md §57 B, §60), when
    /// the task's way of working has it: another AI reads only the change
    /// and points at visible mistakes. It never blocks accepting, and runs on
    /// the task's own account (rule 3), with Sonnet on a Claude account.
    /// `true`: the task was stopped or closed meanwhile.
    pub(super) fn run_task_review(&self, task: &Task) -> AppResult<bool> {
        let Some(working) = &task.working else {
            return Ok(false);
        };
        let Some(ws) = &task.workspace else {
            return Ok(false);
        };
        if !working.review || task.mode != TaskMode::Project || task.flow.is_some() {
            return Ok(false);
        }
        if self.stopped_meanwhile_checks(task)? {
            return Ok(true);
        }
        let diff = self.git.diff(&ws.worktree, &ws.base_commit)?;
        let change = crate::domain::rule_check::change_for_prompt(&diff);
        if change.added.is_empty() {
            return Ok(false);
        }
        let account = self.account(task.account.as_str())?;
        let model =
            (account.provider == crate::domain::Provider::Claude).then(|| "sonnet".to_owned());
        let skipped = |reason: String| self.record(&task.id, AgentEvent::ChecksSkipped { reason });
        if self.account_at_limit(&account)? {
            skipped(tr!(
                "La revisión no se hizo: la cuenta llegó a su límite.",
                "The review did not run: the account reached its limit."
            ))?;
            return Ok(false);
        }
        let stage = crate::domain::Stage {
            phase: crate::domain::Phase::Review,
            account: account.id.clone(),
            model: model.clone(),
        };
        let answer = self.one_shot(super::one_shot::OneShot {
            task: Some(&task.id),
            account: &account,
            stage: &stage,
            cwd: self.ask_dir()?,
            prompt: crate::domain::prompts::review_prompt(&task.prompt, &change.text),
            refusal: tr!(
                "No uses herramientas: revisa solo el cambio que te di.",
                "Do not use tools: review only the change you were given."
            ),
            no_shell: true,
            no_tools: true,
            timeout: REVIEW_TIMEOUT,
        })?;
        if self.stopped_meanwhile_checks(task)? {
            return Ok(true);
        }
        match answer {
            Ok(raw) => self.record(
                &task.id,
                AgentEvent::ReviewRan {
                    warnings: crate::domain::agent::parse_review(&raw),
                    model,
                },
            )?,
            Err(_) => skipped(tr!(
                "La revisión no dio una respuesta: revisa tú el cambio.",
                "The review gave no answer: review the change yourself."
            ))?,
        }
        Ok(false)
    }

    /// Whether the task stopped running while Hedy checked it (the user
    /// stopped, discarded or accepted it), as a flow's checks tell.
    fn stopped_meanwhile_checks(&self, task: &Task) -> AppResult<bool> {
        Ok(self.task(&task.id)?.status != TaskStatus::Running)
    }

    /// Whether a check already failed since the user last wrote: then the
    /// failure is not sent back again.
    fn checks_failed_since_user(&self, task: &Task) -> AppResult<bool> {
        let events = self.store.events(&task.id)?;
        Ok(events
            .iter()
            .rev()
            .take_while(|e| !matches!(e, AgentEvent::UserMessage { .. }))
            .any(|e| matches!(e, AgentEvent::CheckRan { passed: false, .. })))
    }
}

/// The end of a check's output, at most [`OUTPUT_KEPT`] characters.
fn tail(output: &str) -> String {
    let chars: Vec<char> = output.trim_end().chars().collect();
    chars[chars.len().saturating_sub(OUTPUT_KEPT)..]
        .iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::application::NewTask;
    use crate::application::testing::{AgentCall, Fixture};
    use crate::domain::{AgentEvent, Preset, Task, TaskMode, TaskStatus, WorkingStyle};

    fn start(f: &Fixture, working: WorkingStyle) -> Task {
        f.add_claude("Personal");
        let dir = f.dir("/code/agenda");
        f.hedy.use_account(&dir, "personal", None).unwrap();
        f.hedy.set_project_working(&dir, working).unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: dir,
                prompt: "Arregla el total".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
                permissions: None,
            })
            .unwrap()
    }

    fn finish(f: &Fixture) {
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: None,
        });
    }

    fn checks(f: &Fixture, task: &Task) -> Vec<(String, bool)> {
        f.hedy
            .task_events(&task.id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e {
                AgentEvent::CheckRan { check, passed, .. } => Some((check, passed)),
                _ => None,
            })
            .collect()
    }

    fn sent(f: &Fixture) -> Vec<String> {
        f.agents
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                AgentCall::Send(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn after_ok() -> WorkingStyle {
        Preset::AfterOk.apply(&WorkingStyle {
            tests_command: Some("pytest -q".into()),
            e2e_command: Some("npm run e2e".into()),
            ..Default::default()
        })
    }

    #[test]
    fn checks_that_pass_leave_the_task_ready() {
        let f = Fixture::new();
        let task = start(&f, after_ok());
        f.runner.reply("2 passed", 0);
        finish(&f);
        assert_eq!(
            checks(&f, &task),
            vec![("tests".into(), true), ("e2e".into(), true)]
        );
        assert_eq!(f.runner.checks_run(), 2);
        assert!(sent(&f).is_empty());
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
        // Cambios says what Hedy checked.
        let preview = f.hedy.accept_preview(&task.id).unwrap();
        let checked: Vec<_> = preview
            .checks
            .iter()
            .map(|c| (c.check.as_str(), c.passed))
            .collect();
        assert_eq!(checked, [("tests", true), ("e2e", true)]);
        assert_eq!(preview.checks_skipped, None);
        // A turn begun since makes them stale.
        f.hedy
            .send_message(&task.id, "Cambia también el redondeo")
            .unwrap();
        assert!(f.hedy.accept_preview(&task.id).unwrap().checks.is_empty());
    }

    #[test]
    fn a_failure_goes_back_to_the_same_session_once() {
        let f = Fixture::new();
        let task = start(&f, after_ok());
        f.runner.reply("1 failed: test_total", 1);
        finish(&f);
        let sent = sent(&f);
        assert_eq!(sent.len(), 1, "one message back: {sent:?}");
        assert!(sent[0].contains("pytest -q") && sent[0].contains("test_total"));
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Running);
        // It fails again: ready for review with what failed, no second round.
        finish(&f);
        assert_eq!(self::sent(&f).len(), 1);
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
        assert_eq!(checks(&f, &task).len(), 4);
        // Only the last turn's checks count.
        assert_eq!(f.hedy.accept_preview(&task.id).unwrap().checks.len(), 2);
    }

    #[test]
    fn without_isolation_or_stops_nothing_runs() {
        let f = Fixture::new();
        let task = start(&f, after_ok());
        f.runner.without_isolation();
        finish(&f);
        assert_eq!(f.runner.checks_run(), 0);
        let events = f.hedy.task_events(&task.id).unwrap();
        assert!(events.iter().any(
            |e| matches!(e, AgentEvent::ChecksSkipped { reason } if reason.contains("bwrap"))
        ));
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);

        let f = Fixture::new();
        let task = start(&f, WorkingStyle::default());
        finish(&f);
        assert_eq!(f.runner.checks_run(), 0);
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
    }

    #[test]
    fn a_check_that_does_not_start_is_said_not_sent_to_the_ai() {
        let f = Fixture::new();
        let task = start(&f, after_ok());
        finish(&f);
        assert!(sent(&f).is_empty());
        let skipped = f
            .hedy
            .task_events(&task.id)
            .unwrap()
            .into_iter()
            .filter(|e| matches!(e, AgentEvent::ChecksSkipped { .. }))
            .count();
        assert_eq!(skipped, 2);
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
    }

    fn a_change() -> Vec<crate::domain::FileDiff> {
        use crate::domain::diff::{DiffLine, FileStatus, Hunk, LineKind};
        vec![crate::domain::FileDiff {
            path: "shop/cart.py".into(),
            old_path: None,
            status: FileStatus::Modified,
            additions: 1,
            deletions: 0,
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -3 +3,1 @@".into(),
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_line: None,
                    new_line: Some(3),
                    text: "total = round(subtotal) + tax".into(),
                }],
            }],
        }]
    }

    #[test]
    fn a_review_with_a_clean_context_reads_only_the_change_and_never_blocks() {
        let f = Fixture::new();
        let style = Preset::AfterOkReview.apply(&WorkingStyle::default());
        let task = start(
            &f,
            WorkingStyle {
                tests: false,
                ..style
            },
        );
        f.git.set_diff(a_change());
        f.agents.reply_on_start(vec![AgentEvent::Finished {
            ok: true,
            summary: "shop/cart.py:3: redondea antes de sumar el impuesto".into(),
            duration_ms: None,
        }]);
        finish(&f);
        let review = f.agents.last_request();
        assert!(review.no_tools && review.prompt.contains("total = round(subtotal) + tax"));
        assert_eq!(review.model.as_deref(), Some("sonnet"));
        let warnings: Vec<_> = f
            .hedy
            .task_events(&task.id)
            .unwrap()
            .into_iter()
            .filter_map(|e| match e {
                AgentEvent::ReviewRan { warnings, .. } => Some(warnings),
                _ => None,
            })
            .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0][0].line, Some(3));
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
    }

    #[test]
    fn a_failure_is_not_sent_to_a_read_only_last_step() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let dir = f.dir("/code/agenda");
        f.hedy.use_account(&dir, "personal", None).unwrap();
        // An AI review as the last step: it reads, it cannot fix.
        let mut phases = crate::domain::PhasePlan::new();
        phases.insert(
            crate::domain::Phase::Review,
            crate::domain::Assignment {
                account: crate::domain::AccountId::new("personal").unwrap(),
                model: None,
            },
        );
        f.hedy.set_project_phases(&dir, phases).unwrap();
        f.hedy.set_project_working(&dir, after_ok()).unwrap();
        let task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: dir,
                prompt: "Arregla el total".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
                permissions: None,
            })
            .unwrap();
        f.runner.reply("1 failed", 1);
        finish(&f); // implement: the review step comes next, no checks yet
        assert_eq!(f.runner.checks_run(), 0);
        finish(&f); // the review step ends: checks, failing, not sent back
        assert_eq!(f.runner.checks_run(), 2);
        assert!(sent(&f).is_empty());
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
    }

    #[test]
    fn a_review_that_cannot_start_leaves_the_task_ready() {
        let f = Fixture::new();
        let style = Preset::AfterOkReview.apply(&WorkingStyle::default());
        let task = start(
            &f,
            WorkingStyle {
                tests: false,
                ..style
            },
        );
        f.git.set_diff(a_change());
        f.agents.fail_next_start();
        finish(&f);
        let events = f.hedy.task_events(&task.id).unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::ChecksSkipped { .. }))
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, AgentEvent::ReviewRan { .. }))
        );
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
    }

    #[test]
    fn stopping_the_task_while_it_is_checked_ends_it_all() {
        let f = Fixture::new();
        let style = Preset::AfterOkReview.apply(&after_ok());
        let task = start(&f, style);
        f.git.set_diff(a_change());
        f.runner.reply("1 failed", 1);
        let (hedy, id) = (f.hedy.clone(), task.id.clone());
        f.runner.during_check(move || {
            let _ = hedy.stop_task(&id);
        });
        let started = f.agents.requests().len();
        finish(&f);
        assert_eq!(
            f.runner.checks_run(),
            1,
            "the e2e does not run after a stop"
        );
        assert!(sent(&f).is_empty(), "nothing goes back to the AI");
        assert_eq!(f.agents.requests().len(), started, "no review is spent");
        assert!(checks(&f, &task).is_empty(), "a killed check is no result");
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
    }

    #[test]
    fn a_message_written_while_it_is_checked_goes_after_the_checks() {
        let f = Fixture::new();
        let task = start(&f, after_ok());
        f.runner.reply("ok", 0);
        let (hedy, id) = (f.hedy.clone(), task.id.clone());
        f.runner.during_check(move || {
            let _ = hedy.queue_message(&id, "Y agrega un test del total");
        });
        finish(&f);
        let sent = sent(&f);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(sent[0].contains("Y agrega un test del total"));
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Running);
    }
}
