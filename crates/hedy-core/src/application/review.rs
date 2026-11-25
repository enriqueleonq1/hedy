//! Accepting a task's changes after a review, and undoing it.
//!
//! "Aceptar" first shows what will happen (target branch, files, lines, the
//! commit message, the last test run) and only merges when the user confirms.
//! Undo is deliberately narrow: Hedy moves the base branch back
//! (`git reset --keep`) only while the merge is still its last commit, the
//! checkout is on that branch and nothing was pushed; the task's branch and
//! worktree come back so the work is never lost. Anything else is the user's
//! call (for example `git revert`), and Hedy says why.

use super::error::UndoBlocked;
use super::metrics::TestRun;
use super::ports::MergeOutcome;
use super::ship::ShipError;
use super::{AppError, AppResult, Hedy};
use crate::domain::ship::{ShippedFile, conventional_subject, uses_conventional_commits};
use crate::domain::task::default_commit_message;
use crate::domain::{
    AcceptMode, AgentEvent, MergeRecord, Shipment, Task, TaskId, TaskStatus, Workspace,
};

/// What "Aceptar e integrar" is about to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptPreview {
    /// The task's branch.
    pub branch: String,
    /// Where it will be merged.
    pub target_branch: String,
    /// The branch the main checkout is on now; the merge needs the target.
    pub checkout_branch: Option<String>,
    pub files: usize,
    pub additions: u64,
    pub deletions: u64,
    /// The proposed commit message (the user may edit it).
    pub message: String,
    /// The task's latest test run; `None` when it never ran tests.
    pub tests: Option<TestRun>,
    /// What Hedy checked after the AI's last turn (docs/decisions.md §60).
    pub checks: Vec<CheckResult>,
    /// Why those checks could not run, when they could not.
    pub checks_skipped: Option<String>,
    /// What the clean-context review pointed at after that turn; `None`: no review.
    pub review: Option<Vec<crate::domain::ReviewWarning>>,
    /// The branch the task's copy is on when it is not its own: accepting
    /// waits until it goes back.
    pub branch_off: Option<String>,
}

/// One check Hedy ran once the AI was done.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    /// `tests` or `e2e`.
    pub check: String,
    pub command: String,
    pub passed: bool,
}

impl Hedy {
    pub(super) fn acceptable(&self, id: &TaskId) -> AppResult<Task> {
        let task = self.task(id)?;
        Self::workspace(&task)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        self.refuse_trial_run(id)?;
        if !task.mode.changes_files() {
            return Err(AppError::TaskWithoutChanges(id.clone()));
        }
        Ok(task)
    }

    /// The review before accepting: nothing changes.
    pub fn accept_preview(&self, id: &TaskId) -> AppResult<AcceptPreview> {
        let task = self.acceptable(id)?;
        let ws = Self::workspace(&task)?;
        let diff = self.git.diff(&ws.worktree, &self.task_base(ws))?;
        let timed = self.store.timed_events(id)?;
        let tests = timed.iter().rev().find_map(|(at, event)| match event {
            AgentEvent::TestsRun { command, passed } => Some(TestRun {
                command: command.clone(),
                passed: *passed,
                at: *at,
            }),
            _ => None,
        });
        // The checks run after the AI's last turn; a turn begun since (and
        // stopped) makes them stale.
        let finished = timed
            .iter()
            .rposition(|(_, e)| matches!(e, AgentEvent::Finished { .. }));
        let asked = timed
            .iter()
            .rposition(|(_, e)| matches!(e, AgentEvent::UserMessage { .. }));
        let last_turn = match (finished, asked) {
            (Some(f), Some(a)) if a > f => timed.len(),
            (Some(f), _) => f + 1,
            (None, _) => timed.len(),
        };
        let mut checks = Vec::new();
        let mut checks_skipped = None;
        let mut review = None;
        for (_, event) in &timed[last_turn..] {
            match event {
                AgentEvent::ReviewRan { warnings, .. } => review = Some(warnings.clone()),
                AgentEvent::CheckRan {
                    check,
                    command,
                    passed,
                    ..
                } => checks.push(CheckResult {
                    check: check.clone(),
                    command: command.clone(),
                    passed: *passed,
                }),
                AgentEvent::ChecksSkipped { reason, .. } => checks_skipped = Some(reason.clone()),
                _ => {}
            }
        }
        let message = self.proposed_message(&task, timed.iter().map(|(_, e)| e));
        Ok(AcceptPreview {
            branch: ws.branch.clone(),
            target_branch: ws.base_branch.clone(),
            checkout_branch: self.git.current_branch(&task.project).ok(),
            files: diff.len(),
            additions: diff.iter().map(|f| u64::from(f.additions)).sum(),
            deletions: diff.iter().map(|f| u64::from(f.deletions)).sum(),
            message,
            tests,
            checks,
            checks_skipped,
            review,
            branch_off: self.git.branch_off(&ws.worktree, &ws.branch).ok().flatten(),
        })
    }

    fn proposed_message<'a>(
        &self,
        task: &Task,
        events: impl DoubleEndedIterator<Item = &'a AgentEvent>,
    ) -> String {
        let summary = events.rev().find_map(|event| match event {
            AgentEvent::Finished {
                ok: true, summary, ..
            } => Some(summary.as_str()),
            _ => None,
        });
        // The repository's own style: Conventional Commits when its recent
        // subjects follow them.
        let recent = self
            .git
            .recent_commits(&task.project, 10)
            .unwrap_or_default();
        let title = if uses_conventional_commits(recent.iter().map(|c| c.subject.as_str())) {
            conventional_subject(&task.title)
        } else {
            task.title.clone()
        };
        default_commit_message(&title, summary)
    }

    /// "Aceptar e integrar": commits in the worktree with `message` (the
    /// proposed one when empty or `None`), merges into the base branch and
    /// remembers what the merge did, to undo it.
    pub fn accept_task_with(&self, id: &TaskId, message: Option<&str>) -> AppResult<Task> {
        self.accept_task_as(id, message, AcceptMode::Merge)
    }

    /// "Aceptar" in either mode: merged into the base branch, or committed
    /// and kept on the task's own branch (to publish it and open a PR from
    /// it, without touching the main folder).
    pub fn accept_task_as(
        &self,
        id: &TaskId,
        message: Option<&str>,
        mode: AcceptMode,
    ) -> AppResult<Task> {
        let mut task = self.acceptable(id)?;
        let ws = Self::workspace(&task)?.clone();
        // Committed on another branch, its work would never be merged.
        if let Some(on) = self.git.branch_off(&ws.worktree, &ws.branch)? {
            return Err(AppError::CopyOnAnotherBranch {
                branch: ws.branch.clone(),
                on,
            });
        }
        let message = match message.map(str::trim).filter(|m| !m.is_empty()) {
            Some(message) => message.to_owned(),
            None => {
                let events = self.store.events(id)?;
                self.proposed_message(&task, events.iter())
            }
        };
        self.stop_session(id);
        let files = self
            .git
            .diff(&ws.worktree, &self.task_base(&ws))?
            .iter()
            .map(ShippedFile::from)
            .collect();
        self.git.commit_all(&ws.worktree, &message)?;
        let tip = self.git.rev_parse(&ws.worktree, "HEAD")?;
        // A PR opened before a reopen stays the task's.
        let previous = task.ship.take().filter(|s| s.kept && s.branch == ws.branch);
        if mode == AcceptMode::Branch {
            self.source
                .remove_worktree_keep_branch(&task.project, &ws.worktree)?;
            self.forget_skills_plugin(&task.id);
            task.merge = None;
            task.ship = Some(Shipment {
                branch: ws.branch.clone(),
                kept: true,
                message,
                files,
                pushed_to: previous.as_ref().and_then(|p| p.pushed_to.clone()),
                pull_request: previous.and_then(|p| p.pull_request),
            });
            task.wait = None;
            self.store.save_task(&task)?;
            self.set_status(&mut task, TaskStatus::Accepted)?;
            self.clear_queue(id)?;
            return Ok(task);
        }
        // Only meaningful when the merge goes ahead (it checks the branch).
        let before = self.git.head_commit(&task.project).unwrap_or_default();
        match self.git.merge(&task.project, &ws.base_branch, &ws.branch)? {
            MergeOutcome::Merged => {}
            blocked => {
                self.set_status(&mut task, TaskStatus::Review)?;
                return Err(AppError::MergeBlocked {
                    branch: ws.branch.clone(),
                    reason: blocked,
                });
            }
        }
        let after = self.git.head_commit(&task.project)?;
        self.git
            .remove_worktree(&task.project, &ws.worktree, &ws.branch)?;
        self.forget_skills_plugin(&task.id);
        task.merge = Some(MergeRecord {
            before,
            after,
            tip,
            at: self.now_secs(),
        });
        task.ship = Some(Shipment {
            branch: ws.base_branch.clone(),
            kept: false,
            message,
            files,
            pushed_to: None,
            pull_request: None,
        });
        task.wait = None;
        self.store.save_task(&task)?;
        self.set_status(&mut task, TaskStatus::Accepted)?;
        self.clear_queue(id)?;
        Ok(task)
    }

    /// "Deshacer": takes the merge back out of the base branch and returns
    /// the task to review with its branch and worktree, only when that is
    /// safe (see the module docs); otherwise says why.
    pub fn undo_accept(&self, id: &TaskId) -> AppResult<Task> {
        let mut task = self.task(id)?;
        let blocked = |why| Err(AppError::UndoBlocked(why));
        if task.status != TaskStatus::Accepted {
            return blocked(UndoBlocked::NotAccepted);
        }
        if let (Some(ship), Some(ws)) = (task.ship.clone(), task.workspace.clone())
            && ship.kept
        {
            return self.reopen_kept(task, &ws);
        }
        let (Some(record), Some(ws)) = (task.merge.clone(), task.workspace.clone()) else {
            return blocked(UndoBlocked::NoRecord);
        };
        let repo = task.project.clone();
        let current = self.git.current_branch(&repo).unwrap_or_default();
        if current != ws.base_branch {
            return blocked(UndoBlocked::NotOnBase { current });
        }
        if self.git.head_commit(&repo)? != record.after {
            return blocked(UndoBlocked::MovedOn);
        }
        let moved = record.before != record.after;
        if moved {
            let remotes = self.git.remote_branches_containing(&repo, &record.after)?;
            if !remotes.is_empty() {
                return blocked(UndoBlocked::Published { remotes });
            }
        }
        // The task's branch and worktree first: its commits stay reachable
        // whatever happens next.
        self.git
            .add_worktree(&repo, &ws.worktree, &ws.branch, &record.tip)?;
        if moved && let Err(error) = self.git.reset_keep(&repo, &record.before) {
            // Back as it was: the merge stays, so the branch is not needed.
            let _ = self.git.remove_worktree(&repo, &ws.worktree, &ws.branch);
            return blocked(UndoBlocked::LocalChanges(error.message));
        }
        self.share_dependencies(&repo, &ws.worktree)?;
        self.install_skills_in(&repo, &ws.worktree, &task.id);
        task.merge = None;
        task.ship = None;
        self.store.save_task(&task)?;
        self.set_status(&mut task, TaskStatus::Review)?;
        Ok(task)
    }

    /// "Reabrir" a task kept on its branch: its worktree comes back on the
    /// same branch, so the agent can go on (a later push updates its PR).
    /// Nothing was merged, so nothing is moved back; the remote is untouched.
    fn reopen_kept(&self, mut task: Task, ws: &Workspace) -> AppResult<Task> {
        let repo = task.project.clone();
        if self.git.rev_parse(&repo, &ws.branch).is_err() {
            return Err(ShipError::BranchGone {
                branch: ws.branch.clone(),
            }
            .into());
        }
        self.source
            .attach_worktree(&repo, &ws.worktree, &ws.branch)?;
        self.share_dependencies(&repo, &ws.worktree)?;
        self.install_skills_in(&repo, &ws.worktree, &task.id);
        self.store.save_task(&task)?;
        self.set_status(&mut task, TaskStatus::Review)?;
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewTask;
    use crate::application::ports::TaskRepository;
    use crate::application::testing::Fixture;
    use crate::domain::diff::{FileDiff, FileStatus};
    use crate::domain::{AgentEvent, TaskMode};
    use crate::i18n::{Language, with_language};

    fn new_task(f: &Fixture) -> Task {
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/agenda"),
                prompt: "Recordatorios por WhatsApp".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
                permissions: None,
            })
            .unwrap()
    }

    fn file(path: &str, additions: u32, deletions: u32) -> FileDiff {
        FileDiff {
            path: path.into(),
            old_path: None,
            status: FileStatus::Modified,
            additions,
            deletions,
            binary: false,
            hunks: vec![],
        }
    }

    fn finish(f: &Fixture, summary: &str) {
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: summary.into(),
            duration_ms: None,
        });
    }

    #[test]
    fn the_preview_counts_files_and_lines_and_proposes_a_message() {
        let f = Fixture::new();
        let task = new_task(&f);
        f.git
            .set_diff(vec![file("app.py", 10, 2), file("test_app.py", 5, 0)]);
        f.agents.emit(AgentEvent::TestsRun {
            command: "pytest".into(),
            passed: false,
        });
        f.agents.emit(AgentEvent::TestsRun {
            command: "pytest -q".into(),
            passed: true,
        });
        finish(&f, "Listo: se programan 24 h antes.\n\nMás detalles.");

        let preview = f.hedy.accept_preview(&task.id).unwrap();
        assert_eq!(preview.target_branch, "main");
        assert_eq!(preview.checkout_branch.as_deref(), Some("main"));
        assert_eq!(
            (preview.files, preview.additions, preview.deletions),
            (2, 15, 2)
        );
        assert_eq!(
            preview.message,
            "Recordatorios por WhatsApp\n\nListo: se programan 24 h antes."
        );
        let tests = preview.tests.unwrap();
        assert_eq!((tests.command.as_str(), tests.passed), ("pytest -q", true));
        // Nothing happened.
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);
        assert!(f.git.log().is_empty());
    }

    #[test]
    fn a_task_that_never_ran_tests_says_so() {
        let f = Fixture::new();
        let task = new_task(&f);
        assert_eq!(f.hedy.accept_preview(&task.id).unwrap().tests, None);
    }

    #[test]
    fn accepting_uses_the_edited_message_and_remembers_the_merge() {
        let f = Fixture::new();
        let task = new_task(&f);
        let ws = task.workspace.clone().unwrap();
        let accepted = f
            .hedy
            .accept_task_with(&task.id, Some("  feat: recordatorios  "))
            .unwrap();
        assert_eq!(accepted.status, TaskStatus::Accepted);
        assert_eq!(
            f.git.log(),
            vec![
                format!("commit {} feat: recordatorios", ws.worktree.display()),
                format!("merge main <- {}", ws.branch),
                format!("remove {}", ws.branch),
            ]
        );
        let record = accepted.merge.clone().unwrap();
        assert_eq!(
            (
                record.before.as_str(),
                record.after.as_str(),
                record.tip.as_str()
            ),
            ("abc123", "merged", "tip")
        );
        assert!(record.at >= 1_700_000_000);
        assert_eq!(f.hedy.task(&task.id).unwrap().merge, accepted.merge);
    }

    #[test]
    fn an_empty_message_falls_back_to_the_proposed_one() {
        let f = Fixture::new();
        let task = new_task(&f);
        finish(&f, "");
        f.hedy.accept_task_with(&task.id, Some("   ")).unwrap();
        assert!(f.git.log()[0].ends_with(" Recordatorios por WhatsApp"));
    }

    #[test]
    fn undo_moves_the_branch_back_and_returns_the_task_to_review() {
        let f = Fixture::new();
        let task = new_task(&f);
        let ws = task.workspace.clone().unwrap();
        f.hedy.accept_task(&task.id).unwrap();

        let undone = f.hedy.undo_accept(&task.id).unwrap();
        assert_eq!(undone.status, TaskStatus::Review);
        assert_eq!(undone.merge, None);
        assert_eq!(f.git.head(), "abc123");
        assert!(f.git.log().contains(&"reset --keep abc123".to_owned()));
        // The branch comes back at the task's last commit.
        assert_eq!(
            f.git.worktrees().last().unwrap(),
            &format!("{} at tip", ws.branch)
        );
        // It can be accepted again.
        f.hedy.accept_task(&task.id).unwrap();
    }

    #[test]
    fn undo_refuses_when_anything_else_happened() {
        let f = Fixture::new();
        let task = new_task(&f);
        f.hedy.accept_task(&task.id).unwrap();
        let reason = |f: &Fixture| match f.hedy.undo_accept(&task.id) {
            Err(AppError::UndoBlocked(why)) => why,
            other => panic!("expected a refusal, got {other:?}"),
        };

        f.git.set_branch("otra");
        assert_eq!(
            reason(&f),
            UndoBlocked::NotOnBase {
                current: "otra".into()
            }
        );
        f.git.set_branch("main");

        f.git.set_remotes(&["origin/main"]);
        assert_eq!(
            reason(&f),
            UndoBlocked::Published {
                remotes: vec!["origin/main".into()]
            }
        );
        f.git.set_remotes(&[]);

        f.git
            .fail_reset("error: Entry 'app.py' not uptodate. Cannot merge.");
        assert!(matches!(reason(&f), UndoBlocked::LocalChanges(m) if m.contains("app.py")));
        // Nothing moved: still accepted, the worktree removed again.
        assert_eq!(f.git.head(), "merged");
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Accepted);
        assert!(f.git.log().last().unwrap().starts_with("remove "));

        f.git.set_head("someone-else");
        assert_eq!(reason(&f), UndoBlocked::MovedOn);
        assert!(!f.git.log().iter().any(|l| l.starts_with("reset")));

        let message = with_language(Language::English, || {
            AppError::UndoBlocked(UndoBlocked::MovedOn).to_string()
        });
        assert!(message.contains("new commits after the merge"), "{message}");
        assert!(message.contains("git revert"), "{message}");
    }

    #[test]
    fn only_accepted_tasks_with_a_record_can_be_undone() {
        let f = Fixture::new();
        let task = new_task(&f);
        assert_eq!(
            f.hedy.undo_accept(&task.id).unwrap_err(),
            AppError::UndoBlocked(UndoBlocked::NotAccepted)
        );
        let mut old = f.hedy.accept_task(&task.id).unwrap();
        old.merge = None;
        f.store.save_task(&old).unwrap();
        assert_eq!(
            f.hedy.undo_accept(&task.id).unwrap_err(),
            AppError::UndoBlocked(UndoBlocked::NoRecord)
        );
    }
}
