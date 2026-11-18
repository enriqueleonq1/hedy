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
