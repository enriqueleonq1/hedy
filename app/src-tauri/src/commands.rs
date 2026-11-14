//! Tauri commands: thin wrappers over `Hedy` use cases. Each one runs off the
//! main thread, because some use cases start CLI processes or wait on I/O.

use std::path::PathBuf;
use std::sync::Arc;

use hedy_core::application::{AppResult, Hedy, NewAccount, NewTask, Setting};
use hedy_core::domain::{
    AgentEvent, Category, Color, Commit, Decision, FileDiff, Note, PhasePlan, TaskId,
};
use tauri::State;
use tauri::ipc::Channel;

use crate::AppState;
use crate::dto::{
    self, MemoryDto, NewAccountInput, NewTaskInput, OverviewDto, ProjectStatusDto, TaskDto,
    UsagePointDto, UsageSummaryDto,
};
use crate::terminal::{TerminalEvent, TerminalRequest, Terminals};

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

/// Terminal I/O can block (a busy process that does not read its input), so
/// it runs off the main thread like the use cases.
async fn on_terminals<T: Send + 'static>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&Terminals) -> CommandResult<T> + Send + 'static,
) -> CommandResult<T> {
    let terminals = Arc::clone(&state.terminals);
    tauri::async_runtime::spawn_blocking(move || f(&terminals))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn terminal_write(
    state: State<'_, AppState>,
    id: u32,
    data: String,
) -> CommandResult<()> {
    on_terminals(&state, move |t| t.write(id, &data)).await
}

#[tauri::command]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    id: u32,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    on_terminals(&state, move |t| t.resize(id, cols, rows)).await
}

#[tauri::command]
pub async fn terminal_close(state: State<'_, AppState>, id: u32) -> CommandResult<()> {
    on_terminals(&state, move |t| {
        t.close(id);
        Ok(())
    })
    .await
}

// ---------------------------------------------------------------- tasks

fn task_id(id: String) -> TaskId {
    TaskId::new(id)
}

#[tauri::command]
pub async fn start_task(state: State<'_, AppState>, input: NewTaskInput) -> CommandResult<TaskDto> {
    blocking(&state, move |h| {
        let task = h.start_task(NewTask {
            project: input.project,
            prompt: input.prompt,
            mode: input.mode,
            model: input.model.filter(|m| !m.is_empty()),
            plan_first: input.plan_first,
        })?;
        Ok(TaskDto {
            live: h.is_running(&task.id),
            task,
        })
    })
    .await
}

#[tauri::command]
pub async fn tasks(state: State<'_, AppState>, project: PathBuf) -> CommandResult<Vec<TaskDto>> {
    blocking(&state, move |h| {
        Ok(h.tasks(&project)?
            .into_iter()
            .map(|task| TaskDto {
                live: h.is_running(&task.id),
                task,
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn task_events(state: State<'_, AppState>, id: String) -> CommandResult<Vec<AgentEvent>> {
    blocking(&state, move |h| h.task_events(&task_id(id))).await
}

#[tauri::command]
pub async fn answer_permission(
    state: State<'_, AppState>,
    id: String,
    request_id: String,
    decision: Decision,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.answer_permission(&task_id(id), &request_id, decision)
    })
    .await
}

#[tauri::command]
pub async fn answer_plan(
    state: State<'_, AppState>,
    id: String,
    request_id: String,
    approve: bool,
    feedback: Option<String>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.answer_plan(&task_id(id), &request_id, approve, feedback.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> CommandResult<()> {
    blocking(&state, move |h| h.send_message(&task_id(id), &text)).await
}

#[tauri::command]
pub async fn stop_task(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.stop_task(&task_id(id))).await
}

#[tauri::command]
pub async fn task_diff(state: State<'_, AppState>, id: String) -> CommandResult<Vec<FileDiff>> {
    blocking(&state, move |h| h.task_diff(&task_id(id))).await
}

#[tauri::command]
pub async fn accept_task(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.accept_task(&task_id(id)).map(drop)).await
}

#[tauri::command]
pub async fn discard_task(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.discard_task(&task_id(id)).map(drop)).await
}

#[tauri::command]
pub async fn recent_commits(
    state: State<'_, AppState>,
    project: PathBuf,
) -> CommandResult<Vec<Commit>> {
    blocking(&state, move |h| h.recent_commits(&project, 5)).await
}

#[tauri::command]
pub async fn set_project_phases(
    state: State<'_, AppState>,
    path: PathBuf,
    phases: PhasePlan,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.set_project_phases(&path, phases).map(drop)
    })
    .await
}

/// Account ids the task may be handed over to (another provider or an API key).
#[tauri::command]
pub async fn handoff_options(state: State<'_, AppState>, id: String) -> CommandResult<Vec<String>> {
    blocking(&state, move |h| {
        Ok(h.handoff_options(&task_id(id))?
            .into_iter()
            .map(|a| a.id.to_string())
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn handoff_task(
    state: State<'_, AppState>,
    id: String,
    account: String,
    model: Option<String>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.handoff_task(
            &task_id(id),
            &account,
            model.filter(|m| !m.trim().is_empty()),
        )
        .map(drop)
    })
    .await
}

#[tauri::command]
pub async fn memory(state: State<'_, AppState>, path: PathBuf) -> CommandResult<MemoryDto> {
    blocking(&state, move |h| Ok(h.memory(&path)?.into())).await
}

#[tauri::command]
pub async fn search_memory(
    state: State<'_, AppState>,
    path: PathBuf,
    query: String,
) -> CommandResult<Vec<Note>> {
    blocking(&state, move |h| h.search_memory(&path, &query)).await
}

/// A note written by the user; `project: None` makes it global.
#[tauri::command]
pub async fn add_note(
    state: State<'_, AppState>,
    project: Option<PathBuf>,
    text: String,
    category: Category,
) -> CommandResult<Note> {
    blocking(&state, move |h| {
        h.add_note(project.as_deref(), &text, category)
    })
    .await
}

#[tauri::command]
pub async fn edit_note(
    state: State<'_, AppState>,
    id: i64,
    text: String,
    category: Category,
) -> CommandResult<Note> {
    blocking(&state, move |h| h.edit_note(id, &text, category)).await
}

#[tauri::command]
pub async fn approve_note(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    blocking(&state, move |h| h.approve_note(id).map(drop)).await
}

#[tauri::command]
pub async fn forget_note(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    blocking(&state, move |h| h.forget_note(id)).await
}

#[tauri::command]
pub async fn keep_note(state: State<'_, AppState>, keep: i64, forget: i64) -> CommandResult<()> {
    blocking(&state, move |h| h.keep_note(keep, forget)).await
}

#[tauri::command]
pub async fn dismiss_contradiction(
    state: State<'_, AppState>,
    a: i64,
    b: i64,
) -> CommandResult<()> {
    blocking(&state, move |h| h.dismiss_contradiction(a, b)).await
}

#[tauri::command]
pub async fn set_memory_reader(
    state: State<'_, AppState>,
    path: PathBuf,
    account: String,
    reads: bool,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.set_memory_reader(&path, &account, reads).map(drop)
    })
    .await
}

#[tauri::command]
pub async fn project_status(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CommandResult<ProjectStatusDto> {
    blocking(&state, move |h| Ok(h.project_status(&path)?.into())).await
}

#[tauri::command]
pub async fn project_usage(
    state: State<'_, AppState>,
    path: PathBuf,
    days: u32,
) -> CommandResult<Vec<UsagePointDto>> {
    blocking(&state, move |h| {
        Ok(h.project_usage(&path, days)?
            .iter()
            .map(|(at, usage)| UsagePointDto {
                at: *at,
                spend: usage.into(),
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn usage_summary(
    state: State<'_, AppState>,
    days: u32,
) -> CommandResult<UsageSummaryDto> {
    blocking(&state, move |h| Ok(h.usage_summary(days)?.into())).await
}

#[tauri::command]
pub async fn set_project_client(
    state: State<'_, AppState>,
    path: PathBuf,
    client: Option<String>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.set_project_client(&path, client.as_deref()).map(drop)
    })
    .await
}

/// The official CLIs on this computer and their default logins (onboarding).
#[tauri::command]
pub async fn detect_clis(state: State<'_, AppState>) -> CommandResult<Vec<dto::DetectedCliDto>> {
    blocking(&state, |h| {
        Ok(h.detect_clis()?.iter().map(Into::into).collect())
    })
    .await
}

#[tauri::command]
pub async fn open_link(state: State<'_, AppState>, url: String) -> CommandResult<()> {
    blocking(&state, move |h| h.open_link(&url)).await
}
