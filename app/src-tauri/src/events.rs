//! Pushes task events to the web UI as they happen.

use hedy_core::application::ports::TaskEvents;
use hedy_core::domain::{AgentEvent, Task, TaskId};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const TASK_EVENT: &str = "task-event";
pub const TASK_UPDATED: &str = "task-updated";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TaskEventPayload<'a> {
    task_id: &'a TaskId,
    event: &'a AgentEvent,
}

pub struct UiEvents(pub AppHandle);

impl TaskEvents for UiEvents {
    fn event(&self, task: &TaskId, event: &AgentEvent) {
        let _ = self.0.emit(
            TASK_EVENT,
            TaskEventPayload {
                task_id: task,
                event,
            },
        );
    }

    fn task_updated(&self, task: &Task) {
        let _ = self.0.emit(TASK_UPDATED, task);
    }
}
