//! Checking a change against the written rules (docs/decisions.md §37): a
//! task's changes, what is staged or a range of commits, judged by the
//! notes an agent in that project reads, each break citing its note. One
//! read-only request (tools and shell denied, an empty folder), like asking
//! the memory; nothing is sent without rules or lines to judge, protected
//! files go only by name, and it only warns: it never blocks.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::ask::{AskScope, local_source};
use super::one_shot::{OneShot, OneShotFailure};
use super::{AppError, AppResult, Hedy};
use crate::domain::ask::Source;
use crate::domain::project::find_for_dir;
use crate::domain::rule_check::{
    RuleBreak, answer_is_readable, change_for_prompt, parse_breaks, pick_rules, rules_prompt,
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
    /// What is not committed yet there, staged or not, new files included.
    Work(PathBuf),
    /// Commits there: a range (`main..HEAD`) or one commit's own change
    /// (`a1b2c3`, `HEAD~1`).
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
    /// The answer said neither what breaks a rule nor that nothing does:
    /// no break found does not mean the change is clean.
    pub unread: bool,
    /// Whose tokens it spent; `None` when nothing was sent.
    pub account: Option<Account>,
}

impl RuleCheck {
    /// How it ended, for a script or the app: `no_rules` and
    /// `nothing_to_check` sent nothing; `unread` is not a clean change.
    pub fn status(&self) -> &'static str {
        if self.rules.is_empty() {
            "no_rules"
        } else if self.account.is_none() {
            "nothing_to_check"
        } else if self.unread {
            "unread"
        } else {
            "checked"
        }
    }
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
        let (project, diff, left) = self.rule_change(target)?;
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
            // New files left unread mean part of the change was not judged.
            truncated: change.truncated || left > 0,
            unread: false,
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
        let (breaks, readable) = match answer {
            Ok(raw) => {
                let breaks = parse_breaks(&raw, &change, rules.len());
                let readable = answer_is_readable(&raw, &breaks);
                (breaks, readable)
            }
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
            unread: !readable,
            account: Some(account),
            ..nothing(rules)
        })
    }

    /// The project whose rules apply (`None`: a folder Hedy does not know)
    /// and the change. In a folder, the project is the folder's (as for a
    /// launch), and only its files count when it is a part of the
    /// repository; a change to another protected project inside it is
    /// refused (rule 4: its code only goes to its own account).
    /// And how many new files were left unread.
    fn rule_change(
        &self,
        target: &RuleTarget,
    ) -> AppResult<(Option<PathBuf>, Vec<FileDiff>, usize)> {
        let (dir, roots, diff, left) = match target {
            RuleTarget::Task(id) => {
                let task = self.task(id)?;
                if task.status.is_closed() {
                    return Err(AppError::TaskClosed(id.clone()));
                }
                // Its worktree is a copy of the project's repository.
                let repo = self.repo_root(&task.project)?;
                (task.project.clone(), vec![repo], self.task_diff(id)?, 0)
            }
            RuleTarget::Staged(dir) | RuleTarget::Work(dir) | RuleTarget::Revs { dir, .. } => {
                let revs = match target {
                    RuleTarget::Revs { revs, .. } => Some(plain_revs(revs)?),
                    _ => None,
                };
                let worktree = self.repo_root(dir)?;
                let (diff, left) = match (target, revs) {
                    (RuleTarget::Staged(_), _) => (self.git.staged_diff(&worktree)?, 0),
                    (_, Some(revs)) => (self.git.revs_diff(&worktree, &revs)?, 0),
                    _ => self.git.work_diff(&worktree)?,
                };
                let main = self.git.main_root(&worktree)?;
                let dir = self.fs.canonicalize(dir)?;
                let projects = self.projects()?;
                // A project registered in the worktree itself (a second
                // checkout used as a project of its own) is the folder's;
                // else, in a worktree (a task's, or one of the user's), the
                // projects are where its main checkout has the same folder.
                let own =
                    find_for_dir(&projects, &dir).is_some_and(|p| p.path.starts_with(&worktree));
                let dir = if own {
                    dir
                } else {
                    main.join(dir.strip_prefix(&worktree).unwrap_or(Path::new("")))
                };
                let mut roots = vec![worktree];
                if !roots.contains(&main) {
                    roots.push(main);
                }
                (dir, roots, diff, left)
            }
        };
        let projects = self.projects()?;
        let project = find_for_dir(&projects, &dir).cloned();
        // A project's folder in the change, by its place in either checkout.
        // The first root holding it decides (the worktree's own first): a
        // project at a worktree's root is that whole checkout.
        let inside = |path: &Path| {
            roots
                .iter()
                .find_map(|root| path.strip_prefix(root).ok())
                .filter(|rel| !rel.as_os_str().is_empty())
                .map(|rel| format!("{}/", rel.to_string_lossy()))
        };
        let under = |file: &FileDiff, folder: &str| {
            file.path.starts_with(folder)
                || file
                    .old_path
                    .as_deref()
                    .is_some_and(|p| p.starts_with(folder))
        };
        let diff: Vec<FileDiff> = match project.as_ref().and_then(|p| inside(&p.path)) {
            Some(folder) => diff.into_iter().filter(|f| under(f, &folder)).collect(),
            None => diff,
        };
        for other in projects.iter().filter(|p| p.protected) {
            if project.as_ref().is_some_and(|p| p.path == other.path) {
                continue;
            }
            if let Some(folder) = inside(&other.path)
                && diff.iter().any(|f| under(f, &folder))
            {
                return Err(AppError::RulesProtectedInside {
                    project: other.name.clone(),
                });
            }
        }
        Ok((project.map(|p| p.path), diff, left))
    }
}

/// A range or a commit as git reads it, never as an option or a file:
/// `rev:path` (a file's contents) is refused, since it could compare a
/// protected file.
fn plain_revs(revs: &str) -> AppResult<String> {
    let revs = revs.trim();
    let plain = !revs.is_empty()
        && revs.len() <= REVS_MAX
        && !revs.starts_with('-')
        && !revs.contains(':')
        && !revs.chars().any(|c| c.is_whitespace() || c.is_control());
    if !plain {
        return Err(AppError::RulesBadRevs(revs.to_owned()));
    }
    Ok(revs.to_owned())
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
        assert!(!request.prompt.contains("Todo en snake_case"));
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
            f.hedy
                .check_rules(&RuleTarget::Work(f.dir("/code/tienda")), None, None),
            Err(AppError::RulesNoChanges)
        );
        // New files past the most read: only part of the change is judged.
        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
        f.git.set_diff(change());
        f.git.set_work_left(120);
        answer(&f, "NO_BREAKS");
        let check = f
            .hedy
            .check_rules(&RuleTarget::Work(f.dir("/code/tienda")), None, None)
            .unwrap();
        assert!(check.truncated);
        assert!(f.git.log().contains(&"diff work".to_owned()));
    }

    #[test]
    fn a_range_that_could_pass_for_an_option_is_refused() {
        let f = setup();
        for revs in [
            "--output=/tmp/x",
            "",
            "main HEAD",
            "a\nb",
            // A file's contents: it could compare a protected one.
            "HEAD:.env..HEAD:notes.txt",
            ":0:.env..:x",
        ] {
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
        for (revs, sent) in [("HEAD~1", "diff HEAD~1"), ("main..HEAD", "diff main..HEAD")] {
            let target = RuleTarget::Revs {
                dir: f.dir("/code/tienda"),
                revs: revs.into(),
            };
            assert_eq!(
                f.hedy.check_rules(&target, None, None),
                Err(AppError::RulesNoChanges)
            );
            assert_eq!(f.git.log().last().map(String::as_str), Some(sent));
        }
    }

    fn at(path: &str) -> FileDiff {
        let mut file = change().remove(0);
        file.path = path.into();
        file
    }

    #[test]
    fn a_project_inside_a_repository_checks_only_its_files_and_guards_protected_ones() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        // Registered before the parent folder became a repository.
        f.hedy
            .use_account(&f.dir("/code/mono/banco"), "trabajo", Some(true))
            .unwrap();
        f.fs.add_git_root("/code/mono");
        f.hedy
            .set_setting(
                crate::application::Setting::DefaultAccount,
                Some("personal"),
            )
            .unwrap();
        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
        f.hedy
            .add_note(
                Some(&f.dir("/code/mono/banco")),
                "Los importes van en céntimos",
                Category::Rule,
            )
            .unwrap();
        f.git
            .set_staged(vec![at("banco/src/pagos.ts"), at("web/src/inicio.ts")]);

        // From the protected project: only its files, to its own account.
        answer(&f, "NO_BREAKS");
        let check = f
            .hedy
            .check_rules(&RuleTarget::Staged(f.dir("/code/mono/banco")), None, None)
            .unwrap();
        assert_eq!(check.account.unwrap().id.as_str(), "trabajo");
        let prompt = f.agents.last_request().prompt;
        assert!(prompt.contains("banco/src/pagos.ts") && !prompt.contains("inicio.ts"));

        // From the rest of the repository: its change touches the protected
        // project, whose code never goes to another account.
        let calls = f.agents.calls().len();
        assert_eq!(
            f.hedy
                .check_rules(&RuleTarget::Staged(f.dir("/code/mono")), None, None),
            Err(AppError::RulesProtectedInside {
                project: "banco".into()
            })
        );
        assert_eq!(f.agents.calls().len(), calls);
        // Without its files, the rest is checked as usual.
        f.git.set_staged(vec![at("web/src/inicio.ts")]);
        answer(&f, "NO_BREAKS");
        let check = f
            .hedy
            .check_rules(&RuleTarget::Staged(f.dir("/code/mono")), None, None)
            .unwrap();
        assert_eq!(check.account.unwrap().id.as_str(), "personal");
    }

    #[test]
    fn an_answer_that_cannot_be_read_is_not_a_clean_change() {
        let f = setup();
        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
        f.git.set_staged(change());
        let staged = RuleTarget::Staged(f.dir("/code/tienda"));
        for (raw, unread) in [
            ("I think it is fine", true),
            ("BREAK \"otro.ts\":3 [1] un archivo que no está", true),
            ("no_breaks", false),
            ("`NO_BREAKS`.", false),
            // The marker does not cover for a break that was dropped.
            ("BREAK \"zzz.js\":1 [1] x\nOtherwise NO_BREAKS", true),
            ("BREAK \"src/api/clientes.ts\":14 [1] snake", false),
        ] {
            answer(&f, raw);
            let check = f.hedy.check_rules(&staged, None, None).unwrap();
            assert_eq!(check.unread, unread, "{raw}");
        }
    }

    #[test]
    fn from_a_worktree_the_projects_are_those_of_its_main_checkout() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/mono/banco"), "trabajo", Some(true))
            .unwrap();
        f.fs.add_git_root("/code/mono");
        f.hedy
            .use_account(&f.dir("/code/mono"), "personal", None)
            .unwrap();
        f.fs.add_git_root("/code/mono/.hedy/worktrees/t1");
        f.git.set_main_root("/code/mono");
        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
        f.hedy
            .add_note(
                Some(&f.dir("/code/mono/banco")),
                "Los importes van en céntimos",
                Category::Rule,
            )
            .unwrap();
        f.git
            .set_staged(vec![at("banco/src/pagos.ts"), at("web/src/inicio.ts")]);
        let worktree = f.dir("/code/mono/.hedy/worktrees/t1");
        assert_eq!(
            f.hedy
                .check_rules(&RuleTarget::Staged(worktree.clone()), None, None),
            Err(AppError::RulesProtectedInside {
                project: "banco".into()
            })
        );
        answer(&f, "NO_BREAKS");
        let check = f
            .hedy
            .check_rules(&RuleTarget::Staged(worktree.join("banco")), None, None)
            .unwrap();
        assert_eq!(check.account.unwrap().id.as_str(), "trabajo");
        let prompt = f.agents.last_request().prompt;
        assert!(prompt.contains("banco/src/pagos.ts") && !prompt.contains("inicio.ts"));
    }

    #[test]
    fn a_worktree_that_is_a_project_of_its_own_keeps_its_account() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.fs.add_git_root("/code/web");
        f.fs.add_git_root("/code/web-wt");
        f.hedy
            .use_account(&f.dir("/code/web"), "personal", None)
            .unwrap();
        f.hedy
            .use_account(&f.dir("/code/web-wt"), "trabajo", Some(true))
            .unwrap();
        f.git.set_main_root("/code/web");
        f.hedy
            .add_note(
                Some(&f.dir("/code/web-wt")),
                "Los importes van en céntimos",
                Category::Rule,
            )
            .unwrap();
        f.git.set_staged(change());
        answer(&f, "NO_BREAKS");
        let check = f
            .hedy
            .check_rules(&RuleTarget::Staged(f.dir("/code/web-wt")), None, None)
            .unwrap();
        assert_eq!(check.account.unwrap().id.as_str(), "trabajo");
        let texts: Vec<&str> = check.rules.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["Los importes van en céntimos"]);

        // One inside the main checkout: its whole change, not "no changes".
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/web/inner"), "trabajo", Some(true))
            .unwrap();
        f.fs.add_git_root("/code/web");
        f.fs.add_git_root("/code/web/inner");
        f.git.set_main_root("/code/web");
        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
        f.hedy
            .add_note(
                Some(&f.dir("/code/web/inner")),
                "Los importes van en céntimos",
                Category::Rule,
            )
            .unwrap();
        f.git.set_staged(change());
        answer(&f, "NO_BREAKS");
        let check = f
            .hedy
            .check_rules(&RuleTarget::Staged(f.dir("/code/web/inner")), None, None)
            .unwrap();
        assert_eq!(check.account.unwrap().id.as_str(), "trabajo");
        assert_eq!(check.files, 1);
    }

    #[test]
    fn a_closed_task_is_said_to_be_closed() {
        let f = setup();
        f.git.set_diff(change());
        let id = task(&f);
        f.hedy.discard_task(&id).unwrap();
        assert_eq!(
            f.hedy
                .check_rules(&RuleTarget::Task(id.clone()), None, None),
            Err(AppError::TaskClosed(id))
        );
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
        assert!(check.breaks.is_empty() && !check.unread);
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
