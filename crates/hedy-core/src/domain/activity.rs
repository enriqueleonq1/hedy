//! What a task needs from the user right now, for the task board ("Actividad")
//! and the desktop notifications. Derived from the task and its history; nothing
//! here is stored.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{AgentEvent, LimitLevel, Task, TaskMode, TaskStatus};
use crate::tr;

/// The state a task shows on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    /// The agent is working.
    Working,
    /// A plan waits for approval.
    Plan,
    /// A permission card (command, edit…) waits for an answer.
    Permission,
    /// The account hit its limit. Hedy never switches accounts by itself: the
    /// user waits or hands the task over.
    Limited,
    /// Finished with changes to review.
    Review,
    /// A question or research task that finished: the answer is there.
    Answered,
    /// The changes were accepted (merged).
    Accepted,
    Failed,
    /// Discarded by the user.
    Cancelled,
}

impl TaskState {
    /// The task waits for the user: a plan or a permission card to answer,
    /// or a limit (wait, or hand the task over: Hedy never decides that).
    pub fn needs_you(self) -> bool {
        matches!(self, Self::Plan | Self::Permission | Self::Limited)
    }

    /// The agent is still on it, or waits for the user to go on.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Working | Self::Plan | Self::Permission | Self::Limited
        )
    }

    /// Board order: what needs the user first, closed tasks last.
    pub fn rank(self) -> u8 {
        match self {
            Self::Plan | Self::Permission => 0,
            Self::Limited => 1,
            Self::Working => 2,
            Self::Review => 3,
            Self::Failed => 4,
            Self::Answered => 5,
            Self::Accepted => 6,
            Self::Cancelled => 7,
        }
    }

    /// Short label for the CLI.
    pub fn label(self) -> String {
        match self {
            Self::Working => tr!("trabajando", "working"),
            Self::Plan => tr!("plan por aprobar", "plan to approve"),
            Self::Permission => tr!("pide permiso", "asks for permission"),
            Self::Limited => tr!("en su límite", "at its limit"),
            Self::Review => tr!("cambios por revisar", "changes to review"),
            Self::Answered => tr!("respondida", "answered"),
            Self::Accepted => tr!("aceptada", "accepted"),
            Self::Failed => tr!("falló", "failed"),
            Self::Cancelled => tr!("descartada", "discarded"),
        }
    }
}

/// The state of `task`, given its history (oldest event first).
pub fn task_state(task: &Task, events: &[AgentEvent]) -> TaskState {
    match task.status {
        TaskStatus::WaitingApproval => {
            // A flow at its approval step waits for an OK, like a plan.
            let flow_ok = task
                .flow
                .as_ref()
                .and_then(|r| r.current())
                .is_some_and(|n| matches!(n.kind, crate::domain::flow::NodeKind::Approval));
            if flow_ok || open_plan(events) {
                TaskState::Plan
            } else {
                TaskState::Permission
            }
        }
        TaskStatus::Accepted => TaskState::Accepted,
        TaskStatus::Discarded => TaskState::Cancelled,
        TaskStatus::Running | TaskStatus::Failed if limit_reached(events) => TaskState::Limited,
        TaskStatus::Running => TaskState::Working,
        TaskStatus::Failed => TaskState::Failed,
        TaskStatus::Review if task.mode == TaskMode::Project => TaskState::Review,
        TaskStatus::Review => TaskState::Answered,
    }
}

/// Whether a proposed plan still waits for an answer.
fn open_plan(events: &[AgentEvent]) -> bool {
    let mut open = HashSet::new();
    for event in events {
        match event {
            AgentEvent::Plan {
                request_id: Some(id),
                ..
            } => {
                open.insert(id.as_str());
            }
            AgentEvent::PermissionResolved { request_id, .. } => {
                open.remove(request_id.as_str());
            }
            _ => {}
        }
    }
    !open.is_empty()
}

/// Whether the account reached its limit since the last time the user (or a
/// handoff) set the task going.
fn limit_reached(events: &[AgentEvent]) -> bool {
    let mut reached = false;
    for event in events {
        match event {
            AgentEvent::LimitWarning {
                level: LimitLevel::Reached,
                ..
            } => reached = true,
            AgentEvent::UserMessage { .. } | AgentEvent::PhaseStarted { .. } => reached = false,
            AgentEvent::RateLimit {
                status,
                utilization,
                ..
            } if LimitLevel::from_report(status, *utilization) != Some(LimitLevel::Reached) => {
                reached = false;
            }
            _ => {}
        }
    }
    reached
}

/// Why Hedy shows a desktop notification about a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    PlanReady,
    PermissionNeeded,
    /// The agent finished: `changes` when there is something to review.
    Finished {
        changes: bool,
    },
    Failed,
    LimitReached,
    /// The limit the user waited for renewed: Hedy resumed the task on the
    /// same account (`resumed`), or the user can now ("Ya puedes continuar").
    LimitRenewed {
        resumed: bool,
    },
}

/// A desktop notification. Short, and never with file contents, commands or
/// secrets: only who, what kind of thing and which project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    /// "Claude necesita tu aprobación · tienda-online".
    pub title: String,
    pub body: String,
}

impl Notice {
    /// `agent` is the provider ("Claude", "Codex"); `project` the project's name.
    pub fn new(kind: NoticeKind, agent: &str, project: &str) -> Self {
        let (title, body) = match kind {
            NoticeKind::PlanReady => (
                tr!(
                    "{agent} necesita tu aprobación · {project}",
                    "{agent} needs your approval · {project}"
                ),
                tr!(
                    "Hay un plan esperando tu respuesta.",
                    "A plan is waiting for your answer."
                ),
            ),
            NoticeKind::PermissionNeeded => (
                tr!(
                    "{agent} pide permiso · {project}",
                    "{agent} asks for permission · {project}"
                ),
                tr!(
                    "Una acción espera tu respuesta.",
                    "An action is waiting for your answer."
                ),
            ),
            NoticeKind::Finished { changes } => (
                tr!(
                    "{agent} terminó · {project}",
                    "{agent} finished · {project}"
                ),
                if changes {
                    tr!("Hay cambios para revisar.", "There are changes to review.")
                } else {
                    tr!("La respuesta está lista.", "The answer is ready.")
                },
            ),
            NoticeKind::Failed => (
                tr!("La tarea falló · {project}", "The task failed · {project}"),
                tr!("{agent} no pudo terminar.", "{agent} could not finish."),
            ),
            NoticeKind::LimitReached => (
                tr!(
                    "{agent} llegó a su límite · {project}",
                    "{agent} hit its limit · {project}"
                ),
                tr!(
                    "Puedes esperar o pasar la tarea a otra IA.",
                    "You can wait or hand the task to another AI."
                ),
            ),
            NoticeKind::LimitRenewed { resumed: true } => (
                tr!(
                    "{agent} sigue con la tarea · {project}",
                    "{agent} is back on the task · {project}"
                ),
                tr!(
                    "Su límite se renovó y Hedy retomó la tarea con la misma cuenta.",
                    "Its limit renewed and Hedy resumed the task with the same account."
                ),
            ),
            NoticeKind::LimitRenewed { resumed: false } => (
                tr!(
                    "Ya puedes continuar · {project}",
                    "You can continue now · {project}"
                ),
                tr!(
                    "El límite de {agent} se renovó. Abre la tarea para seguir.",
                    "{agent}'s limit renewed. Open the task to go on."
                ),
            ),
        };
        Self { kind, title, body }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::domain::{AccountId, Decision, Phase};
    use crate::i18n::{Language, with_language};

    fn task(status: TaskStatus, mode: TaskMode) -> Task {
        Task {
            images: Vec::new(),
            id: crate::domain::TaskId::new("t1"),
            project: PathBuf::from("/code/agenda"),
            mode,
            title: "Recordatorios".into(),
            prompt: "Recordatorios".into(),
            account: AccountId::new("personal").unwrap(),
            model: None,
            workspace: None,
            scratch: None,
            permissions: Default::default(),
            flow: None,
            session_id: None,
            status,
            created_at: 0,
            stages: Vec::new(),
            stage: 0,
            plan: None,
            merge: None,
            wait: None,
            conversation: false,
            ship: None,
            origin: None,
            code_map: None,
        }
    }

    fn plan(id: &str) -> AgentEvent {
        AgentEvent::Plan {
            text: "1. Hacerlo".into(),
            request_id: Some(id.into()),
        }
    }

    fn resolved(id: &str) -> AgentEvent {
        AgentEvent::PermissionResolved {
            request_id: id.into(),
            decision: Decision::Allow,
            automatic: false,
            reason: None,
        }
    }

    fn limit(level: LimitLevel) -> AgentEvent {
        AgentEvent::LimitWarning {
            account: "personal".into(),
            level,
            percent: None,
            resets_at: None,
        }
    }

    #[test]
    fn waiting_tasks_tell_a_plan_from_a_permission() {
        let waiting = task(TaskStatus::WaitingApproval, TaskMode::Project);
        assert_eq!(task_state(&waiting, &[plan("p1")]), TaskState::Plan);
        let permission = AgentEvent::PermissionRequest {
            request_id: "r1".into(),
            tool: "Bash".into(),
            summary: "pytest".into(),
            input: serde_json::Value::Null,
        };
        assert_eq!(
            task_state(&waiting, &[plan("p1"), resolved("p1"), permission]),
            TaskState::Permission
        );
        assert!(TaskState::Plan.needs_you() && TaskState::Permission.needs_you());
        assert!(TaskState::Limited.needs_you() && !TaskState::Working.needs_you());
    }

    #[test]
    fn a_reached_limit_blocks_until_the_task_goes_on() {
        let running = task(TaskStatus::Running, TaskMode::Project);
        assert_eq!(
            task_state(&running, &[limit(LimitLevel::Warning)]),
            TaskState::Working
        );
        assert_eq!(
            task_state(&running, &[limit(LimitLevel::Reached)]),
            TaskState::Limited
        );
        let failed = task(TaskStatus::Failed, TaskMode::Project);
        assert_eq!(
            task_state(&failed, &[limit(LimitLevel::Reached)]),
            TaskState::Limited
        );
        let handoff = AgentEvent::PhaseStarted {
            phase: Phase::Implement,
            agent: "Codex".into(),
            account: "ChatGPT".into(),
        };
        assert_eq!(
            task_state(&running, &[limit(LimitLevel::Reached), handoff]),
            TaskState::Working
        );
        let renewed = AgentEvent::RateLimit {
            status: "allowed".into(),
            utilization: None,
            resets_at: None,
        };
        assert_eq!(
            task_state(&running, &[limit(LimitLevel::Reached), renewed]),
            TaskState::Working
        );
    }

    #[test]
    fn finished_tasks() {
        let review = task(TaskStatus::Review, TaskMode::Project);
        assert_eq!(task_state(&review, &[]), TaskState::Review);
        let answered = task(TaskStatus::Review, TaskMode::Ask);
        assert_eq!(task_state(&answered, &[]), TaskState::Answered);
        assert_eq!(
            task_state(&task(TaskStatus::Discarded, TaskMode::Project), &[]),
            TaskState::Cancelled
        );
        assert_eq!(
            task_state(&task(TaskStatus::Accepted, TaskMode::Project), &[]),
            TaskState::Accepted
        );
        assert_eq!(
            task_state(&task(TaskStatus::Failed, TaskMode::Project), &[]),
            TaskState::Failed
        );
    }

    #[test]
    fn what_needs_the_user_comes_first() {
        let mut states = vec![
            TaskState::Cancelled,
            TaskState::Working,
            TaskState::Review,
            TaskState::Permission,
            TaskState::Limited,
        ];
        states.sort_by_key(|s| s.rank());
        assert_eq!(
            states,
            vec![
                TaskState::Permission,
                TaskState::Limited,
                TaskState::Working,
                TaskState::Review,
                TaskState::Cancelled,
            ]
        );
    }

    #[test]
    fn notices_are_short_and_bilingual() {
        let spanish = with_language(Language::Spanish, || {
            Notice::new(NoticeKind::PlanReady, "Claude", "tienda-online")
        });
        assert_eq!(
            spanish.title,
            "Claude necesita tu aprobación · tienda-online"
        );
        let english = with_language(Language::English, || {
            Notice::new(NoticeKind::Finished { changes: true }, "Codex", "agenda")
        });
        assert_eq!(english.title, "Codex finished · agenda");
        assert_eq!(english.body, "There are changes to review.");
        let renewed = with_language(Language::Spanish, || {
            Notice::new(
                NoticeKind::LimitRenewed { resumed: false },
                "Claude",
                "agenda",
            )
        });
        assert_eq!(renewed.title, "Ya puedes continuar · agenda");
        let resumed = with_language(Language::English, || {
            Notice::new(
                NoticeKind::LimitRenewed { resumed: true },
                "Claude",
                "agenda",
            )
        });
        assert_eq!(resumed.title, "Claude is back on the task · agenda");
    }
}
