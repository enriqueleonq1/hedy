//! Tauri commands for personal sync (Ajustes → Sincronización, the status
//! bar, a project's memory home; docs/decisions.md §34), and the sync that
//! runs on its own while the app is open.

use std::path::PathBuf;
use std::time::Duration;

use hedy_core::application::ports::SyncConflict;
use hedy_core::application::{Hedy, MemoryHomeStatus, SyncReport, SyncStatus};
use hedy_core::domain::sync::{ItemKind, MemoryHome, Version, item_preview};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::AppState;
use crate::commands::{CommandResult, blocking};

/// The UI refreshes what the sync shows (and what it may have brought in).
pub const SYNC_UPDATED: &str = "sync-updated";

/// The sync runs this long after the app opens, then this often.
const FIRST: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(5 * 60);

/// Syncs on its own while the app is open, when the sync is set up. A
/// failure is kept by the core (`last_error`) and shown in the status bar.
pub fn start(app: AppHandle, hedy: &Hedy) {
    let hedy = hedy.clone();
    let _ = std::thread::Builder::new()
        .name("hedy-sync".into())
        .spawn(move || {
            std::thread::sleep(FIRST);
            loop {
                if hedy.sync_is_set_up().unwrap_or(false) {
                    let _ = hedy.sync_now();
                    let _ = app.emit(SYNC_UPDATED, ());
                }
                std::thread::sleep(EVERY);
            }
        });
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDeviceDto {
    pub id: String,
    pub name: String,
    pub last_sync: Option<i64>,
    pub this_pc: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusDto {
    pub url: String,
    pub last_sync: Option<i64>,
    pub last_error: Option<String>,
    pub devices: Vec<SyncDeviceDto>,
    pub conflicts: usize,
}

impl From<SyncStatus> for SyncStatusDto {
    fn from(s: SyncStatus) -> Self {
        Self {
            url: s.url,
            last_sync: s.last_sync,
            last_error: s.last_error,
            devices: s
                .devices
                .into_iter()
                .map(|d| SyncDeviceDto {
                    id: d.id.as_str().to_owned(),
                    name: d.name,
                    last_sync: d.last_sync,
                    this_pc: d.this_pc,
                })
                .collect(),
            conflicts: s.conflicts,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReportDto {
    pub sent: usize,
    pub received: usize,
    pub conflicts: usize,
}

impl From<SyncReport> for SyncReportDto {
    fn from(r: SyncReport) -> Self {
        Self {
            sent: r.sent,
            received: r.received,
            conflicts: r.conflicts,
        }
    }
}

/// One side of a conflict: what it says, which PC wrote it and when.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSideDto {
    /// `None`: deleted.
    pub text: Option<String>,
    pub device: String,
    pub at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflictDto {
    pub id: i64,
    /// `note`, `skill`, `template` or `theme`.
    pub kind: &'static str,
    /// In use on every PC.
    pub kept: SyncSideDto,
    pub other: SyncSideDto,
}

fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Note => "note",
        ItemKind::Skill => "skill",
        ItemKind::Template => "template",
        ItemKind::Theme => "theme",
    }
}

impl From<SyncConflict> for SyncConflictDto {
    fn from(c: SyncConflict) -> Self {
        let side = |v: &Version| SyncSideDto {
            text: item_preview(c.kind, v.body.as_deref()),
            device: v.stamp.device.as_str().to_owned(),
            at: v.stamp.ms / 1000,
        };
        Self {
            id: c.id,
            kind: kind_name(c.kind),
            kept: side(&c.kept),
            other: side(&c.other),
        }
    }
}

/// Where a project's memory lives, as the UI names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HomeDto {
    Personal,
    Shared { group: String },
    Local,
}

impl From<MemoryHome> for HomeDto {
    fn from(home: MemoryHome) -> Self {
        match home {
            MemoryHome::Personal => Self::Personal,
            MemoryHome::Shared(group) => Self::Shared { group },
            MemoryHome::Local => Self::Local,
        }
    }
}

impl From<HomeDto> for MemoryHome {
    fn from(home: HomeDto) -> Self {
        match home {
            HomeDto::Personal => Self::Personal,
            HomeDto::Shared { group } => Self::Shared(group),
            HomeDto::Local => Self::Local,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHomeDto {
    pub path: String,
    /// `None`: not decided; it stays on this PC.
    pub home: Option<HomeDto>,
    pub chosen: bool,
    pub ask: Option<String>,
    pub group: Option<String>,
}

fn home_dto(path: PathBuf, s: MemoryHomeStatus) -> MemoryHomeDto {
    MemoryHomeDto {
        path: path.to_string_lossy().into_owned(),
        home: s.home.map(HomeDto::from),
        chosen: s.chosen,
        ask: s.ask,
        group: s.group,
    }
}

/// `None` while the sync is not set up.
#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> CommandResult<Option<SyncStatusDto>> {
    blocking(&state, |h| Ok(h.sync_status()?.map(SyncStatusDto::from))).await
}

#[tauri::command]
pub async fn sync_default_name(state: State<'_, AppState>) -> CommandResult<String> {
    blocking(&state, |h| Ok(h.default_device_name())).await
}

/// Whether anyone can read the repo without credentials.
#[tauri::command]
pub async fn sync_repo_is_public(state: State<'_, AppState>, url: String) -> CommandResult<bool> {
    blocking(&state, move |h| h.sync_repo_is_public(url.trim())).await
}

/// Returns the 12 words, to show once.
#[tauri::command]
pub async fn sync_setup(
    state: State<'_, AppState>,
    url: String,
    name: String,
) -> CommandResult<String> {
    blocking(&state, move |h| h.sync_setup(url.trim(), &name)).await
}

#[tauri::command]
pub async fn sync_join(
    state: State<'_, AppState>,
    url: String,
    words: String,
    name: String,
) -> CommandResult<SyncReportDto> {
    blocking(&state, move |h| {
        h.sync_join(url.trim(), &words, &name).map(Into::into)
    })
    .await
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> CommandResult<SyncReportDto> {
    blocking(&state, |h| h.sync_now().map(Into::into)).await
}

/// Whether the sync was set up.
#[tauri::command]
pub async fn sync_leave(state: State<'_, AppState>) -> CommandResult<bool> {
    blocking(&state, |h| h.sync_leave()).await
}

#[tauri::command]
pub async fn sync_conflicts(state: State<'_, AppState>) -> CommandResult<Vec<SyncConflictDto>> {
    blocking(&state, |h| {
        Ok(h.sync_conflicts()?.into_iter().map(Into::into).collect())
    })
    .await
}

#[tauri::command]
pub async fn resolve_sync_conflict(
    state: State<'_, AppState>,
    id: i64,
    use_other: bool,
) -> CommandResult<()> {
    blocking(&state, move |h| h.resolve_sync_conflict(id, use_other)).await
}

#[tauri::command]
pub async fn memory_home(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CommandResult<MemoryHomeDto> {
    blocking(&state, move |h| {
        let status = h.memory_home_status(&path)?;
        Ok(home_dto(path, status))
    })
    .await
}

/// Every project's, for the sync's summary.
#[tauri::command]
pub async fn memory_homes(state: State<'_, AppState>) -> CommandResult<Vec<MemoryHomeDto>> {
    blocking(&state, |h| {
        Ok(h.memory_homes()?
            .into_iter()
            .map(|(path, status)| home_dto(path, status))
            .collect())
    })
    .await
}

/// `home: null` follows the project's owner again.
#[tauri::command]
pub async fn set_memory_home(
    state: State<'_, AppState>,
    path: PathBuf,
    home: Option<HomeDto>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.set_memory_home(&path, home.map(MemoryHome::from).as_ref())
    })
    .await
}

#[tauri::command]
pub async fn answer_owner(
    state: State<'_, AppState>,
    owner: String,
    mine: bool,
) -> CommandResult<()> {
    blocking(&state, move |h| h.answer_owner(&owner, mine)).await
}

#[tauri::command]
pub async fn forget_owner(state: State<'_, AppState>, owner: String) -> CommandResult<()> {
    blocking(&state, move |h| h.forget_owner(&owner)).await
}
