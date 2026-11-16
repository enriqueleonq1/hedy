//! Shipping: the main folder's git state (branch, changes, ahead/behind),
//! pushing and fast-forward pulls with the user's own git credentials, pull
//! requests through GitHub CLI, and a commit message suggested by the task's
//! own agent. Everything stays local except what the user asks to publish to
//! their own remote. Hedy never forces a push and never stores credentials.

use std::path::Path;
use std::time::Duration;

use super::one_shot::{OneShot, OneShotFailure};
use super::ports::{PullRequestSpec, Upstream};
use super::tasks::current_stage;
use super::{AppError, AppResult, Hedy};
use crate::domain::ship::{
    GitFolder, PullProblem, PullRequestFacts, PushProblem, clean_suggestion, commit_message_prompt,
    commit_subject, diff_for_prompt, pick_remote, pull_request_body, remote_host,
};
use crate::domain::{
    Account, AgentEvent, GhState, LimitLevel, PullRequest, Shipment, Task, TaskId, TaskStatus,
    policy,
};
use crate::tr;

/// How long "Sugerir con IA" waits for the agent's answer.
const SUGGESTION_TIMEOUT: Duration = Duration::from_secs(120);
/// The diff sent to the agent is cut here (characters).
const SUGGESTION_DIFF_MAX: usize = 12_000;

/// The main folder's state for its header: branch, changes, ahead/behind,
/// the remote's host (never its URL: it may hold a token) and whether
/// GitHub CLI can open pull requests there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderStatus {
    pub git: GitFolder,
    /// The remote it would publish to.
    pub remote: Option<String>,
    pub remote_host: Option<String>,
    /// `None` when there is no remote to open pull requests on.
    pub gh: Option<GhState>,
}

/// What a push did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pushed {
    pub branch: String,
    /// "origin/main".
    pub upstream: String,
    /// The branch had no upstream: the push set it (`-u`).
    pub upstream_set: bool,
}

/// What a fast-forward pull did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulled {
    /// `false` when it was already up to date.
    pub updated: bool,
}

/// Where an accepted task stands on the way to its remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShipState {
    pub branch: String,
    /// Kept on the task's own branch instead of merged.
    pub kept: bool,
    pub remote: Option<String>,
    pub remote_host: Option<String>,
    /// "origin/main"; `None`: a push would set it.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The upstream has every commit of the branch.
    pub pushed: bool,
    pub gh: Option<GhState>,
    /// The repository's default branch, the pull request's usual base.
    pub default_branch: Option<String>,
    pub pull_request: Option<PullRequest>,
    /// A pull request makes sense: `gh` is ready and the branch is not the
    /// default one.
    pub can_open_pr: bool,
}

/// A pull request proposed from what Hedy knows locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestDraft {
    pub title: String,
    pub body: String,
    pub base: String,
    pub head: String,
}

/// What the user confirmed. Empty fields take the draft's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewPullRequest {
    pub title: String,
    pub body: String,
    pub base: String,
    pub draft: bool,
}

/// Why shipping failed. Messages say what to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShipError {
    /// Not in a git branch (detached HEAD).
    Detached,
    /// No remote to publish to (none, or several without an `origin`).
    NoRemote,
    Push {
        problem: PushProblem,
        branch: String,
        host: Option<String>,
        detail: String,
    },
    Pull {
        problem: PullProblem,
        branch: String,
        upstream: Option<String>,
        detail: String,
    },
    /// The task was not accepted (nothing to publish yet).
    NotAccepted,
    /// The branch has commits its upstream lacks.
    NotPushed {
        branch: String,
    },
    GhMissing,
    GhLoggedOut {
        host: String,
    },
    /// `gh pr create` failed: its own message.
    PullRequest(String),
    /// Rule 3: the task's account is at its limit; no other account is used.
    AccountAtLimit {
        account: String,
    },
    SuggestionFailed(Option<String>),
    SuggestionTimedOut,
    /// Reopening a task kept on its branch: the branch no longer exists.
    BranchGone {
        branch: String,
    },
}

fn last_lines(detail: &str) -> String {
    let lines: Vec<&str> = detail
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines[lines.len().saturating_sub(3)..].join(" ")
}

impl ShipError {
    pub(super) fn message(&self) -> String {
        match self {
            Self::Detached => tr!(
                "tu carpeta no está en una rama (HEAD separado): cambia a una rama para publicar",
                "your folder is not on a branch (detached HEAD): switch to a branch to publish"
            ),
            Self::NoRemote => tr!(
                "esta carpeta no tiene un remoto al que publicar: añade uno con `git remote add origin <url>`",
                "this folder has no remote to publish to: add one with `git remote add origin <url>`"
            ),
            Self::Push {
                problem,
                branch,
                host,
                detail,
            } => {
                let host = host
                    .clone()
                    .unwrap_or_else(|| tr!("el remoto", "the remote"));
                match problem {
                    PushProblem::Auth => tr!(
                        "{host} no aceptó tus credenciales de git. Comprueba tu acceso (por ejemplo `gh auth setup-git` o tu clave SSH) y vuelve a intentarlo",
                        "{host} did not accept your git credentials. Check your access (for example `gh auth setup-git` or your SSH key) and try again"
                    ),
                    PushProblem::Rejected => tr!(
                        "el remoto tiene commits que «{branch}» no tiene. Trae los cambios (Pull) y vuelve a publicar; Hedy nunca fuerza un push",
                        "the remote has commits «{branch}» does not have. Pull them and publish again; Hedy never force-pushes"
                    ),
                    PushProblem::Protected => tr!(
                        "{host} protege la rama «{branch}»: publica en otra rama y abre un PR",
                        "{host} protects branch «{branch}»: publish another branch and open a PR"
                    ),
                    PushProblem::Offline => tr!(
                        "no se pudo conectar con {host}. Revisa tu conexión y vuelve a intentarlo",
                        "could not reach {host}. Check your connection and try again"
                    ),
                    PushProblem::Other => {
                        let detail = last_lines(detail);
                        tr!(
                            "git no pudo publicar «{branch}»: {detail}",
                            "git could not publish «{branch}»: {detail}"
                        )
                    }
                }
            }
            Self::Pull {
                problem,
                branch,
                upstream,
                detail,
            } => {
                let upstream = upstream
                    .clone()
                    .unwrap_or_else(|| tr!("el remoto", "the remote"));
                match problem {
                    PullProblem::Diverged => tr!(
                        "«{branch}» y {upstream} tienen commits distintos: no se puede avanzar sin fusionar. Resuélvelo en VS Code (o con git)",
                        "«{branch}» and {upstream} have different commits: it cannot fast-forward. Sort it out in VS Code (or with git)"
                    ),
                    PullProblem::NoUpstream => tr!(
                        "la rama «{branch}» no sigue ninguna rama remota: publícala primero",
                        "branch «{branch}» tracks no remote branch: publish it first"
                    ),
                    PullProblem::LocalChanges => tr!(
                        "tienes cambios sin guardar en archivos que el pull cambiaría: haz commit o guárdalos aparte y vuelve a intentarlo",
                        "you have uncommitted changes in files the pull would change: commit or stash them and try again"
                    ),
                    PullProblem::Auth => tr!(
                        "el remoto no aceptó tus credenciales de git. Comprueba tu acceso y vuelve a intentarlo",
                        "the remote did not accept your git credentials. Check your access and try again"
                    ),
                    PullProblem::Offline => tr!(
                        "no se pudo conectar con el remoto. Revisa tu conexión y vuelve a intentarlo",
                        "could not reach the remote. Check your connection and try again"
                    ),
                    PullProblem::Other => {
                        let detail = last_lines(detail);
                        tr!(
                            "git no pudo traer los cambios: {detail}",
                            "git could not pull: {detail}"
                        )
                    }
                }
            }
            Self::NotAccepted => tr!(
                "acepta la tarea antes de publicarla",
                "accept the task before publishing it"
            ),
            Self::NotPushed { branch } => tr!(
                "publica «{branch}» antes de crear el PR",
                "publish «{branch}» before creating the PR"
            ),
            Self::GhMissing => tr!(
                "para crear PRs desde Hedy instala GitHub CLI (`gh`)",
                "to create PRs from Hedy, install GitHub CLI (`gh`)"
            ),
            Self::GhLoggedOut { host } => tr!(
                "GitHub CLI no tiene sesión en {host}: ejecuta `gh auth login` y vuelve a intentarlo",
                "GitHub CLI is not logged in to {host}: run `gh auth login` and try again"
            ),
            Self::PullRequest(detail) => {
                let detail = last_lines(detail);
                tr!(
                    "GitHub CLI no pudo crear el PR: {detail}",
                    "GitHub CLI could not create the PR: {detail}"
                )
            }
            Self::AccountAtLimit { account } => tr!(
                "la cuenta «{account}» está en su límite. Hedy no usa otra cuenta: escribe el mensaje tú o espera a que se renueve",
                "account «{account}» is at its limit. Hedy does not use another account: write the message yourself or wait for it to renew"
            ),
            Self::SuggestionFailed(detail) => match detail {
                Some(detail) => tr!(
                    "no se pudo sugerir un mensaje: {detail}",
                    "could not suggest a message: {detail}"
                ),
                None => tr!(
                    "no se pudo sugerir un mensaje; escríbelo tú",
                    "could not suggest a message; write it yourself"
                ),
            },
            Self::SuggestionTimedOut => tr!(
                "la IA tardó demasiado en sugerir un mensaje; escríbelo tú o vuelve a intentarlo",
                "the AI took too long to suggest a message; write it yourself or try again"
            ),
            Self::BranchGone { branch } => tr!(
                "la rama «{branch}» ya no existe en tu carpeta, así que no se puede reabrir la tarea",
                "branch «{branch}» no longer exists in your folder, so the task cannot be reopened"
            ),
        }
    }
}

impl From<ShipError> for AppError {
    fn from(error: ShipError) -> Self {
        AppError::Ship(error)
    }
}

impl Hedy {
    /// The folder's branch, changes, ahead/behind, remote host and `gh`.
    /// Local except `gh auth status` (cached by the adapter).
    pub fn folder_status(&self, path: &Path) -> AppResult<FolderStatus> {
        let repo = self.fs.canonicalize(path)?;
        let git = self.source.folder(&repo)?;
        let remote = match git.upstream.as_deref().and_then(|u| u.split_once('/')) {
            Some((remote, _)) => Some(remote.to_owned()),
            None => pick_remote(&self.source.remotes(&repo)?),
        };
        let remote_host = self.host_of(&repo, remote.as_deref());
        let gh = remote_host.as_deref().map(|h| self.gh_state(&repo, h));
        Ok(FolderStatus {
            git,
            remote,
            remote_host,
            gh,
        })
    }

    fn host_of(&self, repo: &Path, remote: Option<&str>) -> Option<String> {
        let url = self.source.remote_url(repo, remote?).ok()??;
        remote_host(&url)
    }

    fn gh_state(&self, repo: &Path, host: &str) -> GhState {
        if !self.host.installed() {
            GhState::Missing
        } else if self.host.logged_in(repo, host) {
            GhState::Ready
        } else {
            GhState::LoggedOut
        }
    }

    /// Pushes `branch` to its upstream, or sets one on the folder's remote
    /// (`-u`) when it has none. Never forced.
    fn push_branch(&self, repo: &Path, branch: &str) -> AppResult<Pushed> {
        let (remote, remote_branch, set) = match self.source.upstream(repo, branch)? {
            Some(Upstream { remote, branch, .. }) => (remote, branch, false),
            None => {
                let remote = pick_remote(&self.source.remotes(repo)?).ok_or(ShipError::NoRemote)?;
                (remote, branch.to_owned(), true)
            }
        };
        if let Err(error) = self.source.push(repo, &remote, branch, &remote_branch, set) {
            return Err(ShipError::Push {
                problem: PushProblem::from_git(&error.message),
                branch: branch.to_owned(),
                host: self.host_of(repo, Some(&remote)),
                detail: error.message,
            }
            .into());
        }
        Ok(Pushed {
            branch: branch.to_owned(),
            upstream: format!("{remote}/{remote_branch}"),
            upstream_set: set,
        })
    }

    /// "Push" from the folder's menu: its current branch.
    pub fn push_folder(&self, path: &Path) -> AppResult<Pushed> {
        let repo = self.fs.canonicalize(path)?;
        let branch = self
            .source
            .folder(&repo)?
            .branch
            .ok_or(ShipError::Detached)?;
        self.push_branch(&repo, &branch)
    }

    /// "Pull" from the folder's menu: `git pull --ff-only`. On divergence it
    /// says so and points to VS Code; it never merges or rebases.
    pub fn pull_folder(&self, path: &Path) -> AppResult<Pulled> {
        let repo = self.fs.canonicalize(path)?;
        let folder = self.source.folder(&repo)?;
        let branch = folder.branch.clone().ok_or(ShipError::Detached)?;
        let pull_error = |problem, detail: String| ShipError::Pull {
            problem,
            branch: branch.clone(),
            upstream: folder.upstream.clone(),
            detail,
        };
        if folder.upstream.is_none() {
            return Err(pull_error(PullProblem::NoUpstream, String::new()).into());
        }
        let before = self.git.head_commit(&repo).unwrap_or_default();
        if let Err(error) = self.source.pull_ff_only(&repo) {
            return Err(pull_error(PullProblem::from_git(&error.message), error.message).into());
        }
        let after = self.git.head_commit(&repo).unwrap_or_default();
        Ok(Pulled {
            updated: before != after,
        })
    }

    fn shipped(&self, id: &TaskId) -> AppResult<(Task, Shipment)> {
        let task = self.task(id)?;
        match (&task.status, task.ship.clone()) {
            (TaskStatus::Accepted, Some(ship)) => Ok((task, ship)),
            _ => Err(ShipError::NotAccepted.into()),
        }
    }

    fn default_branch_of(
        &self,
        repo: &Path,
        remote: Option<&str>,
        gh: Option<GhState>,
    ) -> Option<String> {
        remote
            .and_then(|r| self.source.default_branch(repo, r))
            .or_else(|| {
                (gh == Some(GhState::Ready))
                    .then(|| self.host.default_branch(repo))
                    .flatten()
            })
    }

    /// Where an accepted task stands: pushed or not, its pull request, and
    /// whether one can be opened.
    pub fn ship_state(&self, id: &TaskId) -> AppResult<ShipState> {
        let (task, ship) = self.shipped(id)?;
        let repo = &task.project;
        let upstream = self.source.upstream(repo, &ship.branch)?;
        let remote = match &upstream {
            Some(up) => Some(up.remote.clone()),
            None => pick_remote(&self.source.remotes(repo)?),
        };
        let remote_host = self.host_of(repo, remote.as_deref());
        let gh = remote_host.as_deref().map(|h| self.gh_state(repo, h));
        let default_branch = self.default_branch_of(repo, remote.as_deref(), gh);
        let can_open_pr =
            gh == Some(GhState::Ready) && default_branch.as_deref() != Some(ship.branch.as_str());
        Ok(ShipState {
            pushed: upstream.as_ref().is_some_and(|u| u.ahead == 0),
            ahead: upstream.as_ref().map_or(0, |u| u.ahead),
            behind: upstream.as_ref().map_or(0, |u| u.behind),
            upstream: upstream.as_ref().map(Upstream::name),
            branch: ship.branch,
            kept: ship.kept,
            remote,
            remote_host,
            gh,
            default_branch,
            pull_request: ship.pull_request,
            can_open_pr,
        })
    }

    /// "Publicar": pushes the branch that carries the accepted work (the
    /// base branch after a merge, the task's branch when kept).
    pub fn push_task(&self, id: &TaskId) -> AppResult<Pushed> {
        let (mut task, mut ship) = self.shipped(id)?;
        let pushed = self.push_branch(&task.project, &ship.branch)?;
        ship.pushed_to = Some(pushed.upstream.clone());
        task.ship = Some(ship);
        self.store.save_task(&task)?;
        self.events.task_updated(&task);
        Ok(pushed)
    }

    /// The pull request Hedy proposes, all from local facts: the commit's
    /// subject, what was asked, the agent's final message, the files and the
    /// tests the task ran.
    pub fn pull_request_draft(&self, id: &TaskId) -> AppResult<PullRequestDraft> {
        let (task, ship) = self.shipped(id)?;
        let events = self.store.events(id)?;
        let summary = events.iter().rev().find_map(|e| match e {
            AgentEvent::Finished {
                ok: true, summary, ..
            } if !summary.trim().is_empty() => Some(summary.as_str()),
            _ => None,
        });
        // Each test command once, with its last result.
        let mut tests: Vec<(String, bool)> = Vec::new();
        for event in &events {
            if let AgentEvent::TestsRun { command, passed } = event {
                match tests.iter_mut().find(|(c, _)| c == command) {
                    Some(entry) => entry.1 = *passed,
                    None => tests.push((command.clone(), *passed)),
                }
            }
        }
        let body = pull_request_body(&PullRequestFacts {
            prompt: task.instruction(),
            summary,
            files: &ship.files,
            tests: &tests,
        });
        let state = self.ship_state(id)?;
        let base = state
            .default_branch
            .or_else(|| task.workspace.as_ref().map(|w| w.base_branch.clone()))
            .unwrap_or_else(|| "main".into());
        Ok(PullRequestDraft {
            title: commit_subject(&ship.message).to_owned(),
            body,
            base,
            head: ship.branch,
        })
    }

    /// "Crear PR" with GitHub CLI, from the pushed branch. An existing PR
    /// for the same branches is taken as the result.
    pub fn create_pull_request(
        &self,
        id: &TaskId,
        request: NewPullRequest,
    ) -> AppResult<PullRequest> {
        let (mut task, mut ship) = self.shipped(id)?;
        let state = self.ship_state(id)?;
        match (state.gh, state.remote_host.clone()) {
            (Some(GhState::Ready), _) => {}
            (Some(GhState::LoggedOut), Some(host)) => {
                return Err(ShipError::GhLoggedOut { host }.into());
            }
            (None, _) => return Err(ShipError::NoRemote.into()),
            _ => return Err(ShipError::GhMissing.into()),
        }
        if !state.pushed {
            return Err(ShipError::NotPushed {
                branch: ship.branch.clone(),
            }
            .into());
        }
        let draft = self.pull_request_draft(id)?;
        let pick = |given: String, default: String| {
            if given.trim().is_empty() {
                default
            } else {
                given.trim().to_owned()
            }
        };
        let spec = PullRequestSpec {
            title: pick(request.title, draft.title),
            body: if request.body.trim().is_empty() {
                draft.body
            } else {
                request.body
            },
            base: pick(request.base, draft.base),
            head: ship.branch.clone(),
            draft: request.draft,
        };
        let pr = match self.host.create_pull_request(&task.project, &spec) {
            Ok(out) => {
                PullRequest::from_gh_output(&out, spec.draft).ok_or(ShipError::PullRequest(out))?
            }
            // "a pull request for branch … already exists: <url>"
            Err(error) => PullRequest::from_gh_output(&error.message, spec.draft)
                .ok_or(ShipError::PullRequest(error.message))?,
        };
        ship.pull_request = Some(pr.clone());
        task.ship = Some(ship);
        self.store.save_task(&task)?;
        self.events.task_updated(&task);
        Ok(pr)
    }

    /// Whether `account` is known to be at its limit now: the task waits for
    /// it, its last report said so, or a plan window is full.
    pub(super) fn at_limit(&self, task: &Task, account: &Account) -> AppResult<bool> {
        if task.wait.as_ref().is_some_and(|w| w.account == account.id) {
            return Ok(true);
        }
        let now = self.now_secs();
        if let Some(limit) = self.store.rate_limit(&account.id)?
            && LimitLevel::from_report(&limit.status, limit.utilization)
                == Some(LimitLevel::Reached)
            && limit.resets_at.map_or(now - limit.at < 3600, |at| at > now)
        {
            return Ok(true);
        }
        let full = self
            .plan_usage_of(account)
            .ok()
            .and_then(|s| s.usage)
            .is_some_and(|u| {
                u.windows
                    .iter()
                    .any(|w| w.percent >= 100.0 && w.resets_at.is_none_or(|at| at > now))
            });
        Ok(full)
    }

    /// "Sugerir con IA": asks the task's own account and model (never
    /// another one: rule 3) for a one-line Conventional Commit message from
    /// the diff. Spends a few tokens, recorded as the task's usage.
    pub fn suggest_commit_message(&self, id: &TaskId) -> AppResult<String> {
        self.suggest_commit_message_within(id, SUGGESTION_TIMEOUT)
    }

    pub(super) fn suggest_commit_message_within(
        &self,
        id: &TaskId,
        timeout: Duration,
    ) -> AppResult<String> {
        let task = self.acceptable(id)?;
        let ws = Self::workspace(&task)?.clone();
        let stage = current_stage(&task);
        let account = self.account(stage.account.as_str())?;
        let project = self.project_at(&task.project)?;
        policy::check_launch(Some(&project), &account.id, &self.projects()?)?;
        if self.at_limit(&task, &account)? {
            return Err(ShipError::AccountAtLimit {
                account: account.name.clone(),
            }
            .into());
        }
        let diff = self.git.diff(&ws.worktree, &ws.base_commit)?;
        let recent: Vec<String> = self
            .git
            .recent_commits(&task.project, 5)
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.subject)
            .collect();
        let prompt = commit_message_prompt(
            &task.title,
            &recent,
            &diff_for_prompt(&diff, SUGGESTION_DIFF_MAX),
        );
        let answer = self.one_shot(OneShot {
            task: id,
            account: &account,
            stage: &stage,
            cwd: ws.worktree.clone(),
            prompt,
            refusal: tr!(
                "No uses herramientas: responde solo con el mensaje de commit.",
                "Do not use tools: reply with the commit message only."
            ),
            timeout,
        })?;
        let result = match answer {
            Ok(text) => clean_suggestion(&text).ok_or(ShipError::SuggestionFailed(None)),
            Err(OneShotFailure::AtLimit) => Err(ShipError::AccountAtLimit {
                account: account.name.clone(),
            }),
            Err(OneShotFailure::Failed(detail)) => Err(ShipError::SuggestionFailed(detail)),
            Err(OneShotFailure::TimedOut) => Err(ShipError::SuggestionTimedOut),
        };
        Ok(result?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewTask;
    use crate::application::ports::RateLimit;
    use crate::application::ports::{TaskRepository, UsageRepository};
    use crate::application::testing::{AgentCall, Fixture};
    use crate::domain::diff::{FileDiff, FileStatus};
    use crate::domain::{AcceptMode, UsageSample};
    use crate::domain::{Phase, TaskMode};
    use crate::i18n::{Language, with_language};

    fn new_task(f: &Fixture) -> Task {
        f.add_claude("Personal");
        f.fs.add_git_root("/code/moira");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        f.git.set_diff(vec![FileDiff {
            path: "app.py".into(),
            old_path: None,
            status: FileStatus::Modified,
            additions: 3,
            deletions: 1,
            binary: false,
            hunks: vec![],
        }]);
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/moira"),
                prompt: "Recordatorios por WhatsApp".into(),
                mode: TaskMode::Project,
                model: Some("sonnet".into()),
                plan_first: false,
            })
            .unwrap()
    }

    fn ship_error(result: AppResult<impl std::fmt::Debug>) -> ShipError {
        match result {
            Err(AppError::Ship(error)) => error,
            other => panic!("expected a ship error, got {other:?}"),
        }
    }

    #[test]
    fn folder_status_has_the_host_but_never_the_url_and_asks_gh_only_with_a_remote() {
        let f = Fixture::new();
        f.source.set_folder(GitFolder {
            branch: Some("main".into()),
            upstream: Some("origin/main".into()),
            ahead: 1,
            behind: 2,
            changed: 3,
            ..Default::default()
        });
        f.source
            .set_remotes(&[("origin", "https://u:tok@github.com/a/b.git")]);
        f.host.set(true, true);
        let status = f.hedy.folder_status(&f.dir("/code/moira")).unwrap();
        assert_eq!(status.remote.as_deref(), Some("origin"));
        assert_eq!(status.remote_host.as_deref(), Some("github.com"));
        assert_eq!(status.gh, Some(GhState::Ready));
        assert_eq!(
            (status.git.ahead, status.git.behind, status.git.changed),
            (1, 2, 3)
        );

        f.host.set(true, false);
        assert_eq!(
            f.hedy.folder_status(&f.dir("/code/moira")).unwrap().gh,
            Some(GhState::LoggedOut)
        );
        f.host.set(false, false);
        assert_eq!(
            f.hedy.folder_status(&f.dir("/code/moira")).unwrap().gh,
            Some(GhState::Missing)
        );
        f.source.set_remotes(&[]);
        f.source.set_folder(GitFolder {
            branch: Some("main".into()),
            ..Default::default()
        });
        let status = f.hedy.folder_status(&f.dir("/code/moira")).unwrap();
        assert_eq!((status.remote, status.gh), (None, None));
    }

    #[test]
    fn pushing_the_folder_sets_an_upstream_when_missing_and_never_forces() {
        let f = Fixture::new();
        f.source
            .set_remotes(&[("origin", "git@github.com:a/b.git")]);
        let pushed = f.hedy.push_folder(&f.dir("/code/moira")).unwrap();
        assert!(pushed.upstream_set);
        assert_eq!(pushed.upstream, "origin/main");
        assert_eq!(f.source.log(), vec!["push -u origin main:main"]);

        f.source.set_upstream("main", "origin", "main", 0, 0);
        let pushed = f.hedy.push_folder(&f.dir("/code/moira")).unwrap();
        assert!(!pushed.upstream_set);
        assert_eq!(f.source.log().last().unwrap(), "push origin main:main");
    }

    #[test]
    fn push_failures_become_humane_hints() {
        let f = Fixture::new();
        f.source
            .set_remotes(&[("origin", "https://github.com/a/b.git")]);
        f.source
            .fail_push(" ! [rejected] main -> main (non-fast-forward)");
        let error = ship_error(f.hedy.push_folder(&f.dir("/code/moira")));
        assert!(matches!(
            error,
            ShipError::Push {
                problem: PushProblem::Rejected,
                ..
            }
        ));
        let message = with_language(Language::English, || AppError::Ship(error).to_string());
        assert!(message.contains("Pull them"), "{message}");
        assert!(message.contains("never force-pushes"), "{message}");

        f.source
            .fail_push("fatal: Authentication failed for 'https://github.com/a/b.git/'");
        let error = ship_error(f.hedy.push_folder(&f.dir("/code/moira")));
        let message = AppError::Ship(error).to_string();
        assert!(message.contains("github.com no aceptó"), "{message}");

        f.source.set_remotes(&[]);
        assert_eq!(
            ship_error(f.hedy.push_folder(&f.dir("/code/moira"))),
            ShipError::NoRemote
        );
    }

    #[test]
    fn pull_is_fast_forward_only_and_explains_divergence() {
        let f = Fixture::new();
        f.source.set_folder(GitFolder {
            branch: Some("main".into()),
            upstream: Some("origin/main".into()),
            behind: 2,
            ..Default::default()
        });
        // The fake moves no commit: already up to date.
        assert!(!f.hedy.pull_folder(&f.dir("/code/moira")).unwrap().updated);
        assert_eq!(f.source.log(), vec!["pull --ff-only"]);

        f.source
            .fail_pull("fatal: Not possible to fast-forward, aborting.");
        let error = ship_error(f.hedy.pull_folder(&f.dir("/code/moira")));
        let message = AppError::Ship(error.clone()).to_string();
        assert!(matches!(
            error,
            ShipError::Pull {
                problem: PullProblem::Diverged,
                ..
            }
        ));
        assert!(message.contains("VS Code"), "{message}");

        f.source.set_folder(GitFolder {
            branch: Some("main".into()),
            ..Default::default()
        });
        assert!(matches!(
            ship_error(f.hedy.pull_folder(&f.dir("/code/moira"))),
            ShipError::Pull {
                problem: PullProblem::NoUpstream,
                ..
            }
        ));
    }

    #[test]
    fn a_task_kept_on_its_branch_is_pushed_and_gets_a_pull_request() {
        let f = Fixture::new();
        let task = new_task(&f);
        let ws = task.workspace.clone().unwrap();
        f.agents.emit(AgentEvent::TestsRun {
            command: "pytest".into(),
            passed: false,
        });
        f.agents.emit(AgentEvent::TestsRun {
            command: "pytest".into(),
            passed: true,
        });
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Se programan 24 h antes.".into(),
            duration_ms: None,
        });
        f.source
            .set_remotes(&[("origin", "git@github.com:a/b.git")]);
        f.source.set_default_branch("main");
        f.host.set(true, true);

        let accepted = f
            .hedy
            .accept_task_as(
                &task.id,
                Some("feat: recordatorios\n\nCuerpo"),
                AcceptMode::Branch,
            )
            .unwrap();
        assert_eq!(accepted.status, TaskStatus::Accepted);
        let ship = accepted.ship.clone().unwrap();
        assert!(ship.kept);
        assert_eq!(ship.branch, ws.branch);
        // Nothing merged into the main folder; the branch stays.
        assert!(
            !f.git
                .log()
                .iter()
                .any(|l| l.starts_with("merge") || l.starts_with("remove"))
        );
        assert_eq!(
            f.source.log(),
            vec![format!("unlink {}", ws.worktree.display())]
        );

        let state = f.hedy.ship_state(&task.id).unwrap();
        assert_eq!(
            (state.pushed, state.upstream.clone(), state.can_open_pr),
            (false, None, true)
        );
        assert_eq!(
            ship_error(
                f.hedy
                    .create_pull_request(&task.id, NewPullRequest::default())
            ),
            ShipError::NotPushed {
                branch: ws.branch.clone()
            }
        );

        let pushed = f.hedy.push_task(&task.id).unwrap();
        assert!(pushed.upstream_set);
        assert_eq!(
            f.source.log().last().unwrap(),
            &format!("push -u origin {0}:{0}", ws.branch)
        );
        assert_eq!(
            f.hedy.task(&task.id).unwrap().ship.unwrap().pushed_to,
            Some(format!("origin/{}", ws.branch))
        );

        let draft = f.hedy.pull_request_draft(&task.id).unwrap();
        assert_eq!(draft.title, "feat: recordatorios");
        assert_eq!(
            (draft.base.as_str(), draft.head.as_str()),
            ("main", ws.branch.as_str())
        );
        assert!(draft.body.contains("> Recordatorios por WhatsApp"));
        assert!(draft.body.contains("Se programan 24 h antes."));
        assert!(draft.body.contains("`app.py` (+3 −1)"));
        assert!(draft.body.contains("✅ `pytest`"), "{}", draft.body);

        f.host.reply("https://github.com/a/b/pull/12\n");
        let pr = f
            .hedy
            .create_pull_request(
                &task.id,
                NewPullRequest {
                    draft: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!((pr.number, pr.draft), (Some(12), true));
        let spec = f.host.created().pop().unwrap();
        assert_eq!(spec.title, "feat: recordatorios");
        assert_eq!(
            (spec.base.as_str(), spec.head.as_str()),
            ("main", ws.branch.as_str())
        );
        assert!(spec.draft);
        assert_eq!(
            f.hedy.task(&task.id).unwrap().ship.unwrap().pull_request,
            Some(pr)
        );
    }

    #[test]
    fn reopening_a_kept_task_brings_its_worktree_back_on_the_same_branch() {
        let f = Fixture::new();
        let task = new_task(&f);
        let ws = task.workspace.clone().unwrap();
        f.hedy
            .accept_task_as(&task.id, None, AcceptMode::Branch)
            .unwrap();
        let reopened = f.hedy.undo_accept(&task.id).unwrap();
        assert_eq!(reopened.status, TaskStatus::Review);
        assert_eq!(
            f.source.log().last().unwrap(),
            &format!("attach {} {}", ws.worktree.display(), ws.branch)
        );
        // Accepting again works, on the branch or merged.
        f.hedy.accept_task(&task.id).unwrap();
    }

    #[test]
    fn a_merged_task_pushes_the_base_branch_and_offers_a_pr_only_off_the_default_branch() {
        let f = Fixture::new();
        let task = new_task(&f);
        f.source
            .set_remotes(&[("origin", "git@github.com:a/b.git")]);
        f.source.set_default_branch("main");
        f.host.set(true, true);
        f.hedy.accept_task(&task.id).unwrap();
        let state = f.hedy.ship_state(&task.id).unwrap();
        assert_eq!(state.branch, "main");
        assert!(!state.kept);
        assert!(!state.can_open_pr, "main is the default branch");
        f.hedy.push_task(&task.id).unwrap();
        assert_eq!(f.source.log().last().unwrap(), "push -u origin main:main");
    }

    #[test]
    fn pull_requests_need_gh_ready() {
        let f = Fixture::new();
        let task = new_task(&f);
        f.source
            .set_remotes(&[("origin", "https://github.com/a/b")]);
        f.hedy
            .accept_task_as(&task.id, None, AcceptMode::Branch)
            .unwrap();
        let ws = task.workspace.clone().unwrap();
        f.source
            .set_upstream(&ws.branch, "origin", &ws.branch, 0, 0);
        f.host.set(false, false);
        assert_eq!(
            ship_error(
                f.hedy
                    .create_pull_request(&task.id, NewPullRequest::default())
            ),
            ShipError::GhMissing
        );
        f.host.set(true, false);
        assert_eq!(
            ship_error(
                f.hedy
                    .create_pull_request(&task.id, NewPullRequest::default())
            ),
            ShipError::GhLoggedOut {
                host: "github.com".into()
            }
        );
        // An existing PR for the same branches is the result.
        f.host.set(true, true);
        f.host.fail("a pull request for branch \"x\" into branch \"main\" already exists:\nhttps://github.com/a/b/pull/3");
        let pr = f
            .hedy
            .create_pull_request(&task.id, NewPullRequest::default())
            .unwrap();
        assert_eq!(pr.number, Some(3));
        f.host.fail("HTTP 422: Validation Failed");
        assert!(matches!(
            ship_error(
                f.hedy
                    .create_pull_request(&task.id, NewPullRequest::default())
            ),
            ShipError::PullRequest(_)
        ));
        // Nothing to publish before accepting.
        let other = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/moira"),
                prompt: "Otra".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
            })
            .unwrap();
        assert_eq!(
            ship_error(f.hedy.push_task(&other.id)),
            ShipError::NotAccepted
        );
    }

    #[test]
    fn the_ai_suggestion_uses_the_tasks_own_account_read_only_and_counts_its_tokens() {
        let f = Fixture::new();
        let task = new_task(&f);
        f.agents.reply_on_start(vec![
            AgentEvent::Usage(UsageSample {
                input_tokens: 900,
                output_tokens: 12,
                ..Default::default()
            }),
            AgentEvent::Finished {
                ok: true,
                summary: "```\nfeat(avisos): send WhatsApp reminders\n```".into(),
                duration_ms: None,
            },
        ]);
        let message = f.hedy.suggest_commit_message(&task.id).unwrap();
        assert_eq!(message, "feat(avisos): send WhatsApp reminders");
        let request = f.agents.last_request();
        assert_eq!(
            (request.mode, request.phase),
            (TaskMode::Ask, Phase::Review)
        );
        assert_eq!(request.model.as_deref(), Some("sonnet"));
        assert!(request.memory.is_none() && request.resume.is_none());
        assert!(request.prompt.contains("--- app.py (+3 -1)"));
        assert_eq!(
            request.launch.cwd,
            Some(task.workspace.clone().unwrap().worktree)
        );
        assert_eq!(f.agents.calls().last(), Some(&AgentCall::Stop));
        let spent = f.store.usage_since(&task.account, 0).unwrap();
        assert_eq!(spent.input_tokens, 900);
        // Not part of the task's conversation.
        assert!(!f
            .store
            .events(&task.id)
            .unwrap()
            .iter()
            .any(|e| matches!(e, AgentEvent::Finished { summary, .. } if summary.contains("WhatsApp reminders"))));
    }

    #[test]
    fn the_ai_suggestion_never_uses_another_account_at_a_limit() {
        let f = Fixture::new();
        let task = new_task(&f);
        let started = f.agents.requests().len();
        f.store
            .set_rate_limit(
                &task.account,
                &RateLimit {
                    status: "rejected".into(),
                    utilization: None,
                    resets_at: Some(f.hedy.now_secs() + 600),
                    at: f.hedy.now_secs(),
                },
            )
            .unwrap();
        assert_eq!(
            ship_error(f.hedy.suggest_commit_message(&task.id)),
            ShipError::AccountAtLimit {
                account: "Personal".into()
            }
        );
        assert_eq!(f.agents.requests().len(), started, "no agent started");

        // A limit reached while answering stops it too.
        f.store
            .set_rate_limit(
                &task.account,
                &RateLimit {
                    status: "allowed".into(),
                    utilization: None,
                    resets_at: None,
                    at: f.hedy.now_secs(),
                },
            )
            .unwrap();
        f.agents.reply_on_start(vec![AgentEvent::RateLimit {
            status: "rejected".into(),
            utilization: None,
            resets_at: None,
        }]);
        assert!(matches!(
            ship_error(f.hedy.suggest_commit_message(&task.id)),
            ShipError::AccountAtLimit { .. }
        ));
        assert_eq!(f.agents.requests().len(), started + 1);
    }

    #[test]
    fn the_ai_suggestion_gives_up_after_a_while_or_on_an_empty_answer() {
        let f = Fixture::new();
        let task = new_task(&f);
        f.agents.reply_on_start(vec![]);
        assert_eq!(
            ship_error(
                f.hedy
                    .suggest_commit_message_within(&task.id, Duration::from_millis(20))
            ),
            ShipError::SuggestionTimedOut
        );
        f.agents.reply_on_start(vec![AgentEvent::Finished {
            ok: true,
            summary: "  ".into(),
            duration_ms: None,
        }]);
        assert_eq!(
            ship_error(f.hedy.suggest_commit_message(&task.id)),
            ShipError::SuggestionFailed(None)
        );
    }
}
