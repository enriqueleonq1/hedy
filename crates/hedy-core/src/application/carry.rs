//! Carrying a quick question into a project ("Llevar a un proyecto"): a new
//! task in the project whose prompt opens with the conversation's context
//! (the questions and final answers the user kept, or a summary of them),
//! then the user's instruction. The task remembers where it came from
//! (`Task::origin`), so both sides link to each other. Also "Guardar como
//! nota del proyecto": the research outcome as a note the user writes.

use std::path::Path;
use std::time::Duration;

use super::one_shot::{OneShot, OneShotFailure};
use super::tasks::current_stage;
use super::{AppError, AppResult, Hedy, NewImage, NewTask};
use crate::domain::attachment::MAX_IMAGES;
use crate::domain::carry::{
    LONG_CONTEXT_CHARS, clean_summary, context_section, conversation_turns, note_text,
    suggested_project_name, summary_prompt,
};
use crate::domain::{
    Account, Category, ContextTurn, Note, Task, TaskId, TaskOrigin, TaskStatus, policy,
};
use crate::tr;

/// How long "Resumir con IA" waits for the agent's answer.
const SUMMARY_TIMEOUT: Duration = Duration::from_secs(120);

/// What a conversation offers to carry: its turns, a project name and
/// whether its agent is still answering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationContext {
    pub conversation: TaskId,
    pub title: String,
    pub turns: Vec<ContextTurn>,
    /// A safe folder name from the title, for "Proyecto nuevo".
    pub suggested_name: String,
    /// From this many characters the context is long (suggest a summary).
    pub long_after: usize,
    /// The agent is answering: carrying waits for it.
    pub busy: bool,
}

/// What of a conversation goes with a new task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carry {
    pub conversation: TaskId,
    /// Indexes of the turns to keep (of `ConversationContext::turns`).
    pub turns: Vec<usize>,
    /// A summary (from "Resumir con IA", maybe edited) instead of the turns'
    /// text; the kept turns still bring their images.
    pub summary: Option<String>,
}

/// A carry worked out for the agent that starts the task.
pub(super) struct Carried {
    pub origin: TaskOrigin,
    /// The kept questions' images, when that agent takes them all.
    pub images: Vec<NewImage>,
}

fn is_busy(task: &Task) -> bool {
    matches!(
        task.status,
        TaskStatus::Running | TaskStatus::WaitingApproval
    )
}

impl Hedy {
    /// A conversation (never a project's task).
    fn conversation(&self, id: &TaskId) -> AppResult<Task> {
        let task = self.task(id)?;
        if !task.conversation {
            return Err(AppError::NotAConversation(id.clone()));
        }
        Ok(task)
    }

    /// What conversation `id` offers to carry into a project.
    pub fn conversation_context(&self, id: &TaskId) -> AppResult<ConversationContext> {
        let task = self.conversation(id)?;
        let turns = conversation_turns(&self.store.events(id)?);
        Ok(ConversationContext {
            conversation: task.id.clone(),
            suggested_name: suggested_project_name(&task.title),
            title: task.title.clone(),
            turns,
            long_after: LONG_CONTEXT_CHARS,
            busy: is_busy(&task),
        })
    }

    /// The idle conversation and the turns `carry` keeps (in order).
    fn kept_turns(&self, carry: &Carry) -> AppResult<(Task, Vec<ContextTurn>)> {
        let task = self.conversation(&carry.conversation)?;
        if is_busy(&task) {
            return Err(AppError::ConversationBusy(task.id));
        }
        let turns = conversation_turns(&self.store.events(&task.id)?);
        let kept: Vec<ContextTurn> = turns
            .into_iter()
            .enumerate()
            .filter(|(index, _)| carry.turns.contains(index))
            .map(|(_, turn)| turn)
            .collect();
        Ok((task, kept))
    }

    /// The context section as it would open a task's prompt, with the
    /// images only named (the CLI's way to see it).
    pub fn carried_context(&self, carry: &Carry) -> AppResult<String> {
        let (task, kept) = self.kept_turns(carry)?;
        let summary = carry
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if kept.is_empty() && summary.is_none() {
            return Err(AppError::NothingToCarry);
        }
        Ok(context_section(&task.title, &kept, summary, false))
    }

    /// Works out the carry for `account`, the agent that starts the task.
    /// The questions' images go with the message when that agent takes
    /// images and all of them fit with the user's own (`own_images`);
    /// otherwise the context only names them.
    pub(super) fn prepare_carry(
        &self,
        carry: &Carry,
        account: &Account,
        own_images: usize,
    ) -> AppResult<Carried> {
        let (task, kept) = self.kept_turns(carry)?;
        let summary = carry
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if kept.is_empty() && summary.is_none() {
            return Err(AppError::NothingToCarry);
        }
        let wanted: Vec<_> = kept.iter().flat_map(|t| t.images.iter()).collect();
        let attach = !wanted.is_empty()
            && own_images + wanted.len() <= MAX_IMAGES
            && self.support_of(account).accepts();
        let mut images = Vec::new();
        if attach {
            for image in wanted {
                // A deleted file is only named.
                if let Some((_, data)) = self.attachment(&task.id, &image.file)? {
                    images.push(NewImage {
                        name: image.name.clone(),
                        data,
                    });
                }
            }
        }
        let context = context_section(&task.title, &kept, summary, attach);
        Ok(Carried {
            origin: TaskOrigin {
                conversation: task.id,
                title: task.title,
                context,
            },
            images,
        })
    }

    /// "Llevar a un proyecto": a new task in `request.project` that opens
    /// with the conversation's context; `request.prompt` is the instruction.
    pub fn start_carried_task(&self, request: NewTask, carry: Carry) -> AppResult<Task> {
        self.start_task_with(request, Some(carry))
    }

    /// The tasks that continue conversation `id`, newest first.
    pub fn continuations(&self, id: &TaskId) -> AppResult<Vec<Task>> {
        Ok(self.store.continuations(id)?)
    }

    /// "Resumir con IA": the conversation's own account and model (never
    /// another one: rule 3) sum up the kept turns, read-only and without
    /// tools. Spends a few tokens, recorded as the conversation's usage.
    pub fn summarize_conversation(&self, id: &TaskId, turns: &[usize]) -> AppResult<String> {
        self.summarize_conversation_within(id, turns, SUMMARY_TIMEOUT)
    }

    pub(super) fn summarize_conversation_within(
        &self,
        id: &TaskId,
        turns: &[usize],
        timeout: Duration,
    ) -> AppResult<String> {
        let (task, kept) = self.kept_turns(&Carry {
            conversation: id.clone(),
            turns: turns.to_vec(),
            summary: None,
        })?;
        if kept.is_empty() {
            return Err(AppError::NothingToCarry);
        }
        let stage = current_stage(&task);
        let account = self.account(stage.account.as_str())?;
        policy::check_launch(None, &account.id, &self.projects()?)?;
        if self.at_limit(&task, &account)? {
            return Err(AppError::SummaryAtLimit {
                account: account.name.clone(),
            });
        }
        let answer = self.one_shot(OneShot {
            task: Some(id),
            account: &account,
            stage: &stage,
            cwd: task.work_dir(),
            prompt: summary_prompt(&task.title, &kept),
            refusal: tr!(
                "No uses herramientas: responde solo con el resumen.",
                "Do not use tools: reply with the summary only."
            ),
            no_shell: true,
            timeout,
        })?;
        match answer {
            Ok(text) => clean_summary(&text).ok_or(AppError::SummaryFailed(None)),
            Err(OneShotFailure::AtLimit) => Err(AppError::SummaryAtLimit {
                account: account.name.clone(),
            }),
            Err(OneShotFailure::Failed(detail)) => Err(AppError::SummaryFailed(detail)),
            Err(OneShotFailure::TimedOut) => Err(AppError::SummaryTimedOut),
        }
    }

    /// "Guardar como nota del proyecto": the user writes it (title first,
    /// then the text), so it is approved right away like any note of
    /// theirs; rule 6 only makes agents propose.
    pub fn save_conversation_note(
        &self,
        conversation: &TaskId,
        project: &Path,
        title: &str,
        text: &str,
        category: Category,
    ) -> AppResult<Note> {
        self.conversation(conversation)?;
        self.add_note(Some(project), &note_text(title, text), category)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewConversation;
    use crate::application::Setting;
    use crate::application::ports::{RateLimit, TaskRepository, UsageRepository};
    use crate::application::testing::Fixture;
    use crate::domain::{AgentEvent, NoteStatus, TaskMode, UsageSample};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    fn finished(summary: &str) -> AgentEvent {
        AgentEvent::Finished {
            ok: true,
            summary: summary.into(),
            duration_ms: None,
        }
    }

    /// A project with an account, and a finished two-turn conversation.
    fn setup(f: &Fixture) -> Task {
        f.add_claude("Personal");
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("personal"))
            .unwrap();
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        let chat = f
            .hedy
            .start_conversation(NewConversation {
                prompt: "¿Postgres o SQLite?".into(),
                mode: TaskMode::Research,
                account: None,
                model: None,
                images: vec![NewImage {
                    name: "diagrama.png".into(),
                    data: PNG.to_vec(),
                }],
            })
            .unwrap();
        f.agents.emit(AgentEvent::Message {
            text: "Depende.".into(),
        });
        f.agents.emit(finished("SQLite para empezar."));
        f.hedy.send_message(&chat.id, "¿Y para escalar?").unwrap();
        f.agents.emit(finished("Postgres."));
        f.hedy.task(&chat.id).unwrap()
    }

    fn instruction() -> NewTask {
        NewTask {
            project: "/code/agenda".into(),
            prompt: "Crea el esquema".into(),
            mode: TaskMode::Project,
            model: None,
            plan_first: false,
            images: Vec::new(),
        }
    }

    #[test]
    fn the_context_lists_questions_and_final_answers() {
        let f = Fixture::new();
        let chat = setup(&f);
        let context = f.hedy.conversation_context(&chat.id).unwrap();
        assert_eq!(context.title, "¿Postgres o SQLite?");
        assert_eq!(context.suggested_name, "postgres-sqlite");
        assert!(!context.busy);
        assert_eq!(context.long_after, LONG_CONTEXT_CHARS);
        let pairs: Vec<(&str, &str)> = context
            .turns
            .iter()
            .map(|t| (t.question.as_str(), t.answer.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("¿Postgres o SQLite?", "SQLite para empezar."),
                ("¿Y para escalar?", "Postgres.")
            ]
        );
        assert_eq!(context.turns[0].images.len(), 1);

        // A project's task is not a conversation.
        let task = f.hedy.start_task(instruction()).unwrap();
        assert!(matches!(
            f.hedy.conversation_context(&task.id),
            Err(AppError::NotAConversation(_))
        ));
    }

    #[test]
    fn a_carried_task_opens_with_the_kept_context_then_the_instruction() {
        let f = Fixture::new();
        let chat = setup(&f);
        let task = f
            .hedy
            .start_carried_task(
                instruction(),
                Carry {
                    conversation: chat.id.clone(),
                    turns: vec![1],
                    summary: None,
                },
            )
            .unwrap();

        let origin = task.origin.clone().expect("an origin");
        assert_eq!(origin.conversation, chat.id);
        assert_eq!(origin.title, "¿Postgres o SQLite?");
        assert!(
            origin
                .context
                .starts_with("--- Contexto: conversación «¿Postgres o SQLite?» ---")
        );
        assert!(origin.context.contains("Pregunta 1:\n¿Y para escalar?"));
        assert!(!origin.context.contains("SQLite para empezar"), "trimmed");
        assert_eq!(
            task.title, "Crea el esquema",
            "the title is the instruction's"
        );
        assert_eq!(task.instruction(), "Crea el esquema");
        assert_eq!(
            task.prompt,
            format!("{}\n\nCrea el esquema", origin.context)
        );
        assert!(task.workspace.is_some());

        // The agent gets the whole prompt; the history keeps it too.
        assert_eq!(f.agents.last_request().prompt, task.prompt);
        assert_eq!(
            f.hedy.task_events(&task.id).unwrap()[0],
            AgentEvent::user(task.prompt.clone())
        );
        // Persisted, and listed from the conversation.
        assert_eq!(f.hedy.task(&task.id).unwrap().origin, Some(origin));
        assert_eq!(f.hedy.continuations(&chat.id).unwrap(), vec![task.clone()]);
        let second = f
            .hedy
            .start_carried_task(
                instruction(),
                Carry {
                    conversation: chat.id.clone(),
                    turns: vec![0, 1],
                    summary: None,
                },
            )
            .unwrap();
        let ids: Vec<TaskId> = f
            .hedy
            .continuations(&chat.id)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&second.id));
    }

    #[test]
    fn the_questions_images_go_along_when_the_agent_takes_them() {
        let f = Fixture::new();
        let chat = setup(&f);
        let task = f
            .hedy
            .start_carried_task(
                instruction(),
                Carry {
                    conversation: chat.id.clone(),
                    turns: vec![0],
                    summary: None,
                },
            )
            .unwrap();
        assert_eq!(task.images.len(), 1);
        assert_eq!(task.images[0].name, "diagrama.png");
        assert_eq!(f.agents.last_request().images.len(), 1);
        assert!(
            task.origin
                .unwrap()
                .context
                .contains("van adjuntas a este mensaje")
        );

        // With the user's own four images there is no room: only named.
        let own = (0..4)
            .map(|i| NewImage {
                name: format!("{i}.png"),
                data: PNG.to_vec(),
            })
            .collect();
        let task = f
            .hedy
            .start_carried_task(
                NewTask {
                    images: own,
                    ..instruction()
                },
                Carry {
                    conversation: chat.id.clone(),
                    turns: vec![0],
                    summary: None,
                },
            )
            .unwrap();
        assert_eq!(task.images.len(), 4);
        let context = task.origin.unwrap().context;
        assert!(context.contains("(Imágenes: diagrama.png)"));
        assert!(context.contains("no van adjuntas"));
    }

    #[test]
    fn a_summary_replaces_the_turns_text() {
        let f = Fixture::new();
        let chat = setup(&f);
        let task = f
            .hedy
            .start_carried_task(
                instruction(),
                Carry {
                    conversation: chat.id.clone(),
                    turns: vec![],
                    summary: Some("Usar SQLite y pasar a Postgres al escalar.".into()),
                },
            )
            .unwrap();
        let context = task.origin.unwrap().context;
        assert!(context.contains("hecho con IA):\nUsar SQLite y pasar a Postgres al escalar."));
        assert!(!context.contains("Pregunta 1"));
        assert!(task.images.is_empty());
    }

    #[test]
    fn nothing_is_created_for_an_empty_or_busy_carry() {
        let f = Fixture::new();
        let chat = setup(&f);
        let before = f.agents.requests().len();
        let carry = |turns: Vec<usize>, summary: Option<&str>| Carry {
            conversation: chat.id.clone(),
            turns,
            summary: summary.map(Into::into),
        };
        assert!(matches!(
            f.hedy
                .start_carried_task(instruction(), carry(vec![], Some("  "))),
            Err(AppError::NothingToCarry)
        ));
        assert!(matches!(
            f.hedy
                .start_carried_task(instruction(), carry(vec![9], None)),
            Err(AppError::NothingToCarry)
        ));
        assert!(matches!(
            f.hedy.carried_context(&carry(vec![], None)),
            Err(AppError::NothingToCarry)
        ));
        assert!(
            f.hedy
                .carried_context(&carry(vec![0], None))
                .unwrap()
                .contains("no van adjuntas")
        );

        // A new question is being answered.
        f.hedy.send_message(&chat.id, "¿Y MySQL?").unwrap();
        assert!(f.hedy.conversation_context(&chat.id).unwrap().busy);
        assert!(matches!(
            f.hedy
                .start_carried_task(instruction(), carry(vec![0], None)),
            Err(AppError::ConversationBusy(_))
        ));
        assert!(matches!(
            f.hedy.summarize_conversation(&chat.id, &[0]),
            Err(AppError::ConversationBusy(_))
        ));
        assert_eq!(f.agents.requests().len(), before, "no agent started");
        assert!(f.git.worktrees().is_empty(), "no worktree");
    }

    #[test]
    fn the_summary_uses_the_conversation_s_own_account_read_only() {
        let f = Fixture::new();
        let chat = setup(&f);
        f.agents.reply_on_start(vec![
            AgentEvent::Usage(UsageSample {
                input_tokens: 30,
                output_tokens: 5,
                ..Default::default()
            }),
            finished("```markdown\n- Empezar con SQLite.\n```"),
        ]);
        let summary = f.hedy.summarize_conversation(&chat.id, &[0, 1]).unwrap();
        assert_eq!(summary, "- Empezar con SQLite.");

        let request = f.agents.last_request();
        assert_eq!(request.mode, TaskMode::Ask);
        assert!(request.memory.is_none() && request.mcp.is_empty() && request.resume.is_none());
        assert_eq!(request.launch.cwd, Some(chat.project.clone()));
        assert!(request.prompt.starts_with("Resume esta conversación"));
        assert!(request.prompt.contains("¿Y para escalar?"));
        assert_eq!(request.launch.env[0].0, "CLAUDE_CONFIG_DIR");
        // Its tokens count for the conversation's account.
        let usage = f.store.usage_since(&chat.account, 0).unwrap();
        assert_eq!(usage.input_tokens, 30);
        // Nothing went to the conversation's history.
        assert!(!f.hedy.task_events(&chat.id).unwrap().iter().any(
            |e| matches!(e, AgentEvent::Finished { summary, .. } if summary.contains("Empezar"))
        ));
    }

    #[test]
    fn the_summary_never_moves_to_another_account_at_a_limit() {
        let f = Fixture::new();
        let chat = setup(&f);
        let started = f.agents.requests().len();
        f.store
            .set_rate_limit(
                &chat.account,
                &RateLimit {
                    status: "rejected".into(),
                    utilization: None,
                    resets_at: Some(f.hedy.now_secs() + 600),
                    at: f.hedy.now_secs(),
                },
            )
            .unwrap();
        assert!(matches!(
            f.hedy.summarize_conversation(&chat.id, &[0]),
            Err(AppError::SummaryAtLimit { account }) if account == "Personal"
        ));
        assert_eq!(f.agents.requests().len(), started, "no agent started");

        f.store
            .set_rate_limit(
                &chat.account,
                &RateLimit {
                    status: "allowed".into(),
                    utilization: None,
                    resets_at: None,
                    at: f.hedy.now_secs(),
                },
            )
            .unwrap();
        f.agents.reply_on_start(vec![]);
        assert!(matches!(
            f.hedy
                .summarize_conversation_within(&chat.id, &[0], Duration::from_millis(20)),
            Err(AppError::SummaryTimedOut)
        ));
        assert!(matches!(
            f.hedy.summarize_conversation(&chat.id, &[]),
            Err(AppError::NothingToCarry)
        ));
    }

    #[test]
    fn a_note_saved_from_a_conversation_is_the_user_s_and_approved() {
        let f = Fixture::new();
        let chat = setup(&f);
        let note = f
            .hedy
            .save_conversation_note(
                &chat.id,
                &f.dir("/code/agenda"),
                "Base de datos",
                "SQLite para empezar; Postgres al escalar.",
                Category::Decision,
            )
            .unwrap();
        assert_eq!(note.status, NoteStatus::Approved);
        assert_eq!(note.author, crate::domain::memory::USER_AUTHOR);
        assert_eq!(
            note.text,
            "Base de datos\n\nSQLite para empezar; Postgres al escalar."
        );
        assert_eq!(note.project, Some(f.dir("/code/agenda")));
        // Agents read it.
        let read = f
            .hedy
            .memory(&f.dir("/code/agenda"))
            .unwrap()
            .notes
            .into_iter()
            .map(|n| n.id)
            .collect::<Vec<_>>();
        assert_eq!(read, vec![note.id]);
        assert!(matches!(
            f.hedy.save_conversation_note(
                &chat.id,
                &f.dir("/code/nowhere"),
                "x",
                "y",
                Category::Decision
            ),
            Err(AppError::ProjectNotFound(_))
        ));
    }

    #[test]
    fn origins_survive_storage() {
        let f = Fixture::new();
        let chat = setup(&f);
        let task = f
            .hedy
            .start_carried_task(
                instruction(),
                Carry {
                    conversation: chat.id.clone(),
                    turns: vec![0],
                    summary: None,
                },
            )
            .unwrap();
        let stored = f.store.task(&task.id).unwrap().unwrap();
        assert_eq!(stored.origin, task.origin);
    }
}
