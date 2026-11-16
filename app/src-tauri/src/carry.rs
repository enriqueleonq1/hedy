//! Tauri commands for carrying a quick question into a project: what a
//! conversation offers to carry, a summary by its own agent, the tasks that
//! continue it, and a note saved from it. Starting the task itself is
//! `start_task` with a `carry`.

use std::path::PathBuf;

use hedy_core::application::{Carry, ConversationContext};
use hedy_core::domain::{Category, ContextTurn, Note, TaskId};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;
use crate::commands::{CommandResult, blocking};
use crate::dto::TaskDto;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationContextDto {
    pub conversation: TaskId,
    pub title: String,
    pub turns: Vec<ContextTurn>,
    pub suggested_name: String,
    pub long_after: usize,
    pub busy: bool,
}

impl From<ConversationContext> for ConversationContextDto {
    fn from(c: ConversationContext) -> Self {
        Self {
            conversation: c.conversation,
            title: c.title,
            turns: c.turns,
            suggested_name: c.suggested_name,
            long_after: c.long_after,
            busy: c.busy,
        }
    }
}

/// What of a conversation goes with a new task (`start_task`'s `carry`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarryInput {
    pub conversation: String,
    #[serde(default)]
    pub turns: Vec<usize>,
    pub summary: Option<String>,
}

impl From<CarryInput> for Carry {
    fn from(c: CarryInput) -> Self {
        Self {
            conversation: TaskId::new(c.conversation),
            turns: c.turns,
            summary: c.summary.filter(|s| !s.trim().is_empty()),
        }
    }
}

#[tauri::command]
pub async fn conversation_context(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<ConversationContextDto> {
    blocking(&state, move |h| {
        h.conversation_context(&TaskId::new(id)).map(Into::into)
    })
    .await
}

/// "Resumir con IA" with the conversation's own account (spends a few tokens).
#[tauri::command]
pub async fn summarize_conversation(
    state: State<'_, AppState>,
    id: String,
    turns: Vec<usize>,
) -> CommandResult<String> {
    blocking(&state, move |h| {
        h.summarize_conversation(&TaskId::new(id), &turns)
    })
    .await
}

/// The tasks that continue a conversation, newest first.
#[tauri::command]
pub async fn continuations(state: State<'_, AppState>, id: String) -> CommandResult<Vec<TaskDto>> {
    blocking(&state, move |h| {
        Ok(h.continuations(&TaskId::new(id))?
            .into_iter()
            .map(|task| TaskDto {
                live: h.is_running(&task.id),
                task,
            })
            .collect())
    })
    .await
}

/// A note of the user's (approved) with a conversation's outcome.
#[tauri::command]
pub async fn save_conversation_note(
    state: State<'_, AppState>,
    conversation: String,
    project: PathBuf,
    title: String,
    text: String,
    category: Category,
) -> CommandResult<Note> {
    blocking(&state, move |h| {
        h.save_conversation_note(
            &TaskId::new(conversation),
            &project,
            &title,
            &text,
            category,
        )
    })
    .await
}
