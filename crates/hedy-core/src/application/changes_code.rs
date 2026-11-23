//! A question that goes on to change code (docs/decisions.md §55): a task in
//! "Solo preguntar" or "Investigar" becomes one that changes code, in the
//! same task and history. It gets a branch of its own and a fresh agent
//! session, whose first prompt opens with what was asked and answered before.

use super::{AppError, AppResult, Hedy, NewImage};
use crate::domain::carry::{carried_prompt, conversation_turns, earlier_turns_section};
use crate::domain::phase::pipeline;
use crate::domain::{AgentEvent, Task, TaskId, TaskMode, TaskStatus, policy};

/// What the user sends to start changing code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeCode {
    pub text: String,
    pub images: Vec<NewImage>,
    pub plan_first: bool,
}

impl Hedy {
    /// Turns question `id` into a task that changes code, starting with the
    /// user's `request`. Refused while its agent is answering.
    pub fn change_code(&self, id: &TaskId, request: ChangeCode) -> AppResult<Task> {
        let mut task = self.task(id)?;
        if task.status.is_closed() {
            return Err(AppError::TaskClosed(id.clone()));
        }
        self.refuse_trial_run(id)?;
        if task.conversation || task.mode.changes_files() {
            return Err(AppError::NotAQuestion(id.clone()));
        }
        if self.turn_in_progress(&task) {
            return Err(AppError::QuestionBusy(id.clone()));
        }
        let text = request.text.trim();
        if text.is_empty() {
            return Err(AppError::EmptyPrompt);
        }
        // As a new task that changes code would start: the project's
        // account and phases, every agent allowed here and ready.
        let project = self.project_at(&task.project)?;
        let main_account = project.account.clone().ok_or(AppError::NoAccountResolved)?;
        let stages = pipeline(
            &main_account,
            &project.phases,
            TaskMode::Project,
            request.plan_first,
            None,
        );
        let projects = self.projects()?;
        for (index, stage) in stages.iter().enumerate() {
            policy::check_launch(Some(&project), &stage.account, &projects)?;
            let account = self.account(stage.account.as_str())?;
            self.backend(&account)?;
            if index == 0 {
                self.check_images(&request.images, &account)?;
            }
        }

        let context = earlier_turns_section(&conversation_turns(&self.store.events(id)?));
        self.stop_session(id);
        self.close_copy(&mut task)?;
        let workspace = self.create_workspace(&project.path, &task.title, id)?;
        let branch = workspace.branch.clone();
        let first = stages[0].clone();
        // Later phases read the task's prompt: now what it is asked to do.
        let prompt = match context {
            Some(context) => carried_prompt(&context, text),
            None => text.to_owned(),
        };
        task.prompt = prompt.clone();
        task.mode = TaskMode::Project;
        task.workspace = Some(workspace);
        task.stages = stages;
        task.stage = 0;
        task.account = first.account;
        task.model = first.model;
        task.session_id = None;
        task.plan = None;
        task.wait = None;
        task.status = TaskStatus::Running;
        // Earlier images are named in the context; the new ones go along.
        task.images = self.store_images(id, &request.images)?;
        self.store.save_task(&task)?;
        self.record(id, AgentEvent::ChangesCode { branch })?;
        self.record(
            id,
            AgentEvent::UserMessage {
                text: text.to_owned(),
                images: task.images.clone(),
            },
        )?;
        let prompt = self.with_briefing(&task, prompt);
        self.run_stage(&mut task, 0, prompt, None)?;
        self.events.task_updated(&task);
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewTask;
    use crate::application::testing::{AgentCall, Fixture};
    use crate::domain::Phase;

    fn project(f: &Fixture) {
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
    }

    fn ask(f: &Fixture, mode: TaskMode) -> Task {
        f.hedy
            .start_task(NewTask {
                project: f.dir("/code/agenda"),
                prompt: "¿Por qué falla el login con mayúsculas?".into(),
                mode,
                model: None,
                plan_first: false,
                permissions: None,
                images: Vec::new(),
            })
            .unwrap()
    }

    fn answer(f: &Fixture) {
        f.agents.emit(AgentEvent::Message {
            text: "El correo no se pasa a minúsculas.".into(),
        });
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "No se normaliza el correo.".into(),
            duration_ms: None,
        });
    }

    fn change(text: &str, plan_first: bool) -> ChangeCode {
        ChangeCode {
            text: text.into(),
            images: Vec::new(),
            plan_first,
        }
    }

    #[test]
    fn a_question_goes_on_to_change_code_with_what_was_said() {
        let f = Fixture::new();
        project(&f);
        let task = ask(&f, TaskMode::Ask);
        assert!(task.workspace.is_none());
        answer(&f);

        let changed = f
            .hedy
            .change_code(&task.id, change("Arréglalo", false))
            .unwrap();
        assert_eq!(changed.id, task.id);
        assert_eq!(changed.mode, TaskMode::Project);
        assert_eq!(changed.status, TaskStatus::Running);
        let branch = changed.workspace.as_ref().unwrap().branch.clone();
        let stored = f.hedy.task(&task.id).unwrap();
        assert_eq!(stored.mode, TaskMode::Project);
        assert!(stored.prompt.ends_with("Arréglalo"), "later phases read it");

        // A fresh session, in the copy, changing code.
        let request = f.agents.last_request();
        assert_eq!(request.mode, TaskMode::Project);
        assert_eq!(request.resume, None);
        assert!(f.agents.calls().contains(&AgentCall::Stop));
        assert!(request.prompt.contains("lo hablado antes en esta tarea"));
        assert!(
            request
                .prompt
                .contains("¿Por qué falla el login con mayúsculas?")
        );
        assert!(request.prompt.contains("No se normaliza el correo."));
        assert!(
            request
                .prompt
                .contains("--- Fin del contexto ---\n\nArréglalo")
        );

        // The history goes on: a divider, then the user's message.
        let events = f.hedy.task_events(&task.id).unwrap();
        let at = events
            .iter()
            .position(|e| {
                *e == AgentEvent::ChangesCode {
                    branch: branch.clone(),
                }
            })
            .expect("a divider");
        assert_eq!(
            events[at + 1],
            AgentEvent::UserMessage {
                text: "Arréglalo".into(),
                images: Vec::new(),
            }
        );
    }

    #[test]
    fn research_can_start_with_a_plan() {
        let f = Fixture::new();
        project(&f);
        let task = ask(&f, TaskMode::Research);
        answer(&f);
        let changed = f.hedy.change_code(&task.id, change("Hazlo", true)).unwrap();
        assert_eq!(changed.stages[0].phase, Phase::Plan);
        assert!(f.agents.last_request().plan_first);
    }

    #[test]
    fn only_an_idle_question_changes_code() {
        let f = Fixture::new();
        project(&f);
        let task = ask(&f, TaskMode::Ask);
        assert_eq!(
            f.hedy.change_code(&task.id, change("Arréglalo", false)),
            Err(AppError::QuestionBusy(task.id.clone()))
        );
        answer(&f);
        assert_eq!(
            f.hedy.change_code(&task.id, change("  ", false)),
            Err(AppError::EmptyPrompt)
        );
        f.hedy
            .change_code(&task.id, change("Arréglalo", false))
            .unwrap();
        answer(&f);
        assert_eq!(
            f.hedy.change_code(&task.id, change("Otra vez", false)),
            Err(AppError::NotAQuestion(task.id.clone()))
        );
    }
}
