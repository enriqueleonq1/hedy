//! Tauri commands: thin wrappers over `Hedy` use cases. Each one runs off the
//! main thread, because some use cases start CLI processes or wait on I/O.

use std::path::PathBuf;
use std::sync::Arc;

use hedy_core::application::{
    AppResult, Hedy, NewAccount, NewConversation, NewProject, NewTask, NewTemplate, Setting,
    TemplateScope,
};
use hedy_core::domain::ImageSupport;
use hedy_core::domain::{
    AgentEvent, Category, Color, Commit, Decision, FileDiff, Note, PhasePlan, QueuedMessage,
    TaskId, TemplateId,
};
use tauri::State;
use tauri::ipc::Channel;

use crate::AppState;
use crate::dto::{
    self, AcceptPreviewDto, ActivityDto, ImageInput, MemoryDto, NewAccountInput,
    NewConversationInput, NewProjectInput, NewTaskInput, NewTemplateInput, OverviewDto, PlanDto,
    ProjectStatusDto, SkillChangeDto, SkillDraftInput, SkillFilesDto, SkillsDto, TaskDto,
    TemplateDto, UsagePointDto, UsageSummaryDto,
};
use crate::terminal::{TerminalEvent, TerminalRequest, Terminals};

pub(crate) type CommandResult<T> = Result<T, String>;

/// Runs a use case off the main thread and turns its error into a message.
pub(crate) async fn blocking<T: Send + 'static>(
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
pub async fn rename_account(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> CommandResult<()> {
    blocking(&state, move |h| h.rename_account(&id, &name).map(drop)).await
}

/// Asks the provider for a new plan-usage reading (at most every few minutes
/// per account; otherwise the cached one comes back). Failures keep the last
/// reading and come back as `problem`, not as an error.
#[tauri::command]
pub async fn refresh_plan_usage(state: State<'_, AppState>, id: String) -> CommandResult<PlanDto> {
    blocking(&state, move |h| Ok(h.refresh_plan_usage(&id)?.into())).await
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
    let images = dto::decode_images(input.images)?;
    blocking(&state, move |h| {
        let request = NewTask {
            project: input.project,
            prompt: input.prompt,
            mode: input.mode,
            model: input.model.filter(|m| !m.is_empty()),
            plan_first: input.plan_first,
            images,
        };
        let task = match input.carry {
            Some(carry) => h.start_carried_task(request, carry.into())?,
            None => h.start_task(request)?,
        };
        Ok(TaskDto {
            live: h.is_running(&task.id),
            task,
        })
    })
    .await
}

/// Where "Nuevo proyecto" proposes to create it.
#[tauri::command]
pub async fn default_project_parent(state: State<'_, AppState>) -> CommandResult<Option<PathBuf>> {
    blocking(&state, |h| h.default_project_parent()).await
}

/// The folder a new project would get, or why it cannot be created there.
#[tauri::command]
pub async fn new_project_path(
    state: State<'_, AppState>,
    name: String,
    parent: PathBuf,
) -> CommandResult<PathBuf> {
    blocking(&state, move |h| h.new_project_path(&name, &parent)).await
}

/// Creates the folder with git and registers it. Returns its path.
#[tauri::command]
pub async fn new_project(
    state: State<'_, AppState>,
    input: NewProjectInput,
) -> CommandResult<PathBuf> {
    blocking(&state, move |h| {
        h.new_project(NewProject {
            name: input.name,
            parent: input.parent,
            account: input.account.filter(|a| !a.is_empty()),
            protect: input.protect,
            client: input.client,
        })
        .map(|p| p.path)
    })
    .await
}

#[tauri::command]
pub async fn start_conversation(
    state: State<'_, AppState>,
    input: NewConversationInput,
) -> CommandResult<TaskDto> {
    let images = dto::decode_images(input.images)?;
    blocking(&state, move |h| {
        let task = h.start_conversation(NewConversation {
            prompt: input.prompt,
            mode: input.mode,
            account: input.account,
            model: input.model,
            images,
        })?;
        Ok(TaskDto {
            live: h.is_running(&task.id),
            task,
        })
    })
    .await
}

/// Every conversation outside a project, newest first.
#[tauri::command]
pub async fn conversations(state: State<'_, AppState>) -> CommandResult<Vec<TaskDto>> {
    blocking(&state, |h| {
        Ok(h.conversations()?
            .into_iter()
            .map(|task| TaskDto {
                live: h.is_running(&task.id),
                task,
            })
            .collect())
    })
    .await
}

/// Accounts a conversation may use (protection rules applied).
#[tauri::command]
pub async fn conversation_accounts(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    blocking(&state, |h| {
        Ok(h.conversation_accounts()?
            .iter()
            .map(ToString::to_string)
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn delete_conversation(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.delete_conversation(&task_id(id))).await
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

/// The task board: every project's running and recent tasks.
#[tauri::command]
pub async fn activity(state: State<'_, AppState>) -> CommandResult<Vec<ActivityDto>> {
    blocking(&state, |h| {
        Ok(h.activity()?
            .into_iter()
            .map(|item| ActivityDto::new(item, h))
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

/// The messages waiting for the end of the agent's turn, oldest first.
#[tauri::command]
pub async fn queued(state: State<'_, AppState>, id: String) -> CommandResult<Vec<QueuedMessage>> {
    blocking(&state, move |h| h.queued(&task_id(id))).await
}

/// Queues a message while the agent works, or sends it now when it is idle.
#[tauri::command]
pub async fn queue_message(
    state: State<'_, AppState>,
    id: String,
    text: String,
    images: Option<Vec<ImageInput>>,
) -> CommandResult<Vec<QueuedMessage>> {
    let images = dto::decode_images(images.unwrap_or_default())?;
    blocking(&state, move |h| {
        h.queue_message_with(&task_id(id), &text, images)
    })
    .await
}

/// The project's files for `@` mentions (never `.env` or keys), sorted and capped.
#[tauri::command]
pub async fn project_files(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CommandResult<Vec<String>> {
    blocking(&state, move |h| h.project_files(&path)).await
}

/// Whether the account's agent takes images with a prompt.
#[tauri::command]
pub async fn image_support(
    state: State<'_, AppState>,
    account: String,
) -> CommandResult<ImageSupport> {
    blocking(&state, move |h| h.image_support(&account)).await
}

/// An image of a task's history as a data URL, or `None` when it is gone.
#[tauri::command]
pub async fn attachment(
    state: State<'_, AppState>,
    id: String,
    file: String,
) -> CommandResult<Option<String>> {
    blocking(&state, move |h| {
        Ok(h.attachment(&task_id(id), &file)?.map(|(kind, bytes)| {
            use base64::Engine as _;
            format!(
                "data:{};base64,{}",
                kind.media_type(),
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        }))
    })
    .await
}

#[tauri::command]
pub async fn edit_queued(
    state: State<'_, AppState>,
    message: i64,
    text: String,
) -> CommandResult<Vec<QueuedMessage>> {
    blocking(&state, move |h| h.edit_queued(message, &text)).await
}

#[tauri::command]
pub async fn remove_queued(
    state: State<'_, AppState>,
    message: i64,
) -> CommandResult<Vec<QueuedMessage>> {
    blocking(&state, move |h| h.remove_queued(message)).await
}

/// Releases a queue on hold and sends it unless the agent is mid-turn.
#[tauri::command]
pub async fn send_queued_now(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<Vec<QueuedMessage>> {
    blocking(&state, move |h| h.send_queued_now(&task_id(id))).await
}

#[tauri::command]
pub async fn stop_task(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.stop_task(&task_id(id))).await
}

#[tauri::command]
pub async fn task_diff(state: State<'_, AppState>, id: String) -> CommandResult<Vec<FileDiff>> {
    blocking(&state, move |h| h.task_diff(&task_id(id))).await
}

/// What accepting would do: shown in the review sheet before anything changes.
#[tauri::command]
pub async fn accept_preview(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<AcceptPreviewDto> {
    blocking(&state, move |h| Ok(h.accept_preview(&task_id(id))?.into())).await
}

/// "Aceptar e integrar" (or keep it on its branch for a PR), with the commit
/// message from the sheet (the proposed one when empty).
#[tauri::command]
pub async fn accept_task(
    state: State<'_, AppState>,
    id: String,
    message: Option<String>,
    mode: Option<hedy_core::domain::AcceptMode>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.accept_task_as(&task_id(id), message.as_deref(), mode.unwrap_or_default())
            .map(drop)
    })
    .await
}

/// "Deshacer": only when it is safe; otherwise the error says why.
#[tauri::command]
pub async fn undo_accept(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.undo_accept(&task_id(id)).map(drop)).await
}

/// "Esperar" on a limit card: Hedy resumes the task on the same account when
/// the limit renews.
#[tauri::command]
pub async fn wait_for_limit(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.wait_for_limit(&task_id(id)).map(drop)).await
}

#[tauri::command]
pub async fn cancel_wait(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.cancel_wait(&task_id(id)).map(drop)).await
}

/// "Continuar": the user resumes a task that waited for its limit.
#[tauri::command]
pub async fn resume_after_limit(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.resume_after_limit(&task_id(id)).map(drop)
    })
    .await
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

/// "Reemplazar esa": the proposal becomes the new text of an approved note.
#[tauri::command]
pub async fn replace_note(
    state: State<'_, AppState>,
    proposal: i64,
    target: i64,
) -> CommandResult<()> {
    blocking(&state, move |h| h.replace_note(proposal, target).map(drop)).await
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
pub async fn skills(state: State<'_, AppState>, path: PathBuf) -> CommandResult<SkillsDto> {
    blocking(&state, move |h| Ok(h.skills(&path)?.into())).await
}

/// A skill written by the user: approved right away.
#[tauri::command]
pub async fn add_skill(
    state: State<'_, AppState>,
    path: PathBuf,
    draft: SkillDraftInput,
) -> CommandResult<SkillChangeDto> {
    blocking(&state, move |h| {
        Ok(h.add_skill(&path, &draft.into())?.into())
    })
    .await
}

/// Edits a proposal (before approving it) or an approved skill.
#[tauri::command]
pub async fn edit_skill(
    state: State<'_, AppState>,
    id: i64,
    path: PathBuf,
    draft: SkillDraftInput,
) -> CommandResult<SkillChangeDto> {
    blocking(&state, move |h| {
        Ok(h.edit_skill(id, &path, &draft.into())?.into())
    })
    .await
}

#[tauri::command]
pub async fn approve_skill(state: State<'_, AppState>, id: i64) -> CommandResult<SkillChangeDto> {
    blocking(&state, move |h| Ok(h.approve_skill(id)?.into())).await
}

#[tauri::command]
pub async fn reject_skill(state: State<'_, AppState>, id: i64) -> CommandResult<()> {
    blocking(&state, move |h| h.reject_skill(id)).await
}

#[tauri::command]
pub async fn remove_skill(state: State<'_, AppState>, id: i64) -> CommandResult<SkillFilesDto> {
    blocking(&state, move |h| Ok(h.remove_skill(id)?.into())).await
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

/// The health check behind `hedy doctor`: CLIs, accounts, Hedy, environment.
#[tauri::command]
pub async fn doctor(state: State<'_, AppState>) -> CommandResult<dto::DoctorReportDto> {
    blocking(&state, |h| Ok((&h.doctor()?).into())).await
}

#[tauri::command]
pub async fn open_link(state: State<'_, AppState>, url: String) -> CommandResult<()> {
    blocking(&state, move |h| h.open_link(&url)).await
}

// ---------------------------------------------------------------- templates

fn template_id(id: &str) -> CommandResult<TemplateId> {
    id.parse()
        .map_err(|e: hedy_core::domain::DomainError| e.to_string())
}

/// What the composer of `project` offers: its templates and the global ones.
#[tauri::command]
pub async fn templates(
    state: State<'_, AppState>,
    project: Option<PathBuf>,
) -> CommandResult<Vec<TemplateDto>> {
    blocking(&state, move |h| {
        let list = h.templates(TemplateScope::For(project.as_deref()))?;
        Ok(list.into_iter().map(TemplateDto::from).collect())
    })
    .await
}

/// Every template, of every project (the management screen).
#[tauri::command]
pub async fn all_templates(state: State<'_, AppState>) -> CommandResult<Vec<TemplateDto>> {
    blocking(&state, |h| {
        let list = h.templates(TemplateScope::All)?;
        Ok(list.into_iter().map(TemplateDto::from).collect())
    })
    .await
}

#[tauri::command]
pub async fn add_template(
    state: State<'_, AppState>,
    input: NewTemplateInput,
) -> CommandResult<TemplateDto> {
    blocking(&state, move |h| {
        h.add_template(NewTemplate {
            project: input.project,
            name: input.name,
            text: input.text,
        })
        .map(TemplateDto::from)
    })
    .await
}

#[tauri::command]
pub async fn update_template(
    state: State<'_, AppState>,
    id: String,
    name: String,
    text: String,
) -> CommandResult<TemplateDto> {
    let id = template_id(&id)?;
    blocking(&state, move |h| {
        h.update_template(&id, &name, &text).map(TemplateDto::from)
    })
    .await
}

#[tauri::command]
pub async fn remove_template(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    let id = template_id(&id)?;
    blocking(&state, move |h| h.remove_template(&id)).await
}

#[tauri::command]
pub async fn restore_builtin_templates(state: State<'_, AppState>) -> CommandResult<()> {
    blocking(&state, |h| h.restore_builtin_templates()).await
}
