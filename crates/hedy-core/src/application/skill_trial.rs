//! Trying a skill on real tasks (level 3, docs/decisions.md): each task runs
//! twice as an ordinary task, each in its own worktree, one with the skill
//! and one without (its plugin, its worktree files and the memory tools
//! leave it out). Once a run stops, Hedy runs the same check in its copy
//! (the project's test command, or the user's). When every run has, they
//! are compared and discarded: nothing reaches the project. The result
//! stays with the skill's version on this PC.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use super::ask::AskScope;
use super::one_shot::{OneShot, OneShotFailure};
use super::ports::PathKind;
use super::{AppError, AppResult, Hedy, NewTask};
use crate::domain::Workspace;
use crate::domain::briefing;
use crate::domain::code::ChangeKind;
use crate::domain::launch::LaunchSpec;
use crate::domain::skill::{SkillFolder, check_visible};
use crate::domain::skill_trial::{
    TRIAL_TASKS_MAX, TrialOutcome, TrialRef, TrialResult, configures_the_check, fingerprint,
    guards_the_check, judge, parse_trial_tasks, restore_scripts, restore_test_tables,
    run_from_events, trial_tasks_prompt, void_reason,
};
use crate::domain::{
    Phase, Provider, Skill, SkillStatus, Stage, Task, TaskId, TaskMode, TaskStatus,
};
use crate::tr;

/// Settings key of a run's trial (by task id).
const RUN_KEY: &str = "skill_trial.run.";
/// Settings key of a skill's last result (by skill id).
const RESULT_KEY: &str = "skill_trial.result.";
/// Settings key of how a trial ended, until its driver reads it.
const OUTCOME_KEY: &str = "skill_trial.outcome.";
/// Longest a check may take in one copy.
const CHECK_TIMEOUT: Duration = Duration::from_secs(600);
/// Largest test file Hedy puts back before a check.
const RESTORE_MAX: u64 = 2 * 1024 * 1024;

/// Two runs that stop at once judge their trial once.
static JUDGING: Mutex<()> = Mutex::new(());

/// A trial that started: its id, its runs (with, without, with, …) and the
/// check Hedy runs in each copy.
#[derive(Debug, Clone)]
pub struct StartedTrial {
    pub id: String,
    pub tasks: Vec<Task>,
    pub check: Option<String>,
}

impl Hedy {
    /// Up to three small tasks of the project where an agent would need the
    /// skill, proposed by the project's account (never another: rule 3; a
    /// light model by default) from the skill alone, with no tools.
    pub fn propose_trial_tasks(
        &self,
        project: &Path,
        skill_id: i64,
        account: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<Vec<String>> {
        let project = self.project_at(project)?;
        // The runs need the project's account: nothing is spent without one.
        project
            .account
            .as_ref()
            .ok_or(AppError::NoAccountResolved)?;
        let skill = self.trial_skill(&project.path, skill_id)?;
        let scope = AskScope::Project(project.path.clone());
        let (account, _) = self.ask_account(&scope, account)?;
        if self.account_at_limit(&account)? {
            return Err(AppError::AskAtLimit {
                account: account.name.clone(),
            });
        }
        let stage = Stage {
            phase: Phase::Review,
            account: account.id.clone(),
            model: model
                .map(str::to_owned)
                .or_else(|| (account.provider == Provider::Claude).then(|| "haiku".into())),
        };
        let answer = self.one_shot(OneShot {
            task: None,
            account: &account,
            stage: &stage,
            cwd: self.ask_dir()?,
            prompt: trial_tasks_prompt(&skill.name, &skill.description, &skill.body),
            refusal: tr!(
                "No uses herramientas: propón solo las tareas.",
                "Do not use tools: only propose the tasks."
            ),
            no_shell: true,
            no_tools: true,
            timeout: std::time::Duration::from_secs(120),
        })?;
        match answer {
            Ok(raw) => Ok(parse_trial_tasks(&raw)),
            Err(OneShotFailure::AtLimit) => Err(AppError::AskAtLimit {
                account: account.name.clone(),
            }),
            Err(OneShotFailure::Failed(detail)) => Err(AppError::SkillReviewFailed(detail)),
            Err(OneShotFailure::TimedOut) => Err(AppError::SkillReviewTimedOut),
        }
    }

    /// Whether Hedy's checks run isolated on this machine (no network, the
    /// system read-only except the copy).
    pub fn trial_checks_isolated(&self) -> bool {
        self.runner.isolates_checks()
    }

    /// The check Hedy would run in each copy: the project's test command,
    /// guessed from its files.
    pub fn trial_check(&self, project: &Path) -> AppResult<Option<String>> {
        let project = self.project_at(project)?;
        let files = self
            .code
            .files_at(&project.path, "HEAD")
            .unwrap_or_default();
        Ok(briefing::test_command(&files))
    }

    /// Starts a trial of an approved skill: each of `tasks` (1 to 3) as two
    /// project tasks, one with the skill and one without, through the
    /// project's own phases and accounts (`model` for all, if given). They
    /// are ordinary tasks: they show on the board and ask for permissions
    /// like any other. `check` (the driver offers [`Hedy::trial_check`]; the
    /// user confirms it, since it runs code an AI wrote) runs in each copy
    /// once its run stops; `None`: no check. If one cannot start, the whole
    /// trial goes.
    pub fn start_skill_trial(
        &self,
        project: &Path,
        skill_id: i64,
        tasks: &[String],
        model: Option<&str>,
        check: Option<&str>,
    ) -> AppResult<StartedTrial> {
        let project = self.project_at(project)?;
        let skill = self.trial_skill(&project.path, skill_id)?;
        let prompts: Vec<&str> = tasks
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .collect();
        if prompts.is_empty() || prompts.len() > TRIAL_TASKS_MAX {
            return Err(AppError::TrialTasks);
        }
        // A copy the user's CLIs read in the repo would reach the run
        // without it too.
        for folder in SkillFolder::ALL {
            let path = project.path.join(folder.file(&skill.name));
            if self.fs.exists(&path) {
                return Err(AppError::TrialSkillOutside(path));
            }
        }
        let check = check
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        let trial = format!("k{:x}", self.clock.now_ms());
        let version = fingerprint(&skill.name, &skill.description, &skill.body);
        let mut started = Vec::new();
        for (index, prompt) in prompts.iter().enumerate() {
            for with_skill in [true, false] {
                let run = TrialRef {
                    trial: trial.clone(),
                    skill: skill.name.clone(),
                    skill_id,
                    with_skill,
                    index,
                    tasks: prompts.len(),
                    check: check.clone(),
                    checked: None,
                    restored: 0,
                    stopped: false,
                    version: version.clone(),
                };
                let request = NewTask {
                    images: Vec::new(),
                    project: project.path.clone(),
                    prompt: (*prompt).to_owned(),
                    mode: TaskMode::Project,
                    model: model.map(str::to_owned),
                    plan_first: false,
                };
                match self.start_task_for_trial(request, run) {
                    Ok(task) => started.push(task),
                    Err(error) => {
                        self.cancel_skill_trial(&project.path, &trial);
                        return Err(error);
                    }
                }
            }
        }
        Ok(StartedTrial {
            id: trial,
            tasks: started,
            check,
        })
    }

    /// Stops a trial before it is judged: every run carrying its id (also
    /// one that failed to start) is discarded and no result is kept (half a
    /// trial would say the wrong thing).
    pub fn cancel_skill_trial(&self, project: &Path, trial: &str) {
        let _judging = JUDGING.lock().unwrap_or_else(|e| e.into_inner());
        let runs = self.trial_runs(project, trial);
        for (task, _) in &runs {
            self.forget_trial_run(&task.id);
        }
        // A check still running in a copy about to go stops first.
        self.runner.stop_checks(trial);
        for (task, _) in &runs {
            self.drop_trial_proposals(&task.project, &task.id);
            if !task.status.is_closed() {
                let _ = self.discard_task(&task.id);
            }
        }
    }

    /// The trial was judged or voided and its runs are gone: its outcome,
    /// written last, is kept. (A cancel is its driver's own doing.)
    pub fn trial_settled(&self, trial: &str) -> bool {
        let key = format!("{OUTCOME_KEY}{trial}");
        matches!(self.store.get_setting(&key), Ok(Some(_)))
    }

    /// How a trial ended, read once (its driver shows it).
    pub fn take_trial_outcome(&self, trial: &str) -> AppResult<Option<TrialOutcome>> {
        let key = format!("{OUTCOME_KEY}{trial}");
        let saved = self.store.get_setting(&key)?;
        if saved.is_some() {
            self.store.set_setting(&key, None)?;
        }
        Ok(saved.and_then(|json| serde_json::from_str(&json).ok()))
    }

    /// The last trial's result for a skill, on this PC, while the skill is
    /// the one tried and its text has not changed since.
    pub fn skill_trial(&self, skill_id: i64) -> AppResult<Option<TrialResult>> {
        let saved = self.store.get_setting(&format!("{RESULT_KEY}{skill_id}"))?;
        let Some(result) = saved.and_then(|json| serde_json::from_str::<TrialResult>(&json).ok())
        else {
            return Ok(None);
        };
        let current = self.store.skill(skill_id)?;
        let same = current.is_some_and(|s| {
            s.name == result.skill
                && fingerprint(&s.name, &s.description, &s.body) == result.version
        });
        Ok(same.then_some(result))
    }

    /// A removed skill's result goes with it (ids are reused).
    pub(super) fn forget_skill_trial(&self, skill_id: i64) {
        let _ = self
            .store
            .set_setting(&format!("{RESULT_KEY}{skill_id}"), None);
    }

    /// The approved skill a trial is about, seen from `project`, whose text
    /// may go to an agent (nothing hidden in it).
    fn trial_skill(&self, project: &Path, skill_id: i64) -> AppResult<Skill> {
        let skill = self.skill_or_error(skill_id)?;
        let seen = self.agent_skills(project)?.iter().any(|s| s.id == skill.id);
        if skill.status != SkillStatus::Approved || !seen {
            return Err(AppError::SkillNotFound(skill_id.to_string()));
        }
        check_visible(&skill.body)?;
        Ok(skill)
    }

    /// The trial `task` is a run of, if any.
    pub(super) fn trial_of(&self, task: &TaskId) -> Option<TrialRef> {
        let saved = self
            .store
            .get_setting(&format!("{RUN_KEY}{}", task.as_str()))
            .ok()??;
        serde_json::from_str(&saved).ok()
    }

    pub(super) fn mark_trial_run(&self, task: &TaskId, run: &TrialRef) -> AppResult<()> {
        let json = serde_json::to_string(run)
            .map_err(|e| AppError::Port(super::ports::PortError::new(e.to_string())))?;
        Ok(self
            .store
            .set_setting(&format!("{RUN_KEY}{}", task.as_str()), Some(&json))?)
    }

    pub(super) fn forget_trial_run(&self, task: &TaskId) {
        let _ = self
            .store
            .set_setting(&format!("{RUN_KEY}{}", task.as_str()), None);
    }

    /// The skill a task's agents must not see (by id: a rename mid-trial
    /// does not bring it back): the one its trial run is without.
    pub(super) fn skill_left_out(&self, task: &TaskId) -> Option<i64> {
        self.trial_of(task)
            .filter(|run| !run.with_skill)
            .map(|run| run.skill_id)
    }

    /// A trial run cannot be accepted or continued: it is a measurement.
    pub(super) fn refuse_trial_run(&self, task: &TaskId) -> AppResult<()> {
        match self.trial_of(task) {
            Some(_) => Err(AppError::TrialRun),
            None => Ok(()),
        }
    }

    /// The user stopped a trial run: the trial will keep no result.
    pub(super) fn trial_run_stopped(&self, task: &TaskId) {
        if let Some(mut run) = self.trial_of(task) {
            run.stopped = true;
            let _ = self.mark_trial_run(task, &run);
        }
    }

    fn trial_runs(&self, project: &Path, trial: &str) -> Vec<(Task, TrialRef)> {
        self.store
            .tasks(project)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| {
                let of = self.trial_of(&t.id)?;
                (of.trial == trial).then_some((t, of))
            })
            .collect()
    }

    /// Hedy's check in a run's copy: passed when it exits 0 in time.
    fn run_trial_check(&self, trial: &str, ws: &Workspace, project: &Path, check: &str) -> bool {
        let spec = LaunchSpec {
            program: "sh".into(),
            args: vec!["-c".into(), check.to_owned()],
            env: Vec::new(),
            cwd: Some(ws.worktree.clone()),
        };
        self.runner
            .run_check(&spec, CHECK_TIMEOUT, trial, &[project.to_path_buf()])
            .is_ok_and(|out| out.exit_code == Some(0))
    }

    /// After a run stops (finished, failed, stopped or discarded): its check
    /// runs in its copy; once every run of its trial is done, they are
    /// compared (or the trial is voided), the outcome is kept and the runs
    /// are discarded. Best effort.
    pub(super) fn after_trial_run(&self, task: &TaskId) {
        let Some(run) = self.trial_of(task) else {
            return;
        };
        let Ok(current) = self.task(task) else {
            return;
        };
        let stopped =
            |t: &Task| !matches!(t.status, TaskStatus::Running | TaskStatus::WaitingApproval);
        // This run's check, outside the lock: it can take minutes.
        let due =
            !run.checked() && !run.stopped && stopped(&current) && !current.status.is_closed();
        let checked = due.then(|| match (&current.workspace, &run.check) {
            (Some(ws), Some(check)) if current.status == TaskStatus::Review => {
                let restored = self.restore_test_files(ws);
                (
                    self.run_trial_check(&run.trial, ws, &current.project, check),
                    restored,
                )
            }
            _ => (false, 0),
        });
        let _judging = JUDGING.lock().unwrap_or_else(|e| e.into_inner());
        // Read again under the lock: a stop or a cancel that came meanwhile
        // must not be overwritten.
        let Some(mut run) = self.trial_of(task) else {
            return;
        };
        if let Some((passed, restored)) = checked
            && run.checked.is_none()
        {
            run.checked = Some(passed);
            run.restored = restored;
            let _ = self.mark_trial_run(task, &run);
        }
        let runs = self.trial_runs(&current.project, &run.trial);
        let done = |(t, of): &(Task, TrialRef)| {
            t.status.is_closed() || of.stopped || (stopped(t) && of.checked())
        };
        if runs.len() < run.tasks * 2 || !runs.iter().all(done) {
            return;
        }
        let measured: Vec<_> = runs
            .iter()
            .map(|(t, of)| {
                let mut of = of.clone();
                of.stopped |= t.status.is_closed();
                let events = self.store.events(&t.id).unwrap_or_default();
                let can_tell = self
                    .account(t.account.as_str())
                    .is_ok_and(|a| a.provider == Provider::Claude);
                run_from_events(&of, &events, t.status == TaskStatus::Failed, can_tell)
            })
            .collect();
        let outcome = match void_reason(&measured) {
            Some(reason) => TrialOutcome::Void(reason),
            None => {
                let result = judge(&measured, &run, self.now_secs());
                if let Ok(json) = serde_json::to_string(&result) {
                    let key = format!("{RESULT_KEY}{}", run.skill_id);
                    let _ = self.store.set_setting(&key, Some(&json));
                }
                TrialOutcome::Judged(result)
            }
        };
        // Marks first: discarding a run calls back here, and must find none.
        for (t, _) in &runs {
            self.forget_trial_run(&t.id);
        }
        for (t, _) in &runs {
            self.drop_trial_proposals(&t.project, &t.id);
            if !t.status.is_closed() {
                let _ = self.discard_task(&t.id);
            }
        }
        // Last: whoever waits for the outcome finds everything tidied up.
        if let Ok(json) = serde_json::to_string(&outcome) {
            let key = format!("{OUTCOME_KEY}{}", run.trial);
            let _ = self.store.set_setting(&key, Some(&json));
        }
    }

    /// Puts back what a run changed in the files that decide the check
    /// ([`guards_the_check`]; the test keys of `package.json`, `Cargo.toml`
    /// and `pyproject.toml`) as the base commit has them, and removes new
    /// test configuration; new test files stay. How many files it touched.
    fn restore_test_files(&self, ws: &Workspace) -> usize {
        let (dir, base) = (&ws.worktree, ws.base_commit.as_str());
        let Ok(changes) = self.code.changes(dir, base, None) else {
            return 0;
        };
        let mut restored = 0;
        for change in changes {
            let new = matches!(change.kind, ChangeKind::Added | ChangeKind::Untracked);
            if new && configures_the_check(&change.path) {
                restored += usize::from(self.fs.remove_file(&dir.join(&change.path)).is_ok());
                continue;
            }
            // A renamed test comes back under its old name only.
            if let Some(old) = &change.old_path
                && guards_the_check(old)
                && guards_the_check(&change.path)
            {
                let _ = self.fs.remove_file(&dir.join(&change.path));
            }
            for path in std::iter::once(change.path).chain(change.old_path) {
                restored += usize::from(self.restore_file(ws, &path));
            }
        }
        restored
    }

    /// One file of the copy back as the base commit has it (or only its test
    /// keys); `true` when something changed.
    fn restore_file(&self, ws: &Workspace, path: &str) -> bool {
        let (dir, base) = (&ws.worktree, ws.base_commit.as_str());
        let Ok(Some(old)) = self.code.read_file(dir, Some(base), path, RESTORE_MAX) else {
            return false; // new in the run: it stays
        };
        if old.size > RESTORE_MAX {
            return false;
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        let keys: Option<fn(&str, &str) -> Option<String>> = match name {
            "package.json" => Some(restore_scripts),
            "Cargo.toml" | "pyproject.toml" => Some(restore_test_tables),
            _ => None,
        };
        let bytes = match keys {
            Some(restore) => {
                let Ok(Some(now)) = self.code.read_file(dir, None, path, RESTORE_MAX) else {
                    return false;
                };
                let old = String::from_utf8_lossy(&old.bytes);
                match restore(&old, &String::from_utf8_lossy(&now.bytes)) {
                    Some(fixed) => fixed.into_bytes(),
                    None => return false,
                }
            }
            None if guards_the_check(path) => old.bytes,
            None => return false,
        };
        let put = match self.writer.kind(dir, path) {
            Ok(Some(PathKind::File)) => self.writer.replace_file(dir, path, &bytes),
            Ok(None) => self.writer.create_file(dir, path, &bytes),
            // A folder or a link where a test was: left alone.
            _ => return false,
        };
        put.is_ok()
    }

    /// Skills a trial run proposed go with it: a skill under trial must not
    /// fill the review queue. (Notes carry no task: they stay proposals,
    /// which only the user approves.)
    fn drop_trial_proposals(&self, project: &Path, task: &TaskId) {
        if let Ok(skills) = self.store.skills(Some(project), true) {
            for skill in skills {
                if skill.status == SkillStatus::Proposed && skill.source_task.as_ref() == Some(task)
                {
                    let _ = self.store.delete_skill(skill.id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::SkillDraft;
    use crate::application::ports::FileSystem;
    use crate::application::testing::Fixture;
    use crate::domain::skill_check::Verdict;
    use crate::domain::skill_trial::TrialVoid;
    use crate::domain::{AgentEvent, LimitLevel, UsageSample};

    fn setup() -> (Fixture, Skill) {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f.runner.reply_to("sh -c npm test", "ok", 0);
        let skill = f
            .hedy
            .add_skill(
                &f.dir("/code/agenda"),
                &SkillDraft {
                    name: "pr-checklist".into(),
                    description: "Before opening a pull request.".into(),
                    body: "1. Update CHANGELOG.md\n2. Run `npm test`".into(),
                    global: false,
                },
            )
            .unwrap()
            .skill;
        (f, skill)
    }

    fn start(f: &Fixture, skill: &Skill) -> StartedTrial {
        f.hedy
            .start_skill_trial(
                &f.dir("/code/agenda"),
                skill.id,
                &["Prepara un PR del carrito".into()],
                None,
                Some("npm test"),
            )
            .unwrap()
    }

    fn finish(f: &Fixture, n: usize, ok: bool, used: bool, tokens: u64) {
        if used {
            f.agents.emit_from(
                n,
                AgentEvent::ToolUsed {
                    tool: "Skill".into(),
                    summary: "hedy:pr-checklist".into(),
                },
            );
        }
        f.agents.emit_from(
            n,
            AgentEvent::Usage(UsageSample {
                input_tokens: tokens,
                ..Default::default()
            }),
        );
        f.agents.emit_from(
            n,
            AgentEvent::Finished {
                ok,
                summary: "listo".into(),
                duration_ms: None,
            },
        );
    }

    fn outcome(f: &Fixture, trial: &StartedTrial) -> TrialOutcome {
        f.hedy.take_trial_outcome(&trial.id).unwrap().unwrap()
    }

    fn all_discarded(f: &Fixture, trial: &StartedTrial) -> bool {
        trial.tasks.iter().all(|t| {
            f.hedy.task(&t.id).unwrap().status == TaskStatus::Discarded
                && f.hedy.trial_of(&t.id).is_none()
        })
    }

    #[test]
    fn a_trial_runs_each_task_with_and_without_the_skill_then_keeps_only_the_result() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        let trial = start(&f, &skill);
        let tasks = &trial.tasks;
        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].title.starts_with("Prueba con «pr-checklist»"));
        assert!(tasks[1].title.starts_with("Prueba sin «pr-checklist»"));

        // With: in its plugin and its worktree. Without: in neither, and the
        // memory tools do not serve it.
        let requests = f.agents.requests();
        let plugin = |n: usize| requests[n].plugins.first().cloned();
        let with_plugin = plugin(0).unwrap();
        assert!(f.fs.exists(&with_plugin.join("skills/pr-checklist/SKILL.md")));
        assert!(plugin(1).is_none(), "its only skill is left out");
        let file = |t: &Task| {
            let wt = &t.workspace.as_ref().unwrap().worktree;
            f.fs.exists(&wt.join(".agents/skills/pr-checklist/SKILL.md"))
        };
        assert!(file(&tasks[0]) && !file(&tasks[1]));
        let tools = |t: &Task| f.hedy.skill_tools(&agenda, Some(&t.id), "Claude").unwrap();
        assert!(tools(&tasks[0]).get("pr-checklist").contains("CHANGELOG"));
        assert!(!tools(&tasks[1]).get("pr-checklist").contains("CHANGELOG"));
        assert!(!tools(&tasks[1]).list().contains("pr-checklist"));

        // A trial run is a measurement: not accepted, not continued.
        assert!(matches!(
            f.hedy.accept_task(&tasks[0].id),
            Err(AppError::TrialRun)
        ));
        assert!(matches!(
            f.hedy.send_message(&tasks[0].id, "y añade un test"),
            Err(AppError::TrialRun)
        ));

        // One run stops: its check runs in its copy, through the guarded
        // runner and under the trial's name; nothing is judged yet.
        finish(&f, 0, true, true, 12_000);
        let (group, check) = f.runner.checks().pop().unwrap();
        assert_eq!(group, trial.id);
        assert_eq!(check.args, ["-c", "npm test"]);
        assert_eq!(
            check.cwd,
            tasks[0].workspace.as_ref().map(|w| w.worktree.clone())
        );
        assert_eq!(f.hedy.skill_trial(skill.id).unwrap(), None);
        assert!(!f.hedy.trial_settled(&trial.id));

        // Both: judged, kept for the skill, and the runs go.
        finish(&f, 1, false, false, 11_000);
        assert!(f.hedy.trial_settled(&trial.id));
        let TrialOutcome::Judged(judged) = outcome(&f, &trial) else {
            panic!("judged")
        };
        let result = f.hedy.skill_trial(skill.id).unwrap().unwrap();
        assert_eq!(result, judged);
        assert_eq!(result.verdict, Verdict::Useful);
        assert_eq!(
            (result.passed_with, result.passed_without, result.used),
            (1, 0, Some(1))
        );
        assert!(all_discarded(&f, &trial));
        assert_eq!(
            f.hedy.take_trial_outcome(&trial.id).unwrap(),
            None,
            "read once"
        );

        // A later version of the skill was never tried.
        let draft = SkillDraft {
            name: skill.name.clone(),
            description: skill.description.clone(),
            body: "1. Run `npm test`".into(),
            global: false,
        };
        f.hedy.edit_skill(skill.id, &agenda, &draft).unwrap();
        assert_eq!(f.hedy.skill_trial(skill.id).unwrap(), None);
    }

    #[test]
    fn a_limit_or_a_run_the_user_ends_keeps_no_result() {
        let (f, skill) = setup();
        // A run hits the account's limit.
        let trial = start(&f, &skill);
        f.agents.emit_from(
            0,
            AgentEvent::LimitWarning {
                account: "Personal".into(),
                level: LimitLevel::Reached,
                percent: Some(100.0),
                resets_at: None,
            },
        );
        finish(&f, 0, false, false, 0);
        finish(&f, 1, true, false, 9_000);
        assert_eq!(outcome(&f, &trial), TrialOutcome::Void(TrialVoid::Limit));
        assert_eq!(f.hedy.skill_trial(skill.id).unwrap(), None);
        assert!(all_discarded(&f, &trial));

        // The user discards one run (from the app): the other goes too.
        let trial = start(&f, &skill);
        finish(&f, 2, true, true, 9_000);
        f.hedy.discard_task(&trial.tasks[1].id).unwrap();
        assert_eq!(outcome(&f, &trial), TrialOutcome::Void(TrialVoid::Stopped));
        assert!(all_discarded(&f, &trial));

        // The user stops one run.
        let trial = start(&f, &skill);
        f.hedy.stop_task(&trial.tasks[0].id).unwrap();
        finish(&f, 5, true, false, 9_000);
        assert_eq!(outcome(&f, &trial), TrialOutcome::Void(TrialVoid::Stopped));
        assert!(all_discarded(&f, &trial));
        assert_eq!(f.hedy.skill_trial(skill.id).unwrap(), None);
    }

    #[test]
    fn a_run_cannot_pass_by_rewriting_its_tests() {
        use crate::domain::code::{ChangeKind, PathChange};
        let (f, skill) = setup();
        let trial = start(&f, &skill);
        let ws = trial.tasks[0].workspace.clone().unwrap();
        let (wt, base) = (ws.worktree.to_str().unwrap(), Some(ws.base_commit.as_str()));
        let base_json = r#"{"scripts":{"test":"vitest run"}}"#;
        f.code
            .add_file(wt, base, "tests/cart.test.ts", b"expect(total).toBe(3)");
        f.code
            .add_file(wt, None, "tests/cart.test.ts", b"// nothing to see");
        f.code
            .add_file(wt, base, "package.json", base_json.as_bytes());
        f.code.add_file(
            wt,
            None,
            "package.json",
            br#"{"scripts":{"test":"exit 0"}}"#,
        );
        f.code.add_file(wt, base, "src/cart.ts", b"old");
        f.code.add_file(wt, None, "src/cart.ts", b"new");
        f.code
            .add_file(wt, None, "tests/new.test.ts", b"a new test");
        let change = |path: &str, kind| PathChange {
            path: path.into(),
            old_path: None,
            kind,
        };
        f.code.set_changes(
            wt,
            vec![
                change("tests/cart.test.ts", ChangeKind::Modified),
                change("package.json", ChangeKind::Modified),
                change("src/cart.ts", ChangeKind::Modified),
                change("tests/new.test.ts", ChangeKind::Untracked),
                // `runner = "true"` would make any `cargo test` pass.
                change(".cargo/config.toml", ChangeKind::Untracked),
            ],
        );
        finish(&f, 0, true, true, 9_000);
        let working = |path: &str| String::from_utf8(f.code.working(wt, path).unwrap()).unwrap();
        assert_eq!(working("tests/cart.test.ts"), "expect(total).toBe(3)");
        assert!(working("package.json").contains("vitest run"));
        assert_eq!(working("src/cart.ts"), "new", "the run's code stays");
        assert_eq!(working("tests/new.test.ts"), "a new test", "new tests stay");
        assert_eq!(f.hedy.trial_of(&trial.tasks[0].id).unwrap().restored, 3);
        finish(&f, 1, true, false, 9_000);
        let TrialOutcome::Judged(result) = outcome(&f, &trial) else {
            panic!("judged")
        };
        assert_eq!(result.restored, 3);
        assert!(result.summary().contains("3 archivos") || result.summary().contains("3 changed"));
    }

    #[test]
    fn a_trial_where_nothing_passes_keeps_no_result() {
        let (f, skill) = setup();
        // Offline, a missing dependency, or tests already failing.
        f.runner.reply_to("sh -c npm test", "", 1);
        let trial = start(&f, &skill);
        finish(&f, 0, true, true, 9_000);
        finish(&f, 1, true, false, 9_000);
        assert_eq!(
            outcome(&f, &trial),
            TrialOutcome::Void(TrialVoid::NothingPassed)
        );
        assert_eq!(f.hedy.skill_trial(skill.id).unwrap(), None);
        assert!(all_discarded(&f, &trial));
    }

    #[test]
    fn a_cancelled_trial_leaves_nothing() {
        let (f, skill) = setup();
        let trial = start(&f, &skill);
        f.hedy.cancel_skill_trial(&f.dir("/code/agenda"), &trial.id);
        assert_eq!(
            f.runner.stopped_checks(),
            std::slice::from_ref(&trial.id),
            "only its checks"
        );
        assert!(all_discarded(&f, &trial));
        assert_eq!(f.hedy.take_trial_outcome(&trial.id).unwrap(), None);
    }

    #[test]
    fn a_result_never_moves_to_another_skill() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        let trial = start(&f, &skill);
        finish(&f, 0, true, true, 9_000);
        finish(&f, 1, true, false, 9_000);
        outcome(&f, &trial);
        assert!(f.hedy.skill_trial(skill.id).unwrap().is_some());
        f.hedy.remove_skill(skill.id).unwrap();
        let other = f
            .hedy
            .add_skill(
                &agenda,
                &SkillDraft {
                    name: "deploy".into(),
                    description: "Before deploying.".into(),
                    body: "1. Run `make deploy`".into(),
                    global: false,
                },
            )
            .unwrap()
            .skill;
        assert_eq!(f.hedy.skill_trial(other.id).unwrap(), None);
    }

    #[test]
    fn a_trial_needs_one_to_three_tasks_an_approved_skill_and_no_copy_outside() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        for tasks in [vec![], vec![" ".to_owned()], vec!["x".to_owned(); 4]] {
            assert!(matches!(
                f.hedy
                    .start_skill_trial(&agenda, skill.id, &tasks, None, None),
                Err(AppError::TrialTasks)
            ));
        }
        assert!(matches!(
            f.hedy
                .start_skill_trial(&agenda, 999, &["x".into()], None, None),
            Err(AppError::SkillNotFound(_))
        ));
        f.fs.add_file("/code/agenda/.claude/skills/pr-checklist/SKILL.md", "mine");
        assert!(matches!(
            f.hedy
                .start_skill_trial(&agenda, skill.id, &["x".into()], None, None),
            Err(AppError::TrialSkillOutside(_))
        ));
        assert!(f.agents.requests().is_empty(), "nothing started");
    }

    #[test]
    fn test_tasks_are_proposed_from_the_skill_alone_with_a_light_model() {
        let (f, skill) = setup();
        f.agents.reply_on_start(vec![AgentEvent::Finished {
            ok: true,
            summary: "TASK: Prepara un PR del carrito.\nTASK: Sube la versión de react.".into(),
            duration_ms: None,
        }]);
        let tasks = f
            .hedy
            .propose_trial_tasks(&f.dir("/code/agenda"), skill.id, None, None)
            .unwrap();
        assert_eq!(
            tasks,
            ["Prepara un PR del carrito.", "Sube la versión de react."]
        );
        let request = f.agents.last_request();
        assert!(request.no_tools && request.prompt.contains("CHANGELOG"));
        assert_eq!(request.model.as_deref(), Some("haiku"));
    }
}
