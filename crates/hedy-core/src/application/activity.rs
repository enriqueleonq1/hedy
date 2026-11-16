//! The task board ("Actividad"): every project's running and recent tasks,
//! what needs the user first.

use super::tasks::current_stage;
use super::{AppResult, Hedy};
use crate::domain::{AgentEvent, Phase, Task, TaskState, task_state};

/// Finished tasks stay on the board this long after their last event.
pub const RECENT_HOURS: i64 = 24;
/// At most this many finished tasks; running and waiting ones are always shown.
pub const MAX_FINISHED: usize = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct TaskActivity {
    pub task: Task,
    pub state: TaskState,
    /// The phase the task is on (or ended on).
    pub phase: Phase,
    /// Who works on it now: "Claude Opus", "Codex".
    pub agent: String,
    /// The project's name (its folder's, if it is no longer registered).
    pub project_name: String,
    /// When the task last did something (seconds).
    pub updated_at: i64,
    /// Messages waiting for the end of the agent's turn.
    pub queued: usize,
}

impl Hedy {
    /// Running and recent tasks of every project: what needs the user first,
    /// then the rest by their latest activity.
    pub fn activity(&self) -> AppResult<Vec<TaskActivity>> {
        let since = self.now_secs() - RECENT_HOURS * 3600;
        let mut items = Vec::new();
        for task in self.store.recent_tasks(since)? {
            let timed = self.store.timed_events(&task.id)?;
            let updated_at = timed.last().map_or(task.created_at, |(at, _)| *at);
            let events: Vec<AgentEvent> = timed.into_iter().map(|(_, e)| e).collect();
            items.push(TaskActivity {
                state: task_state(&task, &events),
                phase: current_stage(&task).phase,
                agent: self.agent_of(&task, &events),
                project_name: self.place_name(&task),
                updated_at,
                queued: self.store.queued(&task.id)?.len(),
                task,
            });
        }
        items.sort_by(|a, b| {
            a.state
                .rank()
                .cmp(&b.state.rank())
                .then(b.updated_at.cmp(&a.updated_at))
        });
        let mut finished = 0;
        items.retain(|item| {
            if item.state.is_active() {
                return true;
            }
            finished += 1;
            finished <= MAX_FINISHED
        });
        Ok(items)
    }

    /// The agent of the latest phase, or the task account's provider.
    fn agent_of(&self, task: &Task, events: &[AgentEvent]) -> String {
        events
            .iter()
            .rev()
            .find_map(|e| match e {
                AgentEvent::PhaseStarted { agent, .. } => Some(agent.clone()),
                _ => None,
            })
            .or_else(|| {
                self.account(task.account.as_str())
                    .ok()
                    .map(|a| a.provider.agent_label(task.model.as_deref()))
            })
            .unwrap_or_else(|| task.account.to_string())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::application::NewTask;
    use crate::application::testing::Fixture;
    use crate::domain::TaskMode;
    use crate::i18n::{Language, with_language};

    fn start(f: &Fixture, project: &str, prompt: &str, plan_first: bool) -> Task {
        f.fs.add_git_root(project);
        f.hedy
            .use_account(&f.dir(project), "personal", None)
            .unwrap();
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir(project),
                prompt: prompt.into(),
                mode: TaskMode::Project,
                model: Some("opus".into()),
                plan_first,
            })
            .unwrap()
    }

    #[test]
    fn the_board_puts_what_needs_you_first_across_projects() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let finished = start(&f, "/code/agenda", "Traduce los correos", false);
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: None,
        });
        let working = start(&f, "/code/tienda", "Arregla el carrito", false);
        let waiting = start(&f, "/code/agenda", "Agrega recordatorios", true);
        f.agents.emit(AgentEvent::Plan {
            text: "1. Hacerlo".into(),
            request_id: Some("p1".into()),
        });

        let board = f.hedy.activity().unwrap();
        let rows: Vec<_> = board
            .iter()
            .map(|a| (a.task.id.clone(), a.state, a.project_name.as_str()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (waiting.id.clone(), TaskState::Plan, "agenda"),
                (working.id.clone(), TaskState::Working, "tienda"),
                (finished.id.clone(), TaskState::Review, "agenda"),
            ]
        );
        assert_eq!(board[0].phase, Phase::Plan);
        assert_eq!(board[0].agent, "Claude Opus");
        assert!(board[0].updated_at >= board[0].task.created_at);
    }

    #[test]
    fn a_limit_shows_on_the_board_and_never_switches_accounts() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let task = start(&f, "/code/agenda", "Hola", false);
        f.agents.emit(AgentEvent::RateLimit {
            status: "rejected".into(),
            utilization: None,
            resets_at: None,
        });
        let board = f.hedy.activity().unwrap();
        assert_eq!(board[0].state, TaskState::Limited);
        assert_eq!(board[0].task.account.as_str(), "personal");
        assert_eq!(f.hedy.task(&task.id).unwrap().account.as_str(), "personal");
    }

    #[test]
    fn notices_for_plans_permissions_limits_and_endings() {
        with_language(Language::Spanish, notices_in_spanish);
    }

    fn notices_in_spanish() {
        let f = Fixture::new();
        f.add_claude("Personal");
        start(&f, "/code/tienda-online", "Hola", true);
        f.agents.emit(AgentEvent::Plan {
            text: "1. Hacerlo".into(),
            request_id: Some("p1".into()),
        });
        // A second card while already waiting does not notify again.
        f.agents.emit(AgentEvent::PermissionRequest {
            request_id: "r1".into(),
            tool: "Bash".into(),
            summary: "pytest".into(),
            input: json!({"command": "pytest"}),
        });
        f.agents.emit(AgentEvent::RateLimit {
            status: "rejected".into(),
            utilization: None,
            resets_at: None,
        });
        f.agents.emit(AgentEvent::Finished {
            ok: false,
            summary: "error".into(),
            duration_ms: None,
        });
        assert_eq!(
            f.notices.titles(),
            vec![
                "Claude necesita tu aprobación · tienda-online",
                "Claude llegó a su límite · tienda-online",
                "La tarea falló · tienda-online",
            ]
        );
    }

    #[test]
    fn notices_can_be_turned_off() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .set_setting(crate::application::Setting::Notifications, Some("off"))
            .unwrap();
        start(&f, "/code/agenda", "Hola", false);
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: None,
        });
        assert!(f.notices.titles().is_empty());
    }
}
