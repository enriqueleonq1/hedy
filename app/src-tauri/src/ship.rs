//! Tauri commands for shipping: the folder's git status, push and pull,
//! and publishing an accepted task (push, pull request, AI commit message).
//! Push and pull failures come back as data (`failure`), so the UI can offer
//! the next step (for example "Pull" after a rejected push).

use std::path::PathBuf;

use hedy_core::application::{
    AppError, FolderStatus, Hedy, NewPullRequest, PullRequestDraft, Pulled, Pushed, ShipError,
    ShipState,
};
use hedy_core::domain::ship::{PullProblem, PushProblem};
use hedy_core::domain::{GhState, PullRequest, TaskId};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

type CommandResult<T> = Result<T, String>;

async fn run<T: Send + 'static>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&Hedy) -> CommandResult<T> + Send + 'static,
) -> CommandResult<T> {
    let hedy = state.hedy.clone();
    tauri::async_runtime::spawn_blocking(move || f(&hedy))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderStatusDto {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub changes: u32,
    pub remote: Option<String>,
    pub remote_host: Option<String>,
    pub gh: Option<GhState>,
}

impl From<FolderStatus> for FolderStatusDto {
    fn from(s: FolderStatus) -> Self {
        Self {
            branch: s.git.branch,
            upstream: s.git.upstream,
            ahead: s.git.ahead,
            behind: s.git.behind,
            staged: s.git.staged,
            unstaged: s.git.unstaged,
            untracked: s.git.untracked,
            conflicted: s.git.conflicted,
            changes: s.git.changed,
            remote: s.remote,
            remote_host: s.remote_host,
            gh: s.gh,
        }
    }
}

/// Why a push or pull failed, for the UI to offer the next step.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureDto {
    /// auth, rejected, protected, offline, diverged, noUpstream,
    /// localChanges, noRemote, detached or other.
    pub kind: &'static str,
    /// Already in the user's language.
    pub message: String,
}

fn failure(error: AppError) -> CommandResult<FailureDto> {
    let kind = match &error {
        AppError::Ship(ship) => match ship {
            ShipError::Push { problem, .. } => match problem {
                PushProblem::Auth => "auth",
                PushProblem::Rejected => "rejected",
                PushProblem::Protected => "protected",
                PushProblem::Offline => "offline",
                PushProblem::Other => "other",
            },
            ShipError::Pull { problem, .. } => match problem {
                PullProblem::Diverged => "diverged",
                PullProblem::NoUpstream => "noUpstream",
                PullProblem::LocalChanges => "localChanges",
                PullProblem::Auth => "auth",
                PullProblem::Offline => "offline",
                PullProblem::Other => "other",
            },
            ShipError::NoRemote => "noRemote",
            ShipError::Detached => "detached",
            _ => return Err(error.to_string()),
        },
        _ => return Err(error.to_string()),
    };
    Ok(FailureDto {
        kind,
        message: error.to_string(),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushOutcomeDto {
    pub pushed: Option<PushedDto>,
    pub failure: Option<FailureDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushedDto {
    pub branch: String,
    pub upstream: String,
    pub upstream_set: bool,
}

impl From<Pushed> for PushedDto {
    fn from(p: Pushed) -> Self {
        Self {
            branch: p.branch,
            upstream: p.upstream,
            upstream_set: p.upstream_set,
        }
    }
}

fn push_outcome(result: Result<Pushed, AppError>) -> CommandResult<PushOutcomeDto> {
    Ok(match result {
        Ok(pushed) => PushOutcomeDto {
            pushed: Some(pushed.into()),
            failure: None,
        },
        Err(error) => PushOutcomeDto {
            pushed: None,
            failure: Some(failure(error)?),
        },
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullOutcomeDto {
    /// `Some(false)`: already up to date.
    pub updated: Option<bool>,
    pub failure: Option<FailureDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipStateDto {
    pub branch: String,
    pub kept: bool,
    pub remote: Option<String>,
    pub remote_host: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub pushed: bool,
    pub gh: Option<GhState>,
    pub default_branch: Option<String>,
    pub pull_request: Option<PullRequest>,
    pub can_open_pr: bool,
}

impl From<ShipState> for ShipStateDto {
    fn from(s: ShipState) -> Self {
        Self {
            branch: s.branch,
            kept: s.kept,
            remote: s.remote,
            remote_host: s.remote_host,
            upstream: s.upstream,
            ahead: s.ahead,
            behind: s.behind,
            pushed: s.pushed,
            gh: s.gh,
            default_branch: s.default_branch,
            pull_request: s.pull_request,
            can_open_pr: s.can_open_pr,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDraftDto {
    pub title: String,
    pub body: String,
    pub base: String,
    pub head: String,
}

impl From<PullRequestDraft> for PullRequestDraftDto {
    fn from(d: PullRequestDraft) -> Self {
        Self {
            title: d.title,
            body: d.body,
            base: d.base,
            head: d.head,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPullRequestInput {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub draft: bool,
}

fn id(id: String) -> TaskId {
    TaskId::new(id)
}

/// Branch, changes, ahead/behind, remote host and `gh` of a folder.
#[tauri::command]
pub async fn folder_status(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CommandResult<FolderStatusDto> {
    run(&state, move |h| {
        h.folder_status(&path)
            .map(Into::into)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn push_folder(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CommandResult<PushOutcomeDto> {
    run(&state, move |h| push_outcome(h.push_folder(&path))).await
}

/// `git pull --ff-only`; a divergence comes back as a failure, never merged.
#[tauri::command]
pub async fn pull_folder(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CommandResult<PullOutcomeDto> {
    run(&state, move |h| {
        Ok(match h.pull_folder(&path) {
            Ok(Pulled { updated }) => PullOutcomeDto {
                updated: Some(updated),
                failure: None,
            },
            Err(error) => PullOutcomeDto {
                updated: None,
                failure: Some(failure(error)?),
            },
        })
    })
    .await
}

#[tauri::command]
pub async fn ship_state(state: State<'_, AppState>, task: String) -> CommandResult<ShipStateDto> {
    run(&state, move |h| {
        h.ship_state(&id(task))
            .map(Into::into)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn push_task(state: State<'_, AppState>, task: String) -> CommandResult<PushOutcomeDto> {
    run(&state, move |h| push_outcome(h.push_task(&id(task)))).await
}

#[tauri::command]
pub async fn pull_request_draft(
    state: State<'_, AppState>,
    task: String,
) -> CommandResult<PullRequestDraftDto> {
    run(&state, move |h| {
        h.pull_request_draft(&id(task))
            .map(Into::into)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn create_pull_request(
    state: State<'_, AppState>,
    task: String,
    input: NewPullRequestInput,
) -> CommandResult<PullRequest> {
    run(&state, move |h| {
        h.create_pull_request(
            &id(task),
            NewPullRequest {
                title: input.title,
                body: input.body,
                base: input.base,
                draft: input.draft,
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
}

/// "Sugerir con IA": the task's own account and model (spends a few tokens).
#[tauri::command]
pub async fn suggest_commit_message(
    state: State<'_, AppState>,
    task: String,
) -> CommandResult<String> {
    run(&state, move |h| {
        h.suggest_commit_message(&id(task))
            .map_err(|e| e.to_string())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_failures_become_data_and_other_errors_stay_errors() {
        let error = AppError::Ship(ShipError::Push {
            problem: PushProblem::Rejected,
            branch: "main".into(),
            host: None,
            detail: String::new(),
        });
        let out = push_outcome(Err(error)).unwrap();
        assert_eq!(out.failure.unwrap().kind, "rejected");
        assert!(push_outcome(Err(AppError::Ship(ShipError::NotAccepted))).is_err());
        assert!(push_outcome(Err(AppError::EmptyNote)).is_err());
    }
}
