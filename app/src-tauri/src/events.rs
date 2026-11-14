//! Pushes task events to the web UI as they happen, and tells the user about
//! tasks with desktop notifications while Hedy's window is not focused.

use hedy_core::application::ports::TaskEvents;
use hedy_core::domain::{AgentEvent, Notice, Task, TaskId};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

pub const TASK_EVENT: &str = "task-event";
pub const TASK_UPDATED: &str = "task-updated";
/// A click on a notification: open that task in its project's workspace.
pub const OPEN_TASK: &str = "open-task";

const MAIN_WINDOW: &str = "main";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TaskEventPayload<'a> {
    task_id: &'a TaskId,
    event: &'a AgentEvent,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenTask {
    task_id: String,
    project: String,
}

pub struct UiEvents(pub AppHandle);

impl UiEvents {
    fn window_focused(&self) -> bool {
        self.0
            .get_webview_window(MAIN_WINDOW)
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false)
    }
}

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

    /// Shown by the OS notification service (D-Bus on Linux, the
    /// Notification Center on macOS): all local. The text only names the
    /// agent and the project; the task travels as data, to open it on a click.
    fn notice(&self, task: &Task, notice: &Notice) {
        if self.window_focused() {
            return;
        }
        let _ = self
            .0
            .notification()
            .builder()
            .title(&notice.title)
            .body(&notice.body)
            .extra("taskId", task.id.as_str())
            .extra("project", task.project.to_string_lossy())
            .show();
    }
}

/// A click on one of Hedy's notifications brings the window to the front and
/// opens that task. Notification servers that do not report clicks only show
/// the notification.
pub fn open_tasks_from_notifications(app: &AppHandle) {
    let handle = app.clone();
    // Never fails on desktop (the signature is shared with mobile).
    let _ = app.notification().on_action(move |performed| {
        if performed.action_id() != "tap" {
            return;
        }
        let Some(notification) = performed.notification() else {
            return;
        };
        let text = |key: &str| {
            notification
                .extra()
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        };
        let (Some(task_id), Some(project)) = (text("taskId"), text("project")) else {
            return;
        };
        if let Some(window) = handle.get_webview_window(MAIN_WINDOW) {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = handle.emit(OPEN_TASK, OpenTask { task_id, project });
    });
}
