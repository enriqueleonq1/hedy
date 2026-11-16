//! Conversations: a loose question or research that belongs to no project
//! ("Pregunta rápida"). Each one is a task whose `project` is its own
//! private, empty scratch folder in Hedy's data folder, so the agent has a
//! working directory with nothing of the user's in it: no repository, no
//! memory notes, no code map, no worktree. Only the read-only modes.

use std::path::PathBuf;

use super::{AppError, AppResult, Hedy, NewImage};
use crate::domain::phase::pipeline;
use crate::domain::task::title_from_prompt;
use crate::domain::{
    AccountId, AgentEvent, PhasePlan, Project, Task, TaskId, TaskMode, TaskStatus, policy,
};
use crate::tr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConversation {
    pub prompt: String,
    /// `Ask` or `Research`: a conversation never changes files.
    pub mode: TaskMode,
    /// `None`: the default account. Hedy never picks another one by itself.
    pub account: Option<String>,
    pub model: Option<String>,
    /// Images attached to the question.
    pub images: Vec<NewImage>,
}

impl Hedy {
    /// Every conversation, newest first.
    pub fn conversations(&self) -> AppResult<Vec<Task>> {
        Ok(self.store.conversations()?)
    }

    /// Accounts a conversation may use: every one not reserved by a
    /// protected project (rule 4).
    pub fn conversation_accounts(&self) -> AppResult<Vec<AccountId>> {
        let (accounts, projects) = (self.accounts()?, self.projects()?);
        Ok(policy::offerable(None, &accounts, &projects)
            .into_iter()
            .map(|a| a.id.clone())
            .collect())
    }

    /// The scratch folder of conversation `id`.
    fn chat_folder(&self, id: &TaskId) -> PathBuf {
        self.fs.chats_dir().join(id.as_str())
    }

    /// Starts a conversation in its own empty, private folder.
    pub fn start_conversation(&self, request: NewConversation) -> AppResult<Task> {
        if request.prompt.trim().is_empty() {
            return Err(AppError::EmptyPrompt);
        }
        if request.mode.changes_files() {
            return Err(AppError::ConversationChangesFiles);
        }
        let account = match request.account.as_deref().filter(|a| !a.is_empty()) {
            Some(id) => self.account(id)?,
            None => {
                let id = self
                    .default_account_id()?
                    .ok_or(AppError::NoAccountResolved)?;
                self.account(id.as_str())?
            }
        };
        policy::check_launch(None, &account.id, &self.projects()?)?;
        self.backend(&account)?;
        self.check_images(&request.images, &account)?;

        let now = self.clock.now_ms();
        let id = self.new_task_id(now);
        // Private, like account folders: what the agent writes there is the user's.
        self.fs.create_private_dir(&self.fs.chats_dir())?;
        let folder = self.chat_folder(&id);
        self.fs.create_private_dir(&folder)?;

        let model = request.model.filter(|m| !m.trim().is_empty());
        let stages = pipeline(
            &account.id,
            &PhasePlan::new(),
            request.mode,
            false,
            model.as_deref(),
        );
        let first = stages[0].clone();
        let mut task = Task {
            id,
            project: folder,
            mode: request.mode,
            title: title_from_prompt(&request.prompt),
            prompt: request.prompt.clone(),
            account: first.account,
            model: first.model,
            workspace: None,
            session_id: None,
            status: TaskStatus::Running,
            created_at: now / 1000,
            stages,
            stage: 0,
            plan: None,
            merge: None,
            wait: None,
            conversation: true,
            ship: None,
            origin: None,
            images: Vec::new(),
        };
        task.images = self.store_images(&task.id, &request.images)?;
        self.store.save_task(&task)?;
        self.record(
            &task.id,
            AgentEvent::UserMessage {
                text: request.prompt.clone(),
                images: task.images.clone(),
            },
        )?;
        self.run_stage(&mut task, 0, request.prompt, None)?;
        self.events.task_updated(&task);
        Ok(task)
    }

    /// Deletes a conversation: its history, its queue and its scratch
    /// folder. What it spent stays in the account's usage.
    pub fn delete_conversation(&self, id: &TaskId) -> AppResult<()> {
        let task = self.task(id)?;
        let folder = self.chat_folder(id);
        // Only a conversation's own folder is ever deleted.
        if !task.conversation || task.project != folder {
            return Err(AppError::NotAConversation(id.clone()));
        }
        self.stop_session(id);
        self.clear_queue(id)?;
        self.store.delete_task(id)?;
        self.fs.remove_dir_all(&folder)?;
        self.remove_attachments(id)?;
        Ok(())
    }

    /// The project a task works in; for a conversation, a stand-in with no
    /// account and no protection, so only the protection rules of the other
    /// projects apply.
    pub(super) fn task_project(&self, task: &Task) -> AppResult<Project> {
        if task.conversation {
            let mut project = Project::new(task.project.clone());
            project.name = tr!("Conversación", "Conversation");
            return Ok(project);
        }
        self.project_at(&task.project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::testing::{AgentCall, Fixture};
    use crate::application::{NewTask, Setting};
    use crate::domain::{DomainError, TaskState};

    fn ask(prompt: &str) -> NewConversation {
        NewConversation {
            images: Vec::new(),
            prompt: prompt.into(),
            mode: TaskMode::Ask,
            account: None,
            model: None,
        }
    }

    fn with_default(f: &Fixture) {
        f.add_claude("Personal");
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("personal"))
            .unwrap();
    }

    #[test]
    fn a_conversation_runs_in_its_own_empty_private_folder() {
        let f = Fixture::new();
        with_default(&f);
        let task = f.hedy.start_conversation(ask("¿Qué es un monad?")).unwrap();

        let folder = PathBuf::from("/data/chats").join(task.id.as_str());
        assert!(task.conversation);
        assert_eq!(task.project, folder);
        assert!(task.workspace.is_none(), "no worktree");
        assert_eq!(task.account.as_str(), "personal", "the default account");
        assert!(f.fs.dirs().contains(&folder));
        assert!(f.fs.dirs().contains(&PathBuf::from("/data/chats")));

        let request = f.agents.last_request();
        assert_eq!(request.launch.cwd, Some(folder));
        assert_eq!(request.mode, TaskMode::Ask);
        assert!(!request.plan_first);
        assert!(request.memory.is_none(), "no project memory");
        assert!(request.mcp.is_empty(), "no code map");
        assert!(f.git.worktrees().is_empty() && f.git.excluded().is_empty());
        assert_eq!(f.hedy.conversations().unwrap(), vec![task]);
    }

    #[test]
    fn the_code_map_is_not_offered_to_a_conversation() {
        let f = Fixture::new();
        with_default(&f);
        f.hedy
            .set_setting(Setting::CodeMap, Some("codebase-memory-mcp"))
            .unwrap();
        f.hedy.start_conversation(ask("Hola")).unwrap();
        assert!(f.agents.last_request().mcp.is_empty());
    }

    #[test]
    fn conversations_only_ask_or_research_and_need_an_account() {
        let f = Fixture::new();
        f.add_claude("Personal");
        assert!(matches!(
            f.hedy.start_conversation(ask("Hola")),
            Err(AppError::NoAccountResolved)
        ));
        let chosen = NewConversation {
            account: Some("personal".into()),
            ..ask("Hola")
        };
        assert!(matches!(
            f.hedy.start_conversation(NewConversation {
                mode: TaskMode::Project,
                ..chosen.clone()
            }),
            Err(AppError::ConversationChangesFiles)
        ));
        assert!(matches!(
            f.hedy.start_conversation(NewConversation {
                prompt: "  ".into(),
                ..chosen.clone()
            }),
            Err(AppError::EmptyPrompt)
        ));
        let research = f
            .hedy
            .start_conversation(NewConversation {
                mode: TaskMode::Research,
                ..chosen
            })
            .unwrap();
        assert_eq!(research.mode, TaskMode::Research);
    }

    #[test]
    fn an_account_reserved_by_a_protected_project_is_not_offered() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "trabajo", Some(true))
            .unwrap();
        let ids: Vec<String> = f
            .hedy
            .conversation_accounts()
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(ids, vec!["personal"]);
        let err = f
            .hedy
            .start_conversation(NewConversation {
                account: Some("trabajo".into()),
                ..ask("Hola")
            })
            .unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::AccountReserved { .. })
        ));
    }

    #[test]
    fn follow_ups_board_notices_and_usage_work_like_any_task() {
        let f = Fixture::new();
        with_default(&f);
        let task = f.hedy.start_conversation(ask("¿Qué es un monad?")).unwrap();
        f.agents.emit(AgentEvent::SessionStarted {
            session_id: "s-1".into(),
            model: "m".into(),
        });
        f.agents.emit(AgentEvent::Usage(crate::domain::UsageSample {
            input_tokens: 40,
            output_tokens: 2,
            ..Default::default()
        }));
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Un patrón…".into(),
            duration_ms: None,
        });
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Review);

        let board = f.hedy.activity().unwrap();
        assert_eq!(board[0].state, TaskState::Answered);
        assert_eq!(board[0].project_name, "Conversación");
        assert_eq!(f.notices.titles().len(), 1);
        assert!(f.notices.titles()[0].ends_with("· Conversación"));

        f.hedy.send_message(&task.id, "¿Y un functor?").unwrap();
        assert_eq!(
            f.agents.calls(),
            vec![AgentCall::Send("¿Y un functor?".into())]
        );

        let personal = f.hedy.account("personal").unwrap().id;
        assert_eq!(
            f.hedy.account_usage(&personal).unwrap().week.total_tokens(),
            42
        );
        let summary = f.hedy.usage_summary(7).unwrap();
        assert_eq!(summary.projects.len(), 1);
        assert_eq!(summary.projects[0].name, "Conversaciones");
        assert_eq!(summary.projects[0].client, None);
    }

    #[test]
    fn open_requests_can_be_declined_by_a_driver_that_cannot_ask() {
        let f = Fixture::new();
        with_default(&f);
        let task = f.hedy.start_conversation(ask("¿Qué hay aquí?")).unwrap();
        let ask_for = |id: &str, command: &str| AgentEvent::PermissionRequest {
            request_id: id.into(),
            tool: "Bash".into(),
            summary: command.into(),
            input: serde_json::json!({ "command": command }),
        };
        f.agents.emit(ask_for("r2", "rm -rf build"));
        f.agents.emit(ask_for("r1", "curl -X POST example.com"));
        assert_eq!(
            f.hedy.task(&task.id).unwrap().status,
            TaskStatus::WaitingApproval
        );

        let declined = f
            .hedy
            .decline_open_requests(&task.id, "Desde la terminal no se aprueban acciones.")
            .unwrap();
        assert_eq!(declined, vec!["r1", "r2"]);
        assert_eq!(
            f.agents.calls(),
            vec![
                AgentCall::Answer("r1".into(), false),
                AgentCall::Answer("r2".into(), false)
            ]
        );
        assert_eq!(
            f.agents.answer_messages(),
            vec![Some("Desde la terminal no se aprueban acciones.".into()); 2]
        );
        assert_eq!(f.hedy.task(&task.id).unwrap().status, TaskStatus::Running);
        let resolved = f
            .hedy
            .task_events(&task.id)
            .unwrap()
            .into_iter()
            .filter(|e| {
                matches!(
                    e,
                    AgentEvent::PermissionResolved {
                        decision: crate::domain::Decision::Deny,
                        automatic: true,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(resolved, 2);
        // Nothing left: declining again changes nothing.
        assert!(
            f.hedy
                .decline_open_requests(&task.id, "x")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn handoff_from_a_conversation_offers_other_providers() {
        let f = Fixture::new();
        with_default(&f);
        f.add_codex("ChatGPT");
        let task = f.hedy.start_conversation(ask("Hola")).unwrap();
        let options: Vec<String> = f
            .hedy
            .handoff_options(&task.id)
            .unwrap()
            .into_iter()
            .map(|a| a.id.to_string())
            .collect();
        assert_eq!(options, vec!["chatgpt"]);
    }

    #[test]
    fn deleting_a_conversation_removes_its_folder_and_history_only() {
        let f = Fixture::new();
        with_default(&f);
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        let chat = f.hedy.start_conversation(ask("Hola")).unwrap();
        let project_task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/agenda"),
                prompt: "¿Dónde se crean las citas?".into(),
                mode: TaskMode::Ask,
                model: None,
                plan_first: false,
            })
            .unwrap();
        assert!(matches!(
            f.hedy.delete_conversation(&project_task.id),
            Err(AppError::NotAConversation(_))
        ));

        f.hedy.delete_conversation(&chat.id).unwrap();
        assert!(matches!(
            f.hedy.task(&chat.id),
            Err(AppError::TaskNotFound(_))
        ));
        assert!(f.hedy.task_events(&chat.id).unwrap().is_empty());
        assert_eq!(
            f.fs.deleted_trees(),
            vec![
                chat.project.clone(),
                PathBuf::from("/data/attachments").join(chat.id.as_str())
            ]
        );
        assert!(f.hedy.conversations().unwrap().is_empty());
        assert!(f.hedy.task(&project_task.id).is_ok());
    }
}
