//! Asking the memory (docs/decisions.md §36): a question answered only from
//! the notes the user can read here (their own, their projects', their
//! teams'), each claim citing its note. One read-only request to one
//! account, tools denied (like "Sugerir con IA"); nothing is sent when no
//! note can answer, and a protected project's notes only go to its own
//! account (rule 4).

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::one_shot::{OneShot, OneShotFailure};
use super::{AppError, AppResult, Hedy};
use crate::domain::ask::{
    CodeAnswer, ParsedAnswer, QUESTION_MAX, Source, SourceLayer, SourceRef, code_prompt,
    parse_answer, parse_code_answer, pick_sources, question_prompt,
};
use crate::domain::sync::MemoryHome;
use crate::domain::team::reveal_hidden;
use crate::domain::{
    Account, AccountId, Category, Note, NoteStatus, Phase, Project, Provider, Stage, policy,
};
use crate::tr;

/// How long an answer may take.
const ASK_TIMEOUT: Duration = Duration::from_secs(120);
/// How long reading the project for an answer may take.
const CODE_TIMEOUT: Duration = Duration::from_secs(240);

/// Which memory a question is asked of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskScope {
    /// Everything this person can read here: their own notes, every
    /// project's, every team's.
    Mine,
    /// What an agent in the project reads: its notes, the global ones and
    /// its team's.
    Project(PathBuf),
    /// One team's notes, by its id.
    Team(String),
}

/// The answer, with every note sent (numbered from 1) and the ones it cites.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryAnswer {
    /// `None`: nothing written answers the question.
    pub text: Option<String>,
    pub sources: Vec<Source>,
    pub cited: Vec<usize>,
    /// Whose tokens it spent; `None` when nothing was sent.
    pub account: Option<Account>,
}

/// What the project's files say when the memory has no answer, and the
/// note proposed from it (rule 6: the user approves it, or not).
#[derive(Debug, Clone, PartialEq)]
pub struct CodeLookup {
    /// `None`: the files do not answer it either.
    pub text: Option<String>,
    pub proposal: Option<Note>,
    pub account: Account,
}

impl Hedy {
    /// "Buscar en el código": the project's account reads its files (only
    /// reads: Ask mode, protected files denied) for an answer the memory
    /// does not have, and proposes the note that would have answered it.
    pub fn look_in_code(
        &self,
        question: &str,
        project: &Path,
        account: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<CodeLookup> {
        self.look_in_code_within(question, project, account, model, CODE_TIMEOUT)
    }

    pub(super) fn look_in_code_within(
        &self,
        question: &str,
        project: &Path,
        account: Option<&str>,
        model: Option<&str>,
        timeout: Duration,
    ) -> AppResult<CodeLookup> {
        let question: String = question.trim().chars().take(QUESTION_MAX).collect();
        if question.is_empty() {
            return Err(AppError::AskEmpty);
        }
        let (account, dir) = self.ask_account(&AskScope::Project(project.to_owned()), account)?;
        // Only Claude Code keeps an agent off protected files (its deny
        // rules, Hedy's hook, no shell); other CLIs read without asking.
        if account.provider != Provider::Claude {
            return Err(AppError::AskCodeNeedsClaude {
                account: account.name.clone(),
            });
        }
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
            cwd: dir.clone(),
            prompt: code_prompt(&question),
            refusal: tr!(
                "No cambies nada: solo lee los archivos del proyecto.",
                "Change nothing: only read the project's files."
            ),
            no_shell: true,
            no_tools: false,
            timeout,
        })?;
        let CodeAnswer { text, note } = match answer {
            Ok(raw) => parse_code_answer(&raw),
            Err(OneShotFailure::AtLimit) => {
                return Err(AppError::AskAtLimit {
                    account: account.name.clone(),
                });
            }
            Err(OneShotFailure::Failed(detail)) => return Err(AppError::AskFailed(detail)),
            Err(OneShotFailure::TimedOut) => return Err(AppError::AskTimedOut),
        };
        let proposal = match (&text, note) {
            (Some(_), Some(note)) => {
                Some(self.propose_note(&dir, &note, Category::Convention, &account.name, None)?)
            }
            _ => None,
        };
        Ok(CodeLookup {
            text,
            proposal,
            account,
        })
    }

    /// Answers `question` from the memory of `scope` through `account`
    /// (else the project's, else the default one), never another account
    /// (rule 3). Spends a few of its tokens, only when some note may answer.
    pub fn ask_memory(
        &self,
        question: &str,
        scope: &AskScope,
        account: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<MemoryAnswer> {
        self.ask_memory_within(question, scope, account, model, ASK_TIMEOUT)
    }

    pub(super) fn ask_memory_within(
        &self,
        question: &str,
        scope: &AskScope,
        account: Option<&str>,
        model: Option<&str>,
        timeout: Duration,
    ) -> AppResult<MemoryAnswer> {
        let question: String = question.trim().chars().take(QUESTION_MAX).collect();
        if question.is_empty() {
            return Err(AppError::AskEmpty);
        }
        // The notes first: with none to send, no account is needed.
        let sources = pick_sources(&question, self.ask_sources(scope)?);
        if sources.is_empty() {
            return Ok(MemoryAnswer {
                text: None,
                sources,
                cited: Vec::new(),
                account: None,
            });
        }
        let (account, _) = self.ask_account(scope, account)?;
        // It needs no file: an empty folder, even for a project's question.
        let cwd = self.ask_dir()?;
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
            cwd,
            prompt: question_prompt(&question, &sources),
            refusal: tr!(
                "No uses herramientas: responde solo con las notas que te di.",
                "Do not use tools: answer only from the notes you were given."
            ),
            no_shell: true,
            no_tools: false,
            timeout,
        })?;
        let ParsedAnswer { text, cited } = match answer {
            Ok(raw) => parse_answer(&raw, sources.len()),
            Err(OneShotFailure::AtLimit) => {
                return Err(AppError::AskAtLimit {
                    account: account.name.clone(),
                });
            }
            Err(OneShotFailure::Failed(detail)) => return Err(AppError::AskFailed(detail)),
            Err(OneShotFailure::TimedOut) => return Err(AppError::AskTimedOut),
        };
        Ok(MemoryAnswer {
            text,
            sources,
            cited,
            account: Some(account),
        })
    }

    /// The account that answers, checked like a launch, and the folder it
    /// runs in.
    pub(super) fn ask_account(
        &self,
        scope: &AskScope,
        account: Option<&str>,
    ) -> AppResult<(Account, PathBuf)> {
        if let AskScope::Project(path) = scope {
            let resolved = match self.resolve_account(path, account) {
                // No account for the project and no default: the only one.
                Err(AppError::NoAccountResolved) if account.is_none() => {
                    match self.only_account()? {
                        Some(only) => self.resolve_account(path, Some(only.as_str())),
                        None => Err(AppError::NoAccountResolved),
                    }
                }
                other => other,
            };
            let (project, account) = resolved?;
            let project = project.ok_or_else(|| AppError::ProjectNotFound(path.clone()))?;
            return Ok((account, project.path));
        }
        let id = match account {
            Some(id) => self.account(id)?.id,
            None => self
                .usual_account(scope)?
                .ok_or(AppError::NoAccountResolved)?,
        };
        let account = self.account(id.as_str())?;
        policy::check_launch(None, &account.id, &self.projects()?)?;
        Ok((account, self.ask_dir()?))
    }

    /// An empty folder of its own: no project's or conversation's files.
    pub(super) fn ask_dir(&self) -> AppResult<PathBuf> {
        let dir = self.fs.chats_dir().with_file_name("ask");
        self.fs.create_private_dir(&dir)?;
        Ok(dir)
    }

    /// The account a question outside a project goes to when none is
    /// chosen: for a team, that of a project of the team here; else the
    /// default one; else the only one.
    fn usual_account(&self, scope: &AskScope) -> AppResult<Option<AccountId>> {
        if let AskScope::Team(team) = scope {
            let home = MemoryHome::Team(team.clone());
            for project in self.projects()? {
                if !project.protected
                    && project.account.is_some()
                    && self.store.memory_home(&project.path)?.as_ref() == Some(&home)
                {
                    return Ok(project.account);
                }
            }
        }
        match self.default_account_id()? {
            Some(id) => Ok(Some(id)),
            None => self.only_account(),
        }
    }

    fn only_account(&self) -> AppResult<Option<AccountId>> {
        let accounts = self.accounts()?;
        Ok((accounts.len() == 1).then(|| accounts[0].id.clone()))
    }

    /// Every note of `scope` that may go to its account. A protected
    /// project's go only to its own account, which answers nowhere else
    /// (rule 4): only a question in that project reads them, its team notes
    /// included (another copy here of the same repo does not).
    pub(super) fn ask_sources(&self, scope: &AskScope) -> AppResult<Vec<Source>> {
        let projects = self.projects()?;
        let allowed = |path: &Path| {
            projects
                .iter()
                .find(|p| p.path == path)
                .is_none_or(|p| !p.protected)
        };
        let mut sources = Vec::new();
        match scope {
            AskScope::Project(path) => {
                let project = self
                    .project_for(path)?
                    .ok_or_else(|| AppError::ProjectNotFound(path.clone()))?;
                let notes = self
                    .store
                    .notes(Some(&project.path), Self::with_global(&project))?;
                sources.extend(notes.into_iter().filter_map(|n| local_source(n, &projects)));
                if let Some(team) = self.project_team_id(&project.path) {
                    let own = self.team_project_id_of(&team, &project.path);
                    let guard = self.team_protection(&team)?;
                    // Another copy here of a protected project keeps its notes to it.
                    let mine = |id: &str| {
                        own.as_deref() == Some(id) && (project.protected || guard.allows(id))
                    };
                    sources.extend(self.team_sources(&team, mine)?);
                }
            }
            AskScope::Team(team) => {
                let guard = self.team_protection(team)?;
                if guard.only_protected {
                    return Err(AppError::AskTeamProtected);
                }
                sources.extend(self.team_sources(team, |id| guard.allows(id))?);
            }
            AskScope::Mine => {
                let mut notes = self.store.notes(None, false)?;
                for project in projects.iter().filter(|p| allowed(&p.path)) {
                    notes.extend(self.store.notes(Some(&project.path), false)?);
                }
                sources.extend(notes.into_iter().filter_map(|n| local_source(n, &projects)));
                for team in self.teams()?.into_iter().filter(|t| !t.waiting) {
                    // A team only protected projects hold stays out; one this
                    // PC cannot read now does not stop the rest.
                    if let Ok(guard) = self.team_protection(&team.id)
                        && !guard.only_protected
                    {
                        sources.extend(
                            self.team_sources(&team.id, |id| guard.allows(id))
                                .unwrap_or_default(),
                        );
                    }
                }
            }
        }
        Ok(sources)
    }

    /// A team's notes: its layer, and the project notes `keep` accepts by
    /// their team project id.
    fn team_sources(&self, team: &str, keep: impl Fn(&str) -> bool) -> AppResult<Vec<Source>> {
        let name = self.team_status(team)?.summary.name;
        let projects = self.team_note_projects(team)?;
        Ok(self
            .team_notes(team)?
            .into_iter()
            // One synced in between, of no known project, stays out.
            .filter(|n| {
                n.team_layer
                    || projects
                        .get(&n.uid)
                        .and_then(|id| id.as_deref())
                        .is_some_and(&keep)
            })
            .map(|n| Source {
                layer: if n.team_layer {
                    SourceLayer::Team
                } else {
                    SourceLayer::Project
                },
                project: n.project.as_deref().map(folder_name),
                // Shown as written, no hidden character reordering it.
                text: reveal_hidden(&n.text),
                category: n.category,
                topic: n.topic,
                author: Some(reveal_hidden(&n.author)),
                team: Some(name.clone()),
                at: n.updated_at.unwrap_or(n.created_at),
                note: SourceRef::Team {
                    team: team.to_owned(),
                    uid: n.uid,
                },
            })
            .collect())
    }
}

/// A note of this PC as a source; proposals are not memory yet.
pub(super) fn local_source(note: crate::domain::Note, projects: &[Project]) -> Option<Source> {
    (note.status == NoteStatus::Approved).then(|| Source {
        project: note.project.as_deref().map(|p| {
            projects
                .iter()
                .find(|known| known.path == p)
                .map_or_else(|| folder_name(p), |known| known.name.clone())
        }),
        text: reveal_hidden(&note.text),
        category: note.category,
        topic: note.topic,
        layer: SourceLayer::Mine,
        author: None,
        team: None,
        at: note.updated_at.unwrap_or(note.created_at),
        note: SourceRef::Local { id: note.id },
    })
}

fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{RateLimit, UsageRepository};
    use crate::application::testing::Fixture;
    use crate::domain::{AgentEvent, Category, TaskMode, UsageSample};

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
                input_tokens: 600,
                output_tokens: 40,
                ..Default::default()
            }),
            AgentEvent::Finished {
                ok: true,
                summary: text.into(),
                duration_ms: None,
            },
        ]);
    }

    #[test]
    fn a_project_question_is_answered_from_its_notes_with_citations() {
        let f = setup();
        let tienda = f.dir("/code/tienda");
        f.hedy
            .add_note(
                Some(&tienda),
                "El fondo oscuro es #0F1117",
                Category::Convention,
            )
            .unwrap();
        f.hedy
            .add_note(None, "Responde en español", Category::Preference)
            .unwrap();
        f.hedy
            .propose_note(
                &tienda,
                "El fondo es negro puro",
                Category::Convention,
                "Claude",
                None,
            )
            .unwrap();
        answer(&f, "El fondo oscuro es #0F1117 [1] [8].");

        let a = f
            .hedy
            .ask_memory(
                "¿Qué color de fondo usamos?",
                &AskScope::Project(tienda.clone()),
                None,
                Some("haiku"),
            )
            .unwrap();
        assert_eq!(a.text.as_deref(), Some("El fondo oscuro es #0F1117 [1]."));
        assert_eq!(a.cited, [1]);
        assert_eq!(a.account.unwrap().id.as_str(), "personal");
        let texts: Vec<&str> = a.sources.iter().map(|s| s.text.as_str()).collect();
        // A proposal is not memory yet.
        assert_eq!(texts, ["El fondo oscuro es #0F1117", "Responde en español"]);
        assert_eq!(a.sources[0].project.as_deref(), Some("tienda"));
        assert_eq!(a.sources[0].layer, SourceLayer::Mine);

        let request = f.agents.last_request();
        assert_eq!(
            (request.mode, request.phase),
            (TaskMode::Ask, Phase::Review)
        );
        assert_eq!(request.model.as_deref(), Some("haiku"));
        assert!(request.memory.is_none() && request.mcp.is_empty());
        // It reads no file: an empty folder, not the project's.
        assert_eq!(request.launch.cwd.as_deref(), Some(Path::new("/data/ask")));
        assert!(request.no_shell);
        assert!(
            request
                .prompt
                .contains("Question: \"¿Qué color de fondo usamos?\"")
        );
        assert!(request.prompt.contains(
            "[1] where=\"own note, project tienda\" category=convention text=\"El fondo oscuro es #0F1117\""
        ));
        assert!(!request.prompt.contains("negro puro"));
    }

    #[test]
    fn nothing_is_sent_when_no_note_can_answer() {
        let f = setup();
        let a = f
            .hedy
            .ask_memory(
                "¿Qué tipografía usan los correos?",
                &AskScope::Mine,
                Some("personal"),
                None,
            )
            .unwrap();
        assert_eq!(a.text, None);
        assert!(a.sources.is_empty() && a.account.is_none());
        assert!(f.agents.calls().is_empty());
        // Outside a project it runs in an empty folder of Hedy's own.
        assert!(!f.fs.dirs().contains(Path::new("/data/ask")));

        f.hedy
            .add_note(None, "Commits en una línea", Category::Convention)
            .unwrap();
        answer(&f, "NOT_WRITTEN");
        let a = f
            .hedy
            .ask_memory(
                "¿Qué tipografía usan los correos?",
                &AskScope::Mine,
                Some("personal"),
                None,
            )
            .unwrap();
        assert_eq!(a.text, None);
        assert_eq!(a.sources.len(), 1);
        assert!(a.account.is_some());
        // Outside a project it runs in an empty folder of its own.
        assert_eq!(
            f.agents.last_request().launch.cwd.as_deref(),
            Some(Path::new("/data/ask"))
        );
        assert!(f.fs.dirs().contains(Path::new("/data/ask")));
    }

    #[test]
    fn a_protected_projects_notes_only_go_to_its_own_account() {
        let f = setup();
        f.add_claude("Trabajo");
        f.fs.add_git_root("/code/banco");
        let banco = f.dir("/code/banco");
        f.hedy.use_account(&banco, "trabajo", Some(true)).unwrap();
        f.hedy
            .add_note(
                Some(&banco),
                "Las claves del banco rotan cada lunes",
                Category::Rule,
            )
            .unwrap();
        f.hedy
            .add_note(
                Some(&f.dir("/code/tienda")),
                "Las claves de tienda no rotan",
                Category::Rule,
            )
            .unwrap();

        answer(&f, "No rotan [1].");
        let a = f
            .hedy
            .ask_memory(
                "¿Cuándo rotan las claves?",
                &AskScope::Mine,
                Some("personal"),
                None,
            )
            .unwrap();
        assert!(a.sources.iter().all(|s| !s.text.contains("banco")));
        assert!(!f.agents.last_request().prompt.contains("banco"));
        // Its account answers nowhere else, and nobody else answers there.
        assert!(matches!(
            f.hedy
                .ask_memory("¿Claves?", &AskScope::Mine, Some("trabajo"), None),
            Err(AppError::Domain(_))
        ));
        assert!(matches!(
            f.hedy.ask_memory(
                "¿Claves?",
                &AskScope::Project(banco.clone()),
                Some("personal"),
                None
            ),
            Err(AppError::Domain(_))
        ));
        answer(&f, "Cada lunes [1].");
        let a = f
            .hedy
            .ask_memory(
                "¿Cuándo rotan las claves?",
                &AskScope::Project(banco),
                None,
                None,
            )
            .unwrap();
        assert_eq!(a.account.unwrap().id.as_str(), "trabajo");
        assert!(a.sources.iter().any(|s| s.text.contains("banco")));
        assert!(a.sources.iter().all(|s| !s.text.contains("tienda")));
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
        assert_eq!(
            f.hedy.ask_memory(
                "¿Cómo son los commits?",
                &AskScope::Project(f.dir("/code/tienda")),
                None,
                None
            ),
            Err(AppError::AskAtLimit {
                account: "Personal".into()
            })
        );
        assert!(f.agents.calls().is_empty());
        assert_eq!(
            f.hedy.ask_memory("   ", &AskScope::Mine, None, None),
            Err(AppError::AskEmpty)
        );
    }

    #[test]
    fn the_code_answers_and_proposes_the_note_the_user_approves() {
        let f = setup();
        let tienda = f.dir("/code/tienda");
        answer(
            &f,
            "El fondo es #0F1117, en `src/theme.ts`.\nNOTE: El fondo oscuro de tienda es #0F1117 (src/theme.ts).",
        );
        let found = f
            .hedy
            .look_in_code("¿Qué fondo usamos?", &tienda, None, Some("haiku"))
            .unwrap();
        assert_eq!(
            found.text.as_deref(),
            Some("El fondo es #0F1117, en `src/theme.ts`.")
        );
        let proposal = found.proposal.unwrap();
        assert_eq!(proposal.status, NoteStatus::Proposed);
        assert_eq!(proposal.project.as_deref(), Some(tienda.as_path()));
        assert_eq!(proposal.author, "Personal");
        let request = f.agents.last_request();
        assert_eq!(
            (request.mode, request.phase),
            (TaskMode::Ask, Phase::Review)
        );
        assert_eq!(request.launch.cwd.as_deref(), Some(tienda.as_path()));
        assert!(request.prompt.contains("only read them, change nothing"));
        // No shell: it could read a protected file without naming it.
        assert!(request.no_shell);

        answer(&f, "NOT_FOUND");
        let none = f
            .hedy
            .look_in_code("¿Tipografía?", &tienda, None, None)
            .unwrap();
        assert_eq!((none.text, none.proposal), (None, None));
        assert_eq!(f.hedy.memory(&tienda).unwrap().proposed.len(), 1);

        // Another CLI could read a protected file without asking.
        f.add_codex("Codex");
        assert_eq!(
            f.hedy.look_in_code("¿Fondo?", &tienda, Some("codex"), None),
            Err(AppError::AskCodeNeedsClaude {
                account: "Codex".into()
            })
        );
    }
}
