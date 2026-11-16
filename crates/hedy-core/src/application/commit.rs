//! `hedy commit`: a commit message for what is staged in the user's own
//! folder, from a one-shot request to the folder's account (read-only,
//! tools denied, no memory or MCP), and the commit itself through the user's
//! own git (hooks and signing included). Hedy never stages or pushes here,
//! and never sends a protected file's contents (`.env`, keys...).

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::one_shot::{OneShot, OneShotFailure};
use super::ship::{SUGGESTION_DIFF_MAX, SUGGESTION_TIMEOUT, ShipError};
use super::{AppResult, Hedy};
use crate::domain::permissions::is_protected_path;
use crate::domain::ship::{clean_suggestion, diff_for_prompt, staged_commit_prompt};
use crate::domain::{Account, FileDiff, Phase, Stage};
use crate::tr;

/// Recent subjects shown to the model, for the repository's style.
const RECENT_SUBJECTS: usize = 8;

/// A suggested message for the staged changes.
#[derive(Debug, Clone, PartialEq)]
pub struct StagedSuggestion {
    pub message: String,
    /// Whose tokens it spent.
    pub account: Account,
    /// The repository's root.
    pub repo: PathBuf,
    /// Staged protected files whose contents were not sent (only named).
    pub skipped: Vec<String>,
    /// The diff was cut to fit the request.
    pub truncated: bool,
}

impl Hedy {
    /// Asks `account` (else the folder's project account, else the default
    /// one) for a one-line message for the staged changes of the repository
    /// at `dir`. Respects protected projects, and refuses an account at its
    /// limit without trying another (rule 3). Spends a few of its tokens.
    pub fn suggest_staged_commit(
        &self,
        dir: &Path,
        account: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<StagedSuggestion> {
        self.suggest_staged_commit_within(dir, account, model, SUGGESTION_TIMEOUT)
    }

    pub(super) fn suggest_staged_commit_within(
        &self,
        dir: &Path,
        account: Option<&str>,
        model: Option<&str>,
        timeout: Duration,
    ) -> AppResult<StagedSuggestion> {
        let repo = self.repo_root(dir)?;
        let staged = self.git.staged_diff(&repo)?;
        if staged.is_empty() {
            return Err(ShipError::NothingStaged.into());
        }
        let (_, account) = self.resolve_account(dir, account)?;
        if self.account_at_limit(&account)? {
            return Err(ShipError::AccountAtLimit {
                account: account.name.clone(),
            }
            .into());
        }
        let (secret, shown): (Vec<FileDiff>, Vec<FileDiff>) = staged.into_iter().partition(|f| {
            is_protected_path(&f.path) || f.old_path.as_deref().is_some_and(is_protected_path)
        });
        let skipped: Vec<String> = secret.into_iter().map(|f| f.path).collect();
        let truncated = diff_for_prompt(&shown, usize::MAX).chars().count() > SUGGESTION_DIFF_MAX;
        let diff = diff_for_prompt(&shown, SUGGESTION_DIFF_MAX);
        let recent: Vec<String> = self
            .git
            .recent_commits(&repo, RECENT_SUBJECTS)
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.subject)
            .collect();
        let stage = Stage {
            phase: Phase::Review,
            account: account.id.clone(),
            model: model.map(str::to_owned),
        };
        let answer = self.one_shot(OneShot {
            task: None,
            account: &account,
            stage: &stage,
            cwd: repo.clone(),
            prompt: staged_commit_prompt(&recent, &diff, &skipped),
            refusal: tr!(
                "No uses herramientas: responde solo con el mensaje de commit.",
                "Do not use tools: reply with the commit message only."
            ),
            timeout,
        })?;
        let message = match answer {
            Ok(text) => clean_suggestion(&text).ok_or(ShipError::SuggestionFailed(None)),
            Err(OneShotFailure::AtLimit) => Err(ShipError::AccountAtLimit {
                account: account.name.clone(),
            }),
            Err(OneShotFailure::Failed(detail)) => Err(ShipError::SuggestionFailed(detail)),
            Err(OneShotFailure::TimedOut) => Err(ShipError::SuggestionTimedOut),
        }?;
        Ok(StagedSuggestion {
            message,
            account,
            repo,
            skipped,
            truncated,
        })
    }

    /// Commits what is staged in the repository at `dir` with `message`
    /// through the user's own git (hooks, signing). With `edit`, git opens
    /// the user's editor on it first (an emptied message cancels).
    pub fn commit_staged(&self, dir: &Path, message: &str, edit: bool) -> AppResult<()> {
        let repo = self.repo_root(dir)?;
        if !edit && message.trim().is_empty() {
            return Err(ShipError::EmptyMessage.into());
        }
        let mut message = message.trim().to_owned();
        message.push('\n');
        Ok(self.git.commit_staged(&repo, &message, edit)?)
    }

    fn repo_root(&self, dir: &Path) -> AppResult<PathBuf> {
        let dir = self.fs.canonicalize(dir)?;
        Ok(self.fs.git_root(&dir).ok_or(ShipError::NotARepository)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::AppError;
    use crate::application::ports::{RateLimit, UsageRepository};
    use crate::application::testing::{AgentCall, Fixture};
    use crate::application::{Setting, ShipError};
    use crate::domain::diff::{DiffLine, FileStatus, Hunk, LineKind};
    use crate::domain::{AgentEvent, DomainError, TaskMode, UsageSample};

    fn file(path: &str, added: &str) -> FileDiff {
        FileDiff {
            path: path.into(),
            old_path: None,
            status: FileStatus::Modified,
            additions: 1,
            deletions: 0,
            binary: false,
            hunks: vec![Hunk {
                header: "@@ -1 +1,2 @@".into(),
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_line: None,
                    new_line: Some(2),
                    text: added.into(),
                }],
            }],
        }
    }

    fn setup() -> Fixture {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f
    }

    fn answer(f: &Fixture, text: &str) {
        f.agents.reply_on_start(vec![
            AgentEvent::Usage(UsageSample {
                input_tokens: 400,
                output_tokens: 9,
                ..Default::default()
            }),
            AgentEvent::Finished {
                ok: true,
                summary: text.into(),
                duration_ms: None,
            },
        ]);
    }

    fn ship_error(result: AppResult<impl std::fmt::Debug>) -> ShipError {
        match result {
            Err(AppError::Ship(error)) => error,
            other => panic!("expected a ship error, got {other:?}"),
        }
    }

    #[test]
    fn the_prompt_has_the_staged_diff_and_recent_subjects_but_no_secrets() {
        let f = setup();
        f.git.set_staged(vec![
            file("src/avisos.py", "def avisar(): enviar_whatsapp()"),
            file("config/.env", "WHATSAPP_TOKEN=sk-secreto"),
        ]);
        f.git
            .set_commits(&["feat(avisos): add reminders", "fix(app): keep the queue"]);
        answer(&f, "```\nfeat(avisos): send reminders by WhatsApp\n```");

        let s = f
            .hedy
            .suggest_staged_commit(&f.dir("/code/agenda/src"), None, Some("haiku"))
            .unwrap();
        assert_eq!(s.message, "feat(avisos): send reminders by WhatsApp");
        assert_eq!(s.account.id.as_str(), "personal");
        assert_eq!(s.repo, f.dir("/code/agenda"));
        assert_eq!(s.skipped, ["config/.env"]);
        assert!(!s.truncated);

        let request = f.agents.last_request();
        assert!(request.prompt.contains("--- src/avisos.py (+1 -0)"));
        assert!(request.prompt.contains("+def avisar(): enviar_whatsapp()"));
        assert!(request.prompt.contains("- feat(avisos): add reminders"));
        assert!(request.prompt.contains("Conventional Commits"));
        // The secret file is only named, never its contents.
        assert!(!request.prompt.contains("sk-secreto"));
        assert!(!request.prompt.contains("--- config/.env"));
        assert!(request.prompt.contains("config/.env"));
        // Read-only, no memory or MCP, in the repository, the chosen model.
        assert_eq!(
            (request.mode, request.phase),
            (TaskMode::Ask, Phase::Review)
        );
        assert!(request.memory.is_none() && request.resume.is_none() && request.mcp.is_empty());
        assert_eq!(request.launch.cwd, Some(f.dir("/code/agenda")));
        assert_eq!(request.model.as_deref(), Some("haiku"));
        assert_eq!(f.agents.calls().last(), Some(&AgentCall::Stop));
        // Its tokens count for the account.
        let spent = f.store.usage_since(&s.account.id, 0).unwrap();
        assert_eq!(spent.input_tokens, 400);
    }

    #[test]
    fn follows_the_repository_style_when_it_is_not_conventional() {
        let f = setup();
        f.git.set_staged(vec![file("app.py", "x = 1")]);
        f.git.set_commits(&["Add reminders", "Keep the queue"]);
        answer(&f, "Add the x setting");
        f.hedy
            .suggest_staged_commit(&f.dir("/code/agenda"), None, None)
            .unwrap();
        let prompt = f.agents.last_request().prompt;
        assert!(!prompt.contains("Conventional Commits"), "{prompt}");
        assert!(prompt.contains("- Keep the queue"));
    }

    #[test]
    fn long_diffs_are_cut() {
        let f = setup();
        let big = "y".repeat(SUGGESTION_DIFF_MAX + 50);
        f.git.set_staged(vec![file("big.txt", &big)]);
        answer(&f, "chore: add big file");
        let s = f
            .hedy
            .suggest_staged_commit(&f.dir("/code/agenda"), None, None)
            .unwrap();
        assert!(s.truncated);
        assert!(f.agents.last_request().prompt.contains("(diff truncated)"));
    }

    #[test]
    fn nothing_staged_or_no_repository_is_a_clear_error() {
        let f = setup();
        assert_eq!(
            ship_error(
                f.hedy
                    .suggest_staged_commit(&f.dir("/code/agenda"), None, None)
            ),
            ShipError::NothingStaged
        );
        assert_eq!(
            ship_error(f.hedy.suggest_staged_commit(&f.dir("/tmp/x"), None, None)),
            ShipError::NotARepository
        );
        assert!(f.agents.requests().is_empty());
    }

    #[test]
    fn an_account_at_its_limit_is_refused_and_no_other_is_used() {
        let f = setup();
        f.add_claude("Trabajo");
        f.git.set_staged(vec![file("app.py", "x = 1")]);
        let personal = crate::domain::AccountId::new("personal").unwrap();
        f.store
            .set_rate_limit(
                &personal,
                &RateLimit {
                    status: "rejected".into(),
                    utilization: None,
                    resets_at: Some(f.hedy.now_secs() + 600),
                    at: f.hedy.now_secs(),
                },
            )
            .unwrap();
        assert_eq!(
            ship_error(
                f.hedy
                    .suggest_staged_commit(&f.dir("/code/agenda"), None, None)
            ),
            ShipError::AccountAtLimit {
                account: "Personal".into()
            }
        );
        assert!(f.agents.requests().is_empty(), "no agent started");

        // A limit reached while answering stops it too.
        f.store
            .set_rate_limit(
                &personal,
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
            ship_error(
                f.hedy
                    .suggest_staged_commit(&f.dir("/code/agenda"), None, None)
            ),
            ShipError::AccountAtLimit { .. }
        ));
        assert_eq!(f.agents.requests().len(), 1);
        assert_eq!(
            f.agents.last_request().launch.env,
            vec![("CLAUDE_CONFIG_DIR".into(), "/data/accounts/personal".into())]
        );
    }

    #[test]
    fn the_account_follows_the_flag_then_the_project_then_the_default_and_protection() {
        let f = setup();
        f.add_claude("Trabajo");
        f.git.set_staged(vec![file("app.py", "x = 1")]);
        answer(&f, "feat: x");
        let s = f
            .hedy
            .suggest_staged_commit(&f.dir("/code/agenda"), Some("trabajo"), None)
            .unwrap();
        assert_eq!(s.account.id.as_str(), "trabajo");

        // Outside a project: the default account.
        f.fs.add_git_root("/code/otro");
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("trabajo"))
            .unwrap();
        answer(&f, "feat: x");
        let s = f
            .hedy
            .suggest_staged_commit(&f.dir("/code/otro"), None, None)
            .unwrap();
        assert_eq!(s.account.id.as_str(), "trabajo");

        // A protected project only uses its own account.
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("trabajo"))
            .unwrap();
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", Some(true))
            .unwrap();
        assert!(matches!(
            f.hedy
                .suggest_staged_commit(&f.dir("/code/agenda"), Some("trabajo"), None),
            Err(AppError::Domain(DomainError::ProtectedProject { .. }))
        ));
    }

    #[test]
    fn commits_through_git_and_never_an_empty_message() {
        let f = setup();
        f.hedy
            .commit_staged(&f.dir("/code/agenda/src"), "  feat: x  ", false)
            .unwrap();
        f.hedy
            .commit_staged(&f.dir("/code/agenda"), "feat: y", true)
            .unwrap();
        assert_eq!(
            f.git.log(),
            [
                "commit-staged /code/agenda feat: x\n",
                "commit-staged /code/agenda (edit) feat: y\n"
            ]
        );
        assert_eq!(
            ship_error(f.hedy.commit_staged(&f.dir("/code/agenda"), " \n", false)),
            ShipError::EmptyMessage
        );
        assert_eq!(
            ship_error(f.hedy.commit_staged(&f.dir("/tmp"), "feat: x", false)),
            ShipError::NotARepository
        );
    }
}
