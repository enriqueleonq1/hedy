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
        ItemKind::Suggestion => "suggestion",
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
    Team { team: String },
}

impl From<MemoryHome> for HomeDto {
    fn from(home: MemoryHome) -> Self {
        match home {
            MemoryHome::Personal => Self::Personal,
            MemoryHome::Shared(group) => Self::Shared { group },
            MemoryHome::Local => Self::Local,
            MemoryHome::Team(team) => Self::Team { team },
        }
    }
}

impl From<HomeDto> for MemoryHome {
    fn from(home: HomeDto) -> Self {
        match home {
            HomeDto::Personal => Self::Personal,
            HomeDto::Shared { group } => Self::Shared(group),
            HomeDto::Local => Self::Local,
            HomeDto::Team { team } => Self::Team(team),
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
    /// Its memory syncs now (personal, with an `origin`).
    pub syncs: bool,
}

fn home_dto(path: PathBuf, s: MemoryHomeStatus) -> MemoryHomeDto {
    MemoryHomeDto {
        path: path.to_string_lossy().into_owned(),
        home: s.home.map(HomeDto::from),
        chosen: s.chosen,
        ask: s.ask,
        group: s.group,
        syncs: s.syncs,
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

/// Removes another PC (by id): it cannot read anything written from now on.
#[tauri::command]
pub async fn sync_remove_device(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<SyncReportDto> {
    blocking(&state, move |h| h.sync_remove_device(&id).map(Into::into)).await
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

/// The app's sync end to end, through Tauri's IPC with the arguments
/// `tauriApi.ts` sends: two PCs on a real git repo, with the real keyring.
/// `cargo test -p hedy-app -- --ignored sync_ipc`; `HEDY_E2E_URL` points it
/// at a real private repo (it must be empty), else a local bare one.
#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use hedy_core::adapters::{self, HedyPaths};
    use hedy_core::application::{Hedy, NewAccount};
    use hedy_core::domain::{Category, Provider};
    use serde_json::{Value, json};
    use tauri::Manager;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, noop_assets};
    use tauri::webview::InvokeRequest;

    use crate::AppState;

    /// One PC: the app with its own data dir, and its project "tienda".
    struct Pc {
        hedy: Hedy,
        webview: tauri::WebviewWindow<MockRuntime>,
        project: PathBuf,
        _app: tauri::App<MockRuntime>,
    }

    impl Drop for Pc {
        /// Leaves the keyring as it was, whatever happened.
        fn drop(&mut self) {
            let _ = self.hedy.sync_leave();
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn pc(root: &Path, name: &str) -> Pc {
        let data = root.join(name);
        std::fs::create_dir_all(&data).unwrap();
        let paths = HedyPaths::at(std::path::absolute(&data).unwrap());
        let program = std::env::current_exe().unwrap();
        let hedy = adapters::system_for_app(&paths, &program).unwrap();
        // A project whose origin is the user's (memory: personal).
        let project = data.join("tienda");
        std::fs::create_dir_all(&project).unwrap();
        git(&project, &["init", "--quiet"]);
        git(
            &project,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/ana-ejemplo/tienda.git",
            ],
        );
        let account = hedy
            .add_account(NewAccount {
                name: "Claude".into(),
                provider: Provider::Claude,
                api_key: false,
                default_folder: false,
                color: None,
            })
            .unwrap();
        hedy.use_account(&project, account.id.as_str(), None)
            .unwrap();
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                super::sync_status,
                super::sync_default_name,
                super::sync_repo_is_public,
                super::sync_setup,
                super::sync_join,
                super::sync_now,
                super::sync_remove_device,
                super::sync_leave,
                super::sync_conflicts,
                super::resolve_sync_conflict,
                super::memory_home,
                super::memory_homes,
                super::set_memory_home,
                super::answer_owner,
                super::forget_owner,
            ])
            // The app's own config, without its assets.
            .build(tauri::generate_context!(
                test = true,
                assets = noop_assets()
            ))
            .unwrap();
        app.manage(AppState {
            hedy: hedy.clone(),
            paths,
            terminals: Arc::default(),
        });
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Pc {
            hedy,
            webview,
            project: std::fs::canonicalize(&project).unwrap(),
            _app: app,
        }
    }

    impl Pc {
        fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
            get_ipc_response(
                &self.webview,
                InvokeRequest {
                    cmd: cmd.into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    // The app's own origin (a remote one would need a capability).
                    url: "tauri://localhost".parse().unwrap(),
                    body: InvokeBody::Json(args),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
            .map(|body| body.deserialize::<Value>().unwrap())
        }

        fn ok(&self, cmd: &str, args: Value) -> Value {
            self.call(cmd, args)
                .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
        }

        fn notes(&self) -> Vec<String> {
            let mut texts: Vec<String> = self
                .hedy
                .memory(&self.project)
                .unwrap()
                .notes
                .into_iter()
                .map(|n| n.text)
                .collect();
            texts.sort();
            texts
        }
    }

    #[test]
    #[ignore = "uses git, the OS keyring and (with HEDY_E2E_URL) the network"]
    fn sync_ipc_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let url = std::env::var("HEDY_E2E_URL").unwrap_or_else(|_| {
            let bare = tmp.path().join("server.git");
            std::fs::create_dir_all(&bare).unwrap();
            git(
                &bare,
                &["init", "--quiet", "--bare", "--initial-branch=main"],
            );
            bare.to_string_lossy().into_owned()
        });
        let home = pc(tmp.path(), "casa");
        let office = pc(tmp.path(), "oficina");

        // Off, then a token in the URL is refused and the repo is private.
        assert_eq!(home.ok("sync_status", json!({})), Value::Null);
        assert!(home.ok("sync_default_name", json!({})).is_string());
        let with_token = url.replacen("https://", "https://ghp_x@", 1);
        if with_token != url {
            let error = home
                .call("sync_repo_is_public", json!({ "url": with_token }))
                .unwrap_err();
            assert!(error.as_str().unwrap().contains("token"), "{error}");
        }
        assert_eq!(
            home.ok("sync_repo_is_public", json!({ "url": url })),
            json!(false)
        );

        // Setup and join, as the dialogs do.
        let words = home.ok("sync_setup", json!({ "url": url, "name": "Casa" }));
        let words = words.as_str().unwrap().to_owned();
        assert_eq!(words.split_whitespace().count(), 12);
        let joined = office.ok(
            "sync_join",
            json!({ "url": url, "words": words, "name": "Oficina" }),
        );
        assert_eq!(joined["conflicts"], 0);
        home.ok("sync_now", json!({}));
        let status = home.ok("sync_status", json!({}));
        let names: Vec<&str> = status["devices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Casa", "Oficina"]);
        assert_eq!(status["lastError"], Value::Null);

        // The project asks about its owner; "Solo mía" makes it personal.
        let homes = home.ok("memory_homes", json!({}));
        assert_eq!(homes[0]["ask"], "github.com/ana-ejemplo");
        for pc in [&home, &office] {
            pc.ok(
                "answer_owner",
                json!({ "owner": "github.com/ana-ejemplo", "mine": true }),
            );
            let mine = pc.ok("memory_home", json!({ "path": pc.project }));
            assert_eq!(mine["home"]["kind"], "personal");
            assert_eq!(mine["syncs"], true);
        }

        // A project note and a global one travel; an edit on both PCs is a
        // conflict, settled from the app.
        let note = home
            .hedy
            .add_note(Some(&home.project), "Precios en USD", Category::Decision)
            .unwrap();
        home.hedy
            .add_note(None, "Responde en español", Category::Preference)
            .unwrap();
        home.ok("sync_now", json!({}));
        office.ok("sync_now", json!({}));
        assert_eq!(office.notes(), ["Precios en USD", "Responde en español"]);
        let theirs = office
            .hedy
            .memory(&office.project)
            .unwrap()
            .notes
            .into_iter()
            .find(|n| n.text == "Precios en USD")
            .unwrap();
        home.hedy
            .edit_note(note.id, "Precios en USD, dos decimales", Category::Decision)
            .unwrap();
        office
            .hedy
            .edit_note(
                theirs.id,
                "Precios en USD, sin decimales",
                Category::Decision,
            )
            .unwrap();
        home.ok("sync_now", json!({}));
        office.ok("sync_now", json!({}));
        home.ok("sync_now", json!({}));
        let (pc, conflicts) = [&home, &office]
            .into_iter()
            .map(|pc| (pc, pc.ok("sync_conflicts", json!({}))))
            .find(|(_, c)| !c.as_array().unwrap().is_empty())
            .expect("a conflict on one PC");
        let conflict = &conflicts[0];
        assert_eq!(conflict["kind"], "note");
        assert!(
            conflict["kept"]["text"]
                .as_str()
                .unwrap()
                .starts_with("Precios en USD")
        );
        assert!(conflict["kept"]["at"].as_i64().unwrap() > 1_600_000_000);
        let other = conflict["other"]["text"].as_str().unwrap().to_owned();
        pc.ok(
            "resolve_sync_conflict",
            json!({ "id": conflict["id"], "useOther": true }),
        );
        for _ in 0..2 {
            home.ok("sync_now", json!({}));
            office.ok("sync_now", json!({}));
        }
        for pc in [&home, &office] {
            assert!(pc.notes().contains(&other), "{:?}", pc.notes());
            assert!(
                pc.ok("sync_conflicts", json!({}))
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }

        // A project made local stops sending; following its owner again.
        office.ok(
            "set_memory_home",
            json!({ "path": office.project, "home": { "kind": "local" } }),
        );
        assert_eq!(
            office.ok("memory_home", json!({ "path": office.project }))["syncs"],
            false
        );
        office.ok(
            "set_memory_home",
            json!({ "path": office.project, "home": null }),
        );
        office.ok("forget_owner", json!({ "owner": "github.com/ana-ejemplo" }));

        // Home removes the office: it is told on its next sync.
        let office_id = office.ok("sync_status", json!({}))["devices"][0]["id"].clone();
        home.ok("sync_remove_device", json!({ "id": office_id }));
        let error = office.call("sync_now", json!({})).unwrap_err();
        assert!(error.as_str().unwrap().contains("quitó"), "{error}");
        assert_eq!(
            home.ok("sync_status", json!({}))["devices"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        // Both leave; the keyring keeps nothing.
        assert_eq!(office.ok("sync_leave", json!({})), true);
        assert_eq!(home.ok("sync_leave", json!({})), true);
        assert_eq!(home.ok("sync_leave", json!({})), false);
        assert_eq!(home.ok("sync_status", json!({})), Value::Null);
    }
}
