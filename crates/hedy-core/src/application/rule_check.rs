//! Checking a change against the written rules (docs/decisions.md §37): a
//! task's changes, what is staged or a range of commits, judged by the
//! notes an agent in that project reads, each break citing its note. One
//! read-only request (tools and shell denied, an empty folder), like asking
//! the memory; nothing is sent without rules or lines to judge, protected
//! files go only by name, and it only warns: it never blocks.

use std::path::PathBuf;
use std::time::Duration;

use super::ask::{AskScope, local_source};
use super::one_shot::{OneShot, OneShotFailure};
use super::{AppError, AppResult, Hedy};
use crate::domain::ask::Source;
use crate::domain::rule_check::{
    RuleBreak, change_for_prompt, parse_breaks, pick_rules, rules_prompt,
};
use crate::domain::{Account, FileDiff, Phase, Stage, TaskId};
use crate::tr;

/// How long a check may take: the change makes a longer request than a
/// question.
const RULES_TIMEOUT: Duration = Duration::from_secs(180);
/// Longest range of commits accepted (`main..HEAD`, a hash...).
const REVS_MAX: usize = 200;

/// Which change is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleTarget {
    /// A task's changes since it started.
    Task(TaskId),
    /// What is staged in the repository at that folder.
    Staged(PathBuf),
    /// `git diff <revs>` in the repository at that folder: `HEAD` for what
    /// is not committed yet, `main..HEAD` for a branch.
    Revs { dir: PathBuf, revs: String },
}

/// The breaks found, with every rule sent (numbered from 1).
#[derive(Debug, Clone, PartialEq)]
pub struct RuleCheck {
    pub breaks: Vec<RuleBreak>,
    pub rules: Vec<Source>,
    /// Files whose lines were judged.
    pub files: usize,
    /// Protected files of the change: named, never sent.
    pub protected: Vec<String>,
    /// The change did not all fit: only its start was judged.
    pub truncated: bool,
    /// Whose tokens it spent; `None` when nothing was sent.
    pub account: Option<Account>,
}

impl Hedy {
    /// Checks `target` against the rules of its project through `account`
    /// (else the project's, else the default one), never another account
    /// (rule 3). Spends a few of its tokens, only when there are rules and
    /// added lines to judge.
    pub fn check_rules(
        &self,
        target: &RuleTarget,
        account: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<RuleCheck> {
        self.check_rules_within(target, account, model, RULES_TIMEOUT)
    }

    pub(super) fn check_rules_within(
        &self,
        target: &RuleTarget,
        account: Option<&str>,
        model: Option<&str>,
        timeout: Duration,
    ) -> AppResult<RuleCheck> {
        let (project, diff) = self.rule_change(target)?;
        if diff.is_empty() {
            return Err(AppError::RulesNoChanges);
        }
        let scope = project.map_or(AskScope::Mine, AskScope::Project);
        let sources = match &scope {
            AskScope::Project(_) => self.ask_sources(&scope)?,
            // A folder Hedy does not know: only the notes for every project.
            _ => {
                let projects = self.projects()?;
                self.store
                    .notes(None, false)?
                    .into_iter()
                    .filter_map(|n| local_source(n, &projects))
                    .collect()
            }
        };
        let rules = pick_rules(&diff, sources);
        let change = change_for_prompt(&diff);
        let nothing = |rules: Vec<Source>| RuleCheck {
            breaks: Vec::new(),
            rules,
            files: change.files,
            protected: change.protected.clone(),
            truncated: change.truncated,
            account: None,
        };
        if rules.is_empty() || change.added.is_empty() {
            return Ok(nothing(rules));
        }
        let (account, _) = self.ask_account(&scope, account)?;
        if self.account_at_limit(&account)? {
            return Err(AppError::AskAtLimit {
                account: account.name.clone(),
            });
        }
        let stage = Stage {
            phase: Phase::Review,
            account: account.id.clone(),
            model: model.map(str::to_owned),
        };
        let answer = self.one_shot(OneShot {
            task: None,
            account: &account,
            stage: &stage,
            // It needs no file: the change goes in the request.
            cwd: self.ask_dir()?,
            prompt: rules_prompt(&rules, &change.text),
            refusal: tr!(
                "No uses herramientas: revisa solo el cambio que te di.",
                "Do not use tools: review only the change you were given."
            ),
            no_shell: true,
            timeout,
        })?;
        let breaks = match answer {
            Ok(raw) => parse_breaks(&raw, &change, rules.len()),
            Err(OneShotFailure::AtLimit) => {
                return Err(AppError::AskAtLimit {
                    account: account.name.clone(),
                });
            }
            Err(OneShotFailure::Failed(detail)) => return Err(AppError::RulesFailed(detail)),
            Err(OneShotFailure::TimedOut) => return Err(AppError::RulesTimedOut),
        };
        Ok(RuleCheck {
            breaks,
            account: Some(account),
            ..nothing(rules)
        })
    }

    /// The project whose rules apply (`None`: a folder Hedy does not know)
    /// and the change.
    fn rule_change(&self, target: &RuleTarget) -> AppResult<(Option<PathBuf>, Vec<FileDiff>)> {
        match target {
            RuleTarget::Task(id) => {
                let task = self.task(id)?;
                Ok((Some(task.project.clone()), self.task_diff(id)?))
            }
            RuleTarget::Staged(dir) => {
                let repo = self.repo_root(dir)?;
                let project = self.project_for(&repo)?.map(|p| p.path);
                Ok((project, self.git.staged_diff(&repo)?))
            }
            RuleTarget::Revs { dir, revs } => {
                let revs = revs.trim();
                let plain = !revs.is_empty()
                    && revs.len() <= REVS_MAX
                    && !revs.starts_with('-')
                    && !revs.chars().any(|c| c.is_whitespace() || c.is_control());
                if !plain {
                    return Err(AppError::RulesBadRevs(revs.to_owned()));
                }
                let repo = self.repo_root(dir)?;
                let project = self.project_for(&repo)?.map(|p| p.path);
                Ok((project, self.git.revs_diff(&repo, revs)?))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::application::NewTask;
    use crate::application::ports::{RateLimit, UsageRepository};
    use crate::application::testing::Fixture;
    use crate::domain::diff::{DiffLine, FileStatus, Hunk, LineKind};
    use crate::domain::{AgentEvent, Category, TaskMode, UsageSample};

    fn change() -> Vec<FileDiff> {
        vec![
            FileDiff {
                path: "src/api/clientes.ts".into(),
                old_path: None,
                status: FileStatus::Modified,
                additions: 1,
                deletions: 0,
                binary: false,
                hunks: vec![Hunk {
                    header: "@@ -13 +13,2 @@".into(),
                    lines: vec![
                        DiffLine {
                            kind: LineKind::Context,
                            old_line: Some(13),
                            new_line: Some(13),
                            text: "const data = await get(id);".into(),
                        },
                        DiffLine {
                            kind: LineKind::Added,
                            old_line: None,
                            new_line: Some(14),
                            text: "const user_name = data.user_name;".into(),
                        },
                    ],
                }],
            },
            FileDiff {
                path: ".env".into(),
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
                        text: "STRIPE_KEY=sk_live_123".into(),
                    }],
                }],
            },
        ]
    }

    fn setup() -> Fixture {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/tienda");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "personal", None)
            .unwrap();
        f
    }

    fn answer(f: &Fixture, text: &str) {
        f.agents.reply_on_start(vec![
            AgentEvent::Usage(UsageSample {
                input_tokens: 900,
                output_tokens: 30,
                ..Default::default()
            }),
            AgentEvent::Finished {
                ok: true,
                summary: text.into(),
                duration_ms: None,
            },
        ]);
    }

    fn task(f: &Fixture) -> TaskId {
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir("/code/tienda"),
                prompt: "Ficha de cliente".into(),
                mode: TaskMode::Project,
                model: Some("sonnet".into()),
                plan_first: false,
            })
            .unwrap()
            .id
    }

    #[test]
    fn a_tasks_change_is_checked_against_its_projects_rules() {
        let f = setup();
        let tienda = f.dir("/code/tienda");
        f.hedy
            .add_note(
                Some(&tienda),
                "Las variables van en camelCase",
                Category::Convention,
            )
            .unwrap();
        f.hedy
            .propose_note(
                &tienda,
                "Todo en snake_case",
                Category::Convention,
                "Claude",
                None,
            )
            .unwrap();
        f.git.set_diff(change());
        let id = task(&f);
        answer(
            &f,
            "BREAK \"src/api/clientes.ts\":14 [1] `user_name` no es camelCase => userName\n\
             BREAK \".env\":2 [1] una clave",
        );

        let check = f
            .hedy
            .check_rules(&RuleTarget::Task(id), None, Some("haiku"))
            .unwrap();
        assert_eq!(check.breaks.len(), 1);
        let found = &check.breaks[0];
        assert_eq!(
            (found.path.as_str(), found.line, found.rule),
            ("src/api/clientes.ts", 14, 1)
        );
        assert_eq!(found.fix.as_deref(), Some("userName"));
        // A proposal is not a rule yet.
        let texts: Vec<&str> = check.rules.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["Las variables van en camelCase"]);
        assert_eq!(check.files, 1);
        assert_eq!(check.protected, [".env"]);
        assert_eq!(check.account.unwrap().id.as_str(), "personal");

        let request = f.agents.last_request();
        assert_eq!(
            (request.mode, request.phase),
            (TaskMode::Ask, Phase::Review)
        );
        assert_eq!(request.model.as_deref(), Some("haiku"));
        // No file, no shell, no memory tools: the change is in the request.
        assert_eq!(request.launch.cwd.as_deref(), Some(Path::new("/data/ask")));
        assert!(request.no_shell);
        assert!(request.memory.is_none() && request.mcp.is_empty());
        assert!(
            request
                .prompt
                .contains("+    14 | const user_name = data.user_name;")
        );
        assert!(
            request
                .prompt
                .contains("File \".env\" (modified): protected file")
        );
        assert!(!request.prompt.contains("sk_live"));
        assert!(!request.prompt.contains("snake_case\" "));
    }

    #[test]
    fn nothing_is_sent_without_rules_and_an_empty_change_is_said() {
        let f = setup();
        f.git.set_diff(change());
        let id = task(&f);
        let calls = f.agents.calls().len();
        let check = f
            .hedy
            .check_rules(&RuleTarget::Task(id), None, None)
            .unwrap();
        assert!(check.breaks.is_empty() && check.rules.is_empty());
        assert!(check.account.is_none());
        assert_eq!(f.agents.calls().len(), calls);

        f.git.set_diff(Vec::new());
        assert_eq!(
            f.hedy.check_rules(
                &RuleTarget::Revs {
                    dir: f.dir("/code/tienda"),
                    revs: "HEAD".into()
                },
                None,
                None
            ),
            Err(AppError::RulesNoChanges)
        );
        assert!(f.git.log().contains(&"diff HEAD".to_owned()));
    }

    #[test]
    fn a_range_that_could_pass_for_an_option_is_refused() {
        let f = setup();
        for revs in ["--output=/tmp/x", "", "main HEAD", "a\nb"] {
            assert_eq!(
                f.hedy.check_rules(
                    &RuleTarget::Revs {
                        dir: f.dir("/code/tienda"),
                        revs: revs.into()
                    },
                    None,
                    None
                ),
                Err(AppError::RulesBadRevs(revs.trim().into()))
            );
        }
        assert!(f.git.log().is_empty());
    }

    #[test]
    fn a_protected_projects_rules_only_go_to_its_own_account() {
        let f = setup();
        f.add_claude("Trabajo");
        f.fs.add_git_root("/code/banco");
        let banco = f.dir("/code/banco");
        f.hedy.use_account(&banco, "trabajo", Some(true)).unwrap();
        f.hedy
            .add_note(Some(&banco), "Los importes van en céntimos", Category::Rule)
            .unwrap();
        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
        f.git.set_staged(change());
        let staged = RuleTarget::Staged(banco.clone());
        assert!(matches!(
            f.hedy.check_rules(&staged, Some("personal"), None),
            Err(AppError::Domain(_))
        ));
        answer(&f, "NO_BREAKS");
        let check = f.hedy.check_rules(&staged, None, None).unwrap();
        assert!(check.breaks.is_empty());
        assert_eq!(check.account.unwrap().id.as_str(), "trabajo");
        // A protected project reads no global note either (as its agents).
        let texts: Vec<&str> = check.rules.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["Los importes van en céntimos"]);
    }

    #[test]
    fn an_unknown_folder_uses_the_notes_for_every_project() {
        let f = setup();
        f.fs.add_git_root("/code/suelto");
        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
        f.hedy
            .add_note(
                Some(&f.dir("/code/tienda")),
                "Las variables van en camelCase",
                Category::Convention,
            )
            .unwrap();
        f.git.set_staged(change());
        answer(&f, "NO_BREAKS");
        let check = f
            .hedy
            .check_rules(&RuleTarget::Staged(f.dir("/code/suelto")), None, None)
            .unwrap();
        let texts: Vec<&str> = check.rules.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["Commits en una línea"]);
        assert_eq!(check.account.unwrap().id.as_str(), "personal");
    }

    #[test]
    fn an_account_at_its_limit_is_refused_and_no_other_is_used() {
        let f = setup();
        f.add_claude("Trabajo");
        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
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
        f.git.set_staged(change());
        assert_eq!(
            f.hedy
                .check_rules(&RuleTarget::Staged(f.dir("/code/tienda")), None, None),
            Err(AppError::AskAtLimit {
                account: "Personal".into()
            })
        );
        assert!(f.agents.calls().is_empty());
    }
}
