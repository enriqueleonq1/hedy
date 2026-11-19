//! Tauri command for checking a task's changes against the written rules
//! (docs/decisions.md §37): "Revisar con tus reglas" in the Cambios tab.

use hedy_core::application::{RuleCheck, RuleTarget};
use hedy_core::domain::TaskId;
use hedy_core::domain::ask::Source;
use hedy_core::domain::rule_check::RuleBreak;
use serde::Serialize;
use tauri::State;

use crate::AppState;
use crate::commands::{CommandResult, blocking};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCheckDto {
    /// `checked`, `unread` (not a clean change), `no_rules` or
    /// `nothing_to_check` (the last two sent nothing).
    pub status: &'static str,
    pub breaks: Vec<RuleBreak>,
    /// Every rule sent, numbered from 1 in this order.
    pub rules: Vec<Source>,
    /// Files whose lines were judged.
    pub files: usize,
    /// Protected files: named, never sent.
    pub protected: Vec<String>,
    /// Only part of the change was judged.
    pub truncated: bool,
    /// The account's name; `None` when nothing was sent.
    pub account: Option<String>,
}

impl From<RuleCheck> for RuleCheckDto {
    fn from(c: RuleCheck) -> Self {
        Self {
            status: c.status(),
            breaks: c.breaks,
            rules: c.rules,
            files: c.files,
            protected: c.protected,
            truncated: c.truncated,
            account: c.account.map(|a| a.name),
        }
    }
}

#[tauri::command]
pub async fn check_task_rules(
    state: State<'_, AppState>,
    task: String,
    account: Option<String>,
) -> CommandResult<RuleCheckDto> {
    blocking(&state, move |h| {
        Ok(h.check_rules(
            &RuleTarget::Task(TaskId::new(task)),
            account.as_deref(),
            None,
        )?
        .into())
    })
    .await
}

/// The command with the arguments `tauriApi.ts` sends, through Tauri's IPC
/// (nothing reaches an AI: the task does not exist).
#[cfg(test)]
mod tests {
    use hedy_core::adapters::{self, HedyPaths};
    use serde_json::{Value, json};
    use tauri::Manager;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, noop_assets};
    use tauri::webview::InvokeRequest;

    use crate::AppState;

    #[test]
    fn check_task_rules_over_ipc() {
        hedy_core::i18n::set_language(hedy_core::i18n::Language::Spanish);
        let tmp = tempfile::tempdir().unwrap();
        let paths = HedyPaths::at(std::path::absolute(tmp.path().join("data")).unwrap());
        let program = std::env::current_exe().unwrap();
        let hedy = adapters::system_for_app(&paths, &program).unwrap();
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![super::check_task_rules])
            .build(tauri::generate_context!(
                test = true,
                assets = noop_assets()
            ))
            .unwrap();
        app.manage(AppState {
            hedy,
            paths,
            terminals: std::sync::Arc::default(),
        });
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let error = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: "check_task_rules".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: InvokeBody::Json(json!({ "task": "t-nadie", "account": null })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().unwrap())
        .unwrap_err();
        assert!(error.as_str().unwrap().contains("t-nadie"), "{error}");
    }
}
