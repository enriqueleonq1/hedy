//! Tauri commands: thin wrappers over `Hedy` use cases. Each one runs on a
//! blocking thread, because some use cases start CLI processes.

use std::path::PathBuf;

use hedy_core::application::{AppResult, Hedy, NewAccount, Setting};
use hedy_core::domain::Color;
use tauri::State;
use tauri::ipc::Channel;

use crate::AppState;
use crate::dto::{self, NewAccountInput, OverviewDto};
use crate::terminal::{TerminalEvent, TerminalRequest};

type CommandResult<T> = Result<T, String>;

/// Runs a use case off the main thread and turns its error into a message.
async fn blocking<T: Send + 'static>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&Hedy) -> AppResult<T> + Send + 'static,
) -> CommandResult<T> {
    let hedy = state.hedy.clone();
    tauri::async_runtime::spawn_blocking(move || f(&hedy))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

fn parse_color(color: &str) -> CommandResult<Color> {
    color
        .parse()
        .map_err(|e: hedy_core::domain::DomainError| e.to_string())
}

#[tauri::command]
pub async fn overview(state: State<'_, AppState>) -> CommandResult<OverviewDto> {
    let data_dir = state.paths.data_dir().to_owned();
    blocking(&state, move |h| dto::overview(h, &data_dir)).await
}

#[tauri::command]
pub async fn add_account(
    state: State<'_, AppState>,
    input: NewAccountInput,
) -> CommandResult<String> {
    let provider = input
        .provider
        .parse()
        .map_err(|e: hedy_core::domain::DomainError| e.to_string())?;
    let color = input.color.as_deref().map(parse_color).transpose()?;
    blocking(&state, move |h| {
        let account = h.add_account(NewAccount {
            name: input.name,
            provider,
            api_key: input.api_key,
            default_folder: input.default_folder,
            color,
        })?;
        if h.setting(Setting::DefaultAccount)?.is_none() {
            h.set_setting(Setting::DefaultAccount, Some(account.id.as_str()))?;
        }
        Ok(account.id.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_api_key(state: State<'_, AppState>, id: String, key: String) -> CommandResult<()> {
    blocking(&state, move |h| h.set_api_key(&id, &key)).await
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.remove_account(&id).map(drop)).await
}

#[tauri::command]
pub async fn set_default_account(
    state: State<'_, AppState>,
    id: Option<String>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.set_setting(Setting::DefaultAccount, id.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn set_account_color(
    state: State<'_, AppState>,
    id: String,
    color: String,
) -> CommandResult<()> {
    let color = parse_color(&color)?;
    blocking(&state, move |h| h.set_account_color(&id, color).map(drop)).await
}

#[tauri::command]
pub async fn use_account(
    state: State<'_, AppState>,
    path: PathBuf,
    account: String,
    protect: Option<bool>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.use_account(&path, &account, protect).map(drop)
    })
    .await
}

/// Accounts that may be assigned to a (possibly new) project at `path`.
#[tauri::command]
pub async fn offerable_accounts(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CommandResult<Vec<String>> {
    blocking(&state, move |h| {
        Ok(h.offerable_accounts(&path)?
            .iter()
            .map(ToString::to_string)
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn set_project_color(
    state: State<'_, AppState>,
    path: PathBuf,
    color: Option<String>,
) -> CommandResult<()> {
    let color = color.as_deref().map(parse_color).transpose()?;
    blocking(&state, move |h| h.set_project_color(&path, color).map(drop)).await
}

#[tauri::command]
pub async fn remove_project(state: State<'_, AppState>, path: PathBuf) -> CommandResult<()> {
    blocking(&state, move |h| h.remove_project(&path).map(drop)).await
}

#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: Option<String>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        let setting: Setting = key.parse()?;
        let value = value.as_deref().map(str::trim).filter(|v| !v.is_empty());
        h.set_setting(setting, value)
    })
    .await
}

#[tauri::command]
pub async fn open_in_editor(state: State<'_, AppState>, path: PathBuf) -> CommandResult<()> {
    blocking(&state, move |h| h.open_in_editor(&path)).await
}

#[tauri::command]
pub async fn terminal_open(
    state: State<'_, AppState>,
    request: TerminalRequest,
    cols: u16,
    rows: u16,
    on_event: Channel<TerminalEvent>,
) -> CommandResult<u32> {
    let spec = blocking(&state, move |h| request.spec(h)).await?;
    state.terminals.open(&spec, cols, rows, on_event)
}

#[tauri::command]
pub fn terminal_write(state: State<'_, AppState>, id: u32, data: String) -> CommandResult<()> {
    state.terminals.write(id, &data)
}

#[tauri::command]
pub fn terminal_resize(
    state: State<'_, AppState>,
    id: u32,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    state.terminals.resize(id, cols, rows)
}

#[tauri::command]
pub fn terminal_close(state: State<'_, AppState>, id: u32) {
    state.terminals.close(id);
}
