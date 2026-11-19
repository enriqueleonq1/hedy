//! Tauri commands for asking the memory and its graph (docs/decisions.md
//! §36): the "Pregunta a tu memoria" screen.

use std::path::PathBuf;

use hedy_core::application::{AskScope, MemoryAnswer, MemoryGraph};
use hedy_core::domain::ask::Source;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;
use crate::commands::{CommandResult, blocking};

/// Which memory the question is for, as the UI sends it.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AskScopeArg {
    Mine,
    Project { project: PathBuf },
    Team { team: String },
}

impl From<AskScopeArg> for AskScope {
    fn from(scope: AskScopeArg) -> Self {
        match scope {
            AskScopeArg::Mine => AskScope::Mine,
            AskScopeArg::Project { project } => AskScope::Project(project),
            AskScopeArg::Team { team } => AskScope::Team(team),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAnswerDto {
    /// `None`: nothing written answers the question.
    pub text: Option<String>,
    /// Every note sent, numbered from 1 in this order.
    pub sources: Vec<Source>,
    pub cited: Vec<usize>,
    /// The account's name; `None` when nothing was sent.
    pub account: Option<String>,
}

impl From<MemoryAnswer> for MemoryAnswerDto {
    fn from(a: MemoryAnswer) -> Self {
        Self {
            text: a.text,
            sources: a.sources,
            cited: a.cited,
            account: a.account.map(|a| a.name),
        }
    }
}

#[tauri::command]
pub async fn ask_memory(
    state: State<'_, AppState>,
    question: String,
    scope: AskScopeArg,
    account: Option<String>,
) -> CommandResult<MemoryAnswerDto> {
    blocking(&state, move |h| {
        Ok(
            h.ask_memory(&question, &scope.into(), account.as_deref(), None)?
                .into(),
        )
    })
    .await
}

#[tauri::command]
pub async fn memory_graph(state: State<'_, AppState>) -> CommandResult<MemoryGraph> {
    blocking(&state, |h| h.memory_graph()).await
}

/// The commands with the arguments `tauriApi.ts` sends, through Tauri's IPC
/// (nothing reaches an AI: no note answers).
#[cfg(test)]
mod tests {
    use hedy_core::adapters::{self, HedyPaths};
    use hedy_core::application::NewAccount;
    use hedy_core::domain::{Category, Provider};
    use serde_json::{Value, json};
    use tauri::Manager;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, noop_assets};
    use tauri::webview::InvokeRequest;

    use crate::AppState;

    #[test]
    fn ask_and_graph_over_ipc() {
        hedy_core::i18n::set_language(hedy_core::i18n::Language::Spanish);
        let tmp = tempfile::tempdir().unwrap();
        let paths = HedyPaths::at(std::path::absolute(tmp.path().join("data")).unwrap());
        let program = std::env::current_exe().unwrap();
        let hedy = adapters::system_for_app(&paths, &program).unwrap();
        let project = tmp.path().join("tienda");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        hedy.add_account(NewAccount {
            name: "Claude".into(),
            provider: Provider::Claude,
            api_key: false,
            default_folder: false,
            color: None,
        })
        .unwrap();
        hedy.use_account(&project, "claude", None).unwrap();
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                super::ask_memory,
                super::memory_graph
            ])
            .build(tauri::generate_context!(
                test = true,
                assets = noop_assets()
            ))
            .unwrap();
        app.manage(AppState {
            hedy: hedy.clone(),
            paths,
            terminals: std::sync::Arc::default(),
        });
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let call = |cmd: &str, args: Value| -> Result<Value, Value> {
            get_ipc_response(
                &webview,
                InvokeRequest {
                    cmd: cmd.into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: InvokeBody::Json(args),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
            .map(|body| body.deserialize::<Value>().unwrap())
        };

        let empty = call(
            "ask_memory",
            json!({ "question": "¿Qué fondo usamos?", "scope": { "kind": "mine" }, "account": null }),
        )
        .unwrap();
        assert_eq!(
            empty,
            json!({ "text": null, "sources": [], "cited": [], "account": null })
        );
        let error = call(
            "ask_memory",
            json!({ "question": "  ", "scope": { "kind": "project", "project": project }, "account": null }),
        )
        .unwrap_err();
        assert!(
            error.as_str().unwrap().contains("escribe una pregunta"),
            "{error}"
        );
        assert!(call(
            "ask_memory",
            json!({ "question": "¿Algo?", "scope": { "kind": "team", "team": "nadie" }, "account": null }),
        )
        .is_err());

        hedy.add_note(Some(&project), "Fondo oscuro #0F1117", Category::Convention)
            .unwrap();
        let graph = call("memory_graph", json!({})).unwrap();
        let note = graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["kind"] == "note")
            .unwrap();
        assert_eq!(note["label"], "Fondo oscuro #0F1117");
        assert_eq!(note["layer"], "mine");
        assert_eq!(note["source"]["kind"], "local");
        assert!(
            graph["links"]
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l["source"] == note["id"] || l["target"] == note["id"])
        );
    }
}
