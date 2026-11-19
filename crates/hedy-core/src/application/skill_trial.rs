//! Trying a skill on real tasks (level 3, docs/decisions.md): each task runs
//! twice as an ordinary task, each in its own worktree, one with the skill
//! and one without (its plugin, its worktree files and the memory tools
//! leave it out). When every run has stopped, they are compared and
//! discarded: nothing reaches the project. The result stays with the skill
//! on this PC.

use std::path::Path;
use std::sync::Mutex;

use super::ask::AskScope;
use super::one_shot::{OneShot, OneShotFailure};
use super::{AppError, AppResult, Hedy, NewTask};
use crate::domain::skill::check_visible;
use crate::domain::skill_trial::{
    TRIAL_TASKS_MAX, TrialRef, TrialResult, judge, parse_trial_tasks, run_from_events,
    trial_tasks_prompt,
};
use crate::domain::{
    Phase, Provider, Skill, SkillStatus, Stage, Task, TaskId, TaskMode, TaskStatus,
};
use crate::tr;

/// Settings key of a run's trial (by task id).
const RUN_KEY: &str = "skill_trial.run.";
/// Settings key of a skill's last result (by skill id).
const RESULT_KEY: &str = "skill_trial.result.";

/// Two runs that stop at once judge their trial once.
static JUDGING: Mutex<()> = Mutex::new(());

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

    /// Starts a trial of an approved skill: each of `tasks` (1 to 3) as two
    /// project tasks, one with the skill and one without, through the
    /// project's own phases and accounts (`model` for all, if given). They
    /// are ordinary tasks: they show on the board and ask for permissions
    /// like any other. If one cannot start, those already started go.
    pub fn start_skill_trial(
        &self,
        project: &Path,
        skill_id: i64,
        tasks: &[String],
        model: Option<&str>,
    ) -> AppResult<Vec<Task>> {
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
        let trial = format!("k{:x}", self.clock.now_ms());
        let mut started: Vec<Task> = Vec::new();
        for (index, prompt) in prompts.iter().enumerate() {
            for with_skill in [true, false] {
                let run = TrialRef {
                    trial: trial.clone(),
                    skill: skill.name.clone(),
                    skill_id,
                    skill_version: skill.updated_at,
                    with_skill,
                    index,
                    tasks: prompts.len(),
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
                        for task in &started {
                            let _ = self.discard_task(&task.id);
                            self.forget_trial_run(&task.id);
                        }
                        return Err(error);
                    }
                }
            }
        }
        Ok(started)
    }

    /// Stops a trial before it is judged: its runs are discarded and no
    /// result is kept (a half trial would say the wrong thing).
    pub fn cancel_skill_trial(&self, tasks: &[TaskId]) {
        for id in tasks {
            if let Ok(task) = self.task(id)
                && !task.status.is_closed()
            {
                self.forget_trial_run(id);
                let _ = self.discard_task(id);
            }
            self.forget_trial_run(id);
        }
    }

    /// The last trial's result for a skill, on this PC, while the skill is
    /// still the version tried.
    pub fn skill_trial(&self, skill_id: i64) -> AppResult<Option<TrialResult>> {
        let saved = self.store.get_setting(&format!("{RESULT_KEY}{skill_id}"))?;
        let Some(result) = saved.and_then(|json| serde_json::from_str::<TrialResult>(&json).ok())
        else {
            return Ok(None);
        };
        let current = self.store.skill(skill_id)?.map(|s| s.updated_at);
        Ok((current == Some(result.skill_version)).then_some(result))
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

    fn forget_trial_run(&self, task: &TaskId) {
        let _ = self
            .store
            .set_setting(&format!("{RUN_KEY}{}", task.as_str()), None);
    }

    /// The skill a task's agents must not see: the one its trial run is
    /// without.
    pub(super) fn skill_left_out(&self, task: &TaskId) -> Option<String> {
        self.trial_of(task)
            .filter(|run| !run.with_skill)
            .map(|run| run.skill)
    }

    /// After a run stops (finished, failed or stopped): once every run of its
    /// trial has, they are compared, the result is kept for the skill and
    /// the runs are discarded. Best effort.
    pub(super) fn after_trial_run(&self, task: &TaskId) {
        let Some(run) = self.trial_of(task) else {
            return;
        };
        let _judging = JUDGING.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(project) = self.task(task).map(|t| t.project) else {
            return;
        };
        let Ok(tasks) = self.store.tasks(&project) else {
            return;
        };
        let runs: Vec<(Task, TrialRef)> = tasks
            .into_iter()
            .filter_map(|t| {
                let of = self.trial_of(&t.id)?;
                (of.trial == run.trial).then_some((t, of))
            })
            .collect();
        let stopped =
            |t: &Task| !matches!(t.status, TaskStatus::Running | TaskStatus::WaitingApproval);
        if runs.len() < run.tasks * 2 || !runs.iter().all(|(t, _)| stopped(t)) {
            return;
        }
        let measured: Vec<_> = runs
            .iter()
            .map(|(t, of)| run_from_events(of, &self.store.events(&t.id).unwrap_or_default()))
            .collect();
        let result = judge(&measured, self.now_secs(), run.skill_version);
        if let Ok(json) = serde_json::to_string(&result) {
            let key = format!("{RESULT_KEY}{}", run.skill_id);
            let _ = self.store.set_setting(&key, Some(&json));
        }
        for (t, _) in &runs {
            if !t.status.is_closed() {
                let _ = self.discard_task(&t.id);
            }
            self.forget_trial_run(&t.id);
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
    use crate::domain::{AgentEvent, UsageSample};

    fn setup() -> (Fixture, Skill) {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
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

    fn finish(f: &Fixture, n: usize, tests_pass: bool, used: bool, tokens: u64) {
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
            AgentEvent::TestsRun {
                command: "npm test".into(),
                passed: tests_pass,
            },
        );
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
                ok: true,
                summary: "listo".into(),
                duration_ms: None,
            },
        );
    }

    #[test]
    fn a_trial_runs_each_task_with_and_without_the_skill_then_keeps_only_the_result() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        let tasks = f
            .hedy
            .start_skill_trial(
                &agenda,
                skill.id,
                &["Prepara un PR del carrito".into()],
                None,
            )
            .unwrap();
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

        // One run stops: nothing is judged yet.
        finish(&f, 0, true, true, 12_000);
        assert_eq!(f.hedy.skill_trial(skill.id).unwrap(), None);
        assert_eq!(
            f.hedy.task(&tasks[0].id).unwrap().status,
            TaskStatus::Review
        );

        // Both: judged, kept for the skill, and the runs go.
        finish(&f, 1, false, false, 11_000);
        let result = f.hedy.skill_trial(skill.id).unwrap().unwrap();
        assert_eq!(result.verdict, Verdict::Useful);
        assert_eq!(
            (result.passed_with, result.passed_without, result.used),
            (1, 0, 1)
        );
        for task in &tasks {
            assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Discarded);
            assert!(f.hedy.trial_of(&task.id).is_none());
        }

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
    fn a_trial_needs_one_to_three_tasks_and_an_approved_skill() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        for tasks in [vec![], vec![" ".to_owned()], vec!["x".to_owned(); 4]] {
            assert!(matches!(
                f.hedy.start_skill_trial(&agenda, skill.id, &tasks, None),
                Err(AppError::TrialTasks)
            ));
        }
        assert!(matches!(
            f.hedy.start_skill_trial(&agenda, 999, &["x".into()], None),
            Err(AppError::SkillNotFound(_))
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
