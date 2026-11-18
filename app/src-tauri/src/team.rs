//! Tauri commands for team memory (Ajustes → Equipos, a project's memory,
//! the status bar; docs/decisions.md §35). The background sync of
//! `sync::start` syncs every team too.

use std::path::PathBuf;

use hedy_core::application::{
    SyncDevice, TeamInvite, TeamJoin, TeamNote, TeamPerson, TeamStatus, TeamSuggestion,
    TeamSummary, TeamWriteOutcome,
};
use hedy_core::domain::Category;
use hedy_core::domain::sync::DeviceId;
use hedy_core::domain::team::{PersonId, Role, TEAM_LAYER_TOKENS};
use serde::Serialize;
use tauri::State;

use crate::AppState;
use crate::commands::{CommandResult, blocking};
use crate::sync::{SyncDeviceDto, SyncReportDto};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSummaryDto {
    pub id: String,
    pub name: String,
    pub url: String,
    pub waiting: bool,
    pub leaving: bool,
    pub role: Option<Role>,
    pub last_sync: Option<i64>,
    pub last_error: Option<String>,
}

impl From<TeamSummary> for TeamSummaryDto {
    fn from(t: TeamSummary) -> Self {
        Self {
            id: t.id,
            name: t.name,
            url: t.url,
            waiting: t.waiting,
            leaving: t.leaving,
            role: t.role,
            last_sync: t.last_sync,
            last_error: t.last_error,
        }
    }
}

fn device_dto(d: SyncDevice) -> SyncDeviceDto {
    SyncDeviceDto {
        id: d.id.as_str().to_owned(),
        name: d.name,
        last_sync: d.last_sync,
        this_pc: d.this_pc,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamPersonDto {
    pub person: String,
    pub name: String,
    pub role: Role,
    pub devices: Vec<SyncDeviceDto>,
    pub me: bool,
}

impl From<TeamPerson> for TeamPersonDto {
    fn from(p: TeamPerson) -> Self {
        Self {
            person: p.person.0,
            name: p.name,
            role: p.role,
            devices: p.devices.into_iter().map(device_dto).collect(),
            me: p.me,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamInviteDto {
    pub number: u32,
    pub code: String,
    pub expires_at: i64,
    pub own_pc: bool,
    pub role: Role,
    pub answered: bool,
}

impl From<TeamInvite> for TeamInviteDto {
    fn from(i: TeamInvite) -> Self {
        Self {
            number: i.number,
            code: i.code,
            expires_at: i.expires_at,
            own_pc: i.own_pc,
            role: i.role,
            answered: i.answered,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamStatusDto {
    pub summary: TeamSummaryDto,
    pub device: String,
    pub person: String,
    pub people: Vec<TeamPersonDto>,
    pub suggestions: usize,
    /// The ones this person may take or turn down.
    pub to_review: usize,
    pub conflicts: usize,
    pub invites: Vec<TeamInviteDto>,
    pub failed_invites: Vec<u32>,
    /// The team layer's size now, and the most any agent gets of it.
    pub layer_tokens: usize,
    pub layer_max: usize,
}

impl From<TeamStatus> for TeamStatusDto {
    fn from(s: TeamStatus) -> Self {
        Self {
            summary: s.summary.into(),
            device: s.device.as_str().to_owned(),
            person: s.person.0,
            people: s.people.into_iter().map(Into::into).collect(),
            suggestions: s.suggestions,
            to_review: s.to_review,
            conflicts: s.conflicts,
            invites: s.invites.into_iter().map(Into::into).collect(),
            failed_invites: s.failed_invites,
            layer_tokens: s.layer_tokens,
            layer_max: TEAM_LAYER_TOKENS,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamNoteDto {
    pub uid: String,
    pub text: String,
    pub category: Category,
    pub topic: Option<String>,
    pub team_layer: bool,
    pub project: Option<PathBuf>,
    pub person: String,
    pub author: String,
    pub mine: bool,
    pub can_edit: bool,
    pub created_at: i64,
    pub updated_at: Option<i64>,
}

impl From<TeamNote> for TeamNoteDto {
    fn from(n: TeamNote) -> Self {
        Self {
            uid: n.uid,
            text: n.text,
            category: n.category,
            topic: n.topic,
            team_layer: n.team_layer,
            project: n.project,
            person: n.person.0,
            author: n.author,
            mine: n.mine,
            can_edit: n.can_edit,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSuggestionDto {
    pub uid: String,
    pub note: Option<String>,
    pub before: Option<String>,
    /// Exactly as written; `shown` has its hidden characters visible.
    pub after: Option<String>,
    pub shown: Option<String>,
    pub hidden: bool,
    pub category: Category,
    pub topic: Option<String>,
    pub team_layer: bool,
    pub project: Option<PathBuf>,
    pub person: String,
    pub author: String,
    pub mine: bool,
    pub can_review: bool,
    pub created_at: i64,
}

impl From<TeamSuggestion> for TeamSuggestionDto {
    fn from(s: TeamSuggestion) -> Self {
        Self {
            uid: s.uid,
            note: s.note,
            before: s.before,
            after: s.after,
            shown: s.shown,
            hidden: s.hidden,
            category: s.category,
            topic: s.topic,
            team_layer: s.team_layer,
            project: s.project,
            person: s.person.0,
            author: s.author,
            mine: s.mine,
            can_review: s.can_review,
            created_at: s.created_at,
        }
    }
}

/// What a team write became: the note, published, or a suggestion.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamWriteDto {
    pub published: bool,
    pub uid: String,
}

impl From<TeamWriteOutcome> for TeamWriteDto {
    fn from(o: TeamWriteOutcome) -> Self {
        match o {
            TeamWriteOutcome::Published(uid) => Self {
                published: true,
                uid,
            },
            TeamWriteOutcome::Suggested(uid) => Self {
                published: false,
                uid,
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamJoinDto {
    pub id: String,
    pub joined: bool,
}

impl From<TeamJoin> for TeamJoinDto {
    fn from(j: TeamJoin) -> Self {
        Self {
            id: j.id,
            joined: j.joined,
        }
    }
}

/// A new team: its id and its 12 words, to show once.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamCreatedDto {
    pub id: String,
    pub words: String,
}

#[tauri::command]
pub async fn teams(state: State<'_, AppState>) -> CommandResult<Vec<TeamSummaryDto>> {
    blocking(&state, |h| {
        Ok(h.teams()?.into_iter().map(Into::into).collect())
    })
    .await
}

#[tauri::command]
pub async fn team_status(state: State<'_, AppState>, team: String) -> CommandResult<TeamStatusDto> {
    blocking(&state, move |h| h.team_status(&team).map(Into::into)).await
}

#[tauri::command]
pub async fn team_create(
    state: State<'_, AppState>,
    url: String,
    name: String,
    me: String,
    pc: String,
) -> CommandResult<TeamCreatedDto> {
    blocking(&state, move |h| {
        let (id, words) = h.team_create(url.trim(), &name, &me, &pc)?;
        Ok(TeamCreatedDto { id, words })
    })
    .await
}

/// A new person (`admin` for an admin), or another PC of this person
/// (`mine`).
#[tauri::command]
pub async fn team_invite(
    state: State<'_, AppState>,
    team: String,
    admin: bool,
    mine: bool,
) -> CommandResult<TeamInviteDto> {
    blocking(&state, move |h| {
        let invite = if mine {
            h.team_invite_pc(&team)?
        } else {
            let role = if admin { Role::Admin } else { Role::Member };
            h.team_invite(&team, role)?
        };
        Ok(invite.into())
    })
    .await
}

#[tauri::command]
pub async fn team_cancel_invite(
    state: State<'_, AppState>,
    team: String,
    number: u32,
) -> CommandResult<()> {
    blocking(&state, move |h| h.team_cancel_invite(&team, number)).await
}

#[tauri::command]
pub async fn team_dismiss_failed_invite(
    state: State<'_, AppState>,
    team: String,
    number: u32,
) -> CommandResult<()> {
    blocking(&state, move |h| h.team_dismiss_failed_invite(&team, number)).await
}

#[tauri::command]
pub async fn team_join(
    state: State<'_, AppState>,
    url: String,
    code: String,
    me: String,
    pc: String,
) -> CommandResult<TeamJoinDto> {
    blocking(&state, move |h| {
        h.team_join(url.trim(), &code, &me, &pc).map(Into::into)
    })
    .await
}

#[tauri::command]
pub async fn team_sync(state: State<'_, AppState>, team: String) -> CommandResult<SyncReportDto> {
    blocking(&state, move |h| h.team_sync(&team).map(Into::into)).await
}

#[tauri::command]
pub async fn team_link_project(
    state: State<'_, AppState>,
    team: String,
    path: PathBuf,
) -> CommandResult<()> {
    blocking(&state, move |h| h.team_link_project(&team, &path)).await
}

#[tauri::command]
pub async fn team_notes(
    state: State<'_, AppState>,
    team: String,
) -> CommandResult<Vec<TeamNoteDto>> {
    blocking(&state, move |h| {
        Ok(h.team_notes(&team)?.into_iter().map(Into::into).collect())
    })
    .await
}

/// To a project's notes (`project`), or to the team layer (`None`).
#[tauri::command]
pub async fn team_add_note(
    state: State<'_, AppState>,
    team: String,
    project: Option<PathBuf>,
    text: String,
    category: Category,
    topic: Option<String>,
) -> CommandResult<TeamWriteDto> {
    blocking(&state, move |h| {
        h.team_add_note(&team, project.as_deref(), &text, category, topic.as_deref())
            .map(Into::into)
    })
    .await
}

/// `text: None` removes the note (or suggests removing it).
#[tauri::command]
pub async fn team_edit_note(
    state: State<'_, AppState>,
    team: String,
    uid: String,
    text: Option<String>,
    category: Option<Category>,
) -> CommandResult<TeamWriteDto> {
    blocking(&state, move |h| {
        h.team_edit_note(&team, &uid, text.as_deref(), category, None)
            .map(Into::into)
    })
    .await
}

/// Moves a note of this PC (an agent's proposal, the user's own) to the
/// team: its project's notes, or the team layer (`everyone`).
#[tauri::command]
pub async fn team_publish_note(
    state: State<'_, AppState>,
    team: String,
    note_id: i64,
    everyone: bool,
) -> CommandResult<TeamWriteDto> {
    blocking(&state, move |h| {
        h.team_publish_note(&team, note_id, everyone)
            .map(Into::into)
    })
    .await
}

#[tauri::command]
pub async fn team_suggestions(
    state: State<'_, AppState>,
    team: String,
) -> CommandResult<Vec<TeamSuggestionDto>> {
    blocking(&state, move |h| {
        Ok(h.team_suggestions(&team)?
            .into_iter()
            .map(Into::into)
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn team_accept_suggestion(
    state: State<'_, AppState>,
    team: String,
    uid: String,
) -> CommandResult<()> {
    blocking(&state, move |h| h.team_accept_suggestion(&team, &uid)).await
}

#[tauri::command]
pub async fn team_reject_suggestion(
    state: State<'_, AppState>,
    team: String,
    uid: String,
) -> CommandResult<()> {
    blocking(&state, move |h| h.team_reject_suggestion(&team, &uid)).await
}

#[tauri::command]
pub async fn team_set_role(
    state: State<'_, AppState>,
    team: String,
    person: String,
    admin: bool,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        let role = if admin { Role::Admin } else { Role::Member };
        h.team_set_role(&team, &PersonId(person), role)
    })
    .await
}

#[tauri::command]
pub async fn team_remove_person(
    state: State<'_, AppState>,
    team: String,
    person: String,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.team_remove_person(&team, &PersonId(person))
    })
    .await
}

#[tauri::command]
pub async fn team_remove_pc(
    state: State<'_, AppState>,
    team: String,
    device: String,
) -> CommandResult<()> {
    blocking(&state, move |h| h.team_remove_pc(&team, &DeviceId(device))).await
}

#[tauri::command]
pub async fn team_rename(
    state: State<'_, AppState>,
    team: String,
    name: String,
) -> CommandResult<()> {
    blocking(&state, move |h| h.team_rename(&team, &name)).await
}

#[tauri::command]
pub async fn team_rename_person(
    state: State<'_, AppState>,
    team: String,
    person: String,
    name: String,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.team_rename_person(&team, &PersonId(person), &name)
    })
    .await
}

/// Asks to be taken out (an admin's PC waits for another admin's), or
/// (`here`) only forgets the team on this PC.
#[tauri::command]
pub async fn team_leave(state: State<'_, AppState>, team: String, here: bool) -> CommandResult<()> {
    blocking(&state, move |h| {
        if here {
            h.team_forget(&team)
        } else {
            h.team_leave(&team)
        }
    })
    .await
}

/// The app's team memory end to end, through Tauri's IPC with the arguments
/// `tauriApi.ts` sends: three people on a real git repo, with the real
/// keyring. `cargo test -p hedy-app -- --ignored team_ipc`; `HEDY_E2E_URL`
/// points it at a real private repo (it must be empty), else a local bare
/// one.
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

    /// One person's PC: the app with its own data dir, and the project "app".
    struct Pc {
        hedy: Hedy,
        webview: tauri::WebviewWindow<MockRuntime>,
        project: PathBuf,
        _app: tauri::App<MockRuntime>,
    }

    impl Drop for Pc {
        /// Leaves the keyring as it was, whatever happened.
        fn drop(&mut self) {
            for team in self.hedy.teams().unwrap_or_default() {
                let _ = self.hedy.team_forget(&team.id);
            }
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
        let project = data.join("app");
        std::fs::create_dir_all(&project).unwrap();
        git(&project, &["init", "--quiet"]);
        git(
            &project,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/empresa-ejemplo/app.git",
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
                super::teams,
                super::team_status,
                super::team_create,
                super::team_invite,
                super::team_join,
                super::team_sync,
                super::team_link_project,
                super::team_notes,
                super::team_add_note,
                super::team_edit_note,
                super::team_publish_note,
                super::team_suggestions,
                super::team_accept_suggestion,
                super::team_set_role,
                super::team_remove_person,
                super::team_leave,
            ])
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

        fn team(&self) -> String {
            self.ok("teams", json!({}))[0]["id"]
                .as_str()
                .unwrap()
                .to_owned()
        }

        fn sync(&self) -> Value {
            self.ok("team_sync", json!({ "team": self.team() }))
        }

        fn notes(&self) -> Vec<String> {
            let mut texts: Vec<String> = self
                .ok("team_notes", json!({ "team": self.team() }))
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["text"].as_str().unwrap().to_owned())
                .collect();
            texts.sort();
            texts
        }

        /// Joins with an invite another PC made, as the dialog does.
        fn join(&self, url: &str, code: &str, me: &str) -> Value {
            self.ok(
                "team_join",
                json!({ "url": url, "code": code, "me": me, "pc": format!("PC de {me}") }),
            )
        }
    }

    #[test]
    #[ignore = "uses git, the OS keyring and (with HEDY_E2E_URL) the network"]
    fn team_ipc_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let url = std::env::var("HEDY_E2E_URL").unwrap_or_else(|_| {
            let bare = tmp.path().join("team.git");
            std::fs::create_dir_all(&bare).unwrap();
            git(
                &bare,
                &["init", "--quiet", "--bare", "--initial-branch=main"],
            );
            bare.to_string_lossy().into_owned()
        });
        let ana = pc(tmp.path(), "ana");
        let luis = pc(tmp.path(), "luis");
        let marta = pc(tmp.path(), "marta");

        // Ana creates the team (the 12 words come back once).
        assert_eq!(ana.ok("teams", json!({})), json!([]));
        let created = ana.ok(
            "team_create",
            json!({ "url": url, "name": "Plataforma", "me": "Ana", "pc": "Portátil" }),
        );
        assert_eq!(
            created["words"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .count(),
            12
        );
        let team = ana.team();
        assert_eq!(ana.ok("teams", json!({}))[0]["role"], "admin");

        // Luis (member) and Marta (admin) join with invites.
        let code = ana.ok(
            "team_invite",
            json!({ "team": team, "admin": false, "mine": false }),
        );
        let joined = luis.join(&url, code["code"].as_str().unwrap(), "Luis");
        assert_eq!(joined["joined"], false);
        assert_eq!(luis.ok("teams", json!({}))[0]["waiting"], true);
        let code = ana.ok(
            "team_invite",
            json!({ "team": team, "admin": true, "mine": false }),
        );
        marta.join(&url, code["code"].as_str().unwrap(), "Marta");
        ana.sync();
        luis.sync();
        marta.sync();
        let status = ana.ok("team_status", json!({ "team": team }));
        let people: Vec<(&str, &str)> = status["people"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["name"].as_str().unwrap(), p["role"].as_str().unwrap()))
            .collect();
        assert_eq!(
            people,
            [("Ana", "admin"), ("Marta", "admin"), ("Luis", "member")]
        );
        assert_eq!(status["layerMax"], 1500);

        // Projects joined; notes in both layers, and a member's suggestion.
        for pc in [&ana, &luis, &marta] {
            pc.ok(
                "team_link_project",
                json!({ "team": pc.team(), "path": pc.project }),
            );
        }
        let write = ana.ok(
            "team_add_note",
            json!({ "team": team, "project": null, "text": "Precios en USD", "category": "rule", "topic": null }),
        );
        assert_eq!(write["published"], true);
        let write = luis.ok(
            "team_add_note",
            json!({ "team": luis.team(), "project": null, "text": "Todo en inglés", "category": "convention", "topic": null }),
        );
        assert_eq!(write["published"], false);
        // An agent's proposal Luis shares with the project ("¿A quién le sirve?").
        let proposal = luis
            .hedy
            .propose_note(
                &luis.project,
                "Los tests van en tests/",
                Category::Convention,
                "Claude",
                None,
            )
            .unwrap();
        let write = luis.ok(
            "team_publish_note",
            json!({ "team": luis.team(), "noteId": proposal.id, "everyone": false }),
        );
        assert_eq!(write["published"], true);
        luis.sync();
        marta.sync();

        // Marta (admin) takes Luis's suggestion; everyone ends up equal.
        let suggestions = marta.ok("team_suggestions", json!({ "team": marta.team() }));
        let suggestion = &suggestions[0];
        assert_eq!(
            (
                suggestion["author"].as_str(),
                suggestion["canReview"].as_bool()
            ),
            (Some("Luis"), Some(true))
        );
        marta.ok(
            "team_accept_suggestion",
            json!({ "team": marta.team(), "uid": suggestion["uid"] }),
        );
        marta.sync();
        for pc in [&ana, &luis, &marta] {
            pc.sync();
        }
        for pc in [&ana, &luis, &marta] {
            assert_eq!(
                pc.notes(),
                [
                    "Los tests van en tests/",
                    "Precios en USD",
                    "Todo en inglés"
                ]
            );
        }
        // Luis may not change Ana's note: his edit is a suggestion.
        let notes = luis.ok("team_notes", json!({ "team": luis.team() }));
        let anas = notes
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["author"] == "Ana")
            .unwrap();
        assert_eq!(anas["canEdit"], false);
        let write = luis.ok(
            "team_edit_note",
            json!({ "team": luis.team(), "uid": anas["uid"], "text": "Precios en EUR" }),
        );
        assert_eq!(write["published"], false);

        // Roles and removal.
        let person = |name: &str| -> String {
            ana.ok("team_status", json!({ "team": team }))["people"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == name)
                .unwrap()["person"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let error = luis
            .call(
                "team_remove_person",
                json!({ "team": luis.team(), "person": person("Ana") }),
            )
            .unwrap_err();
        assert!(error.as_str().unwrap().contains("rol"), "{error}");
        ana.ok(
            "team_remove_person",
            json!({ "team": team, "person": person("Luis") }),
        );
        let error = luis
            .call("team_sync", json!({ "team": luis.team() }))
            .unwrap_err();
        assert!(
            error.as_str().unwrap().contains("ya no está en el equipo"),
            "{error}"
        );
        luis.ok("team_leave", json!({ "team": luis.team(), "here": false }));
        assert_eq!(luis.ok("teams", json!({})), json!([]));
        marta.sync();
        assert_eq!(
            marta.ok("team_status", json!({ "team": marta.team() }))["people"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        // His notes stay, under his name.
        assert!(marta.notes().contains(&"Todo en inglés".to_owned()));

        // The last admin cannot step down; with Marta still admin, Ana may.
        ana.ok(
            "team_set_role",
            json!({ "team": team, "person": person("Ana"), "admin": false }),
        );
        let error = marta
            .call(
                "team_set_role",
                json!({ "team": marta.team(), "person": person("Marta"), "admin": false }),
            )
            .unwrap_err();
        assert!(error.as_str().unwrap().contains("sin admin"), "{error}");
    }
}
