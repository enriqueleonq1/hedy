//! Project memory: notes every AI of a project reads, which the user curates.
//! Notes from AIs are proposals until the user approves them (spec rule 6).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::ports::{McpServer, MemoryLink, MemoryTools, NewNote};
use super::{AppError, AppResult, Hedy};
use crate::domain::memory::{USER_AUTHOR, contradictions, is_stale};
use crate::domain::{Account, AccountId, Category, Note, NoteStatus, Project, TaskId};

/// Everything the memory screen shows for a project.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryView {
    /// Approved notes: the project's and, unless it is protected, global ones.
    pub notes: Vec<Note>,
    /// Proposed by AIs, waiting for the user.
    pub proposed: Vec<Note>,
    pub contradictions: Vec<(Note, Note)>,
    /// Not used for 60 days: still true?
    pub stale: Vec<Note>,
    /// Notes read during the project's latest task.
    pub used_last_task: Vec<Note>,
    /// Accounts that work in the project, and whether they read the memory.
    pub readers: Vec<(Account, bool)>,
    /// A protected project neither reads nor shares global notes.
    pub protected: bool,
}

/// How the app starts the memory MCP server: program and leading arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryServerCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl Hedy {
    /// Where the memory MCP server comes from (the app binary serves it).
    pub fn with_memory_server(mut self, command: MemoryServerCommand) -> Self {
        self.memory_server = Some(command);
        self
    }

    fn with_global(project: &Project) -> bool {
        !project.protected
    }

    fn note_or_error(&self, id: i64) -> AppResult<Note> {
        self.store.note(id)?.ok_or(AppError::NoteNotFound(id))
    }

    /// Accounts that work in a project: its own and the ones in its phases.
    fn project_agents(&self, project: &Project) -> AppResult<Vec<Account>> {
        let mut ids: Vec<AccountId> = project.account.iter().cloned().collect();
        for assignment in project.phases.values() {
            if !ids.contains(&assignment.account) {
                ids.push(assignment.account.clone());
            }
        }
        Ok(self
            .accounts()?
            .into_iter()
            .filter(|a| ids.contains(&a.id))
            .collect())
    }

    pub fn memory(&self, path: &Path) -> AppResult<MemoryView> {
        let project = self.project_at(path)?;
        let all = self
            .store
            .notes(Some(&project.path), Self::with_global(&project))?;
        let (notes, proposed): (Vec<_>, Vec<_>) = all
            .into_iter()
            .partition(|n| n.status == NoteStatus::Approved);
        let dismissed = self.store.dismissed_pairs()?;
        let find = |id: i64| notes.iter().find(|n| n.id == id).cloned();
        let contradictions = contradictions(&notes, &dismissed)
            .into_iter()
            .filter_map(|(a, b)| Some((find(a)?, find(b)?)))
            .collect();
        let now = self.clock.now_ms() / 1000;
        let stale = notes.iter().filter(|n| is_stale(n, now)).cloned().collect();
        let used_last_task = match self.tasks(&project.path)?.first() {
            Some(task) => self.store.notes_used_by(&task.id)?,
            None => Vec::new(),
        };
        let readers = self
            .project_agents(&project)?
            .into_iter()
            .map(|a| {
                let on = !project.memory_off.contains(&a.id);
                (a, on)
            })
            .collect();
        Ok(MemoryView {
            notes,
            proposed,
            contradictions,
            stale,
            used_last_task,
            readers,
            protected: project.protected,
        })
    }

    /// A note written by the user: approved right away. `None`: a global note.
    pub fn add_note(
        &self,
        project: Option<&Path>,
        text: &str,
        category: Category,
    ) -> AppResult<Note> {
        let project = match project {
            Some(path) => Some(self.project_at(path)?.path),
            None => None,
        };
        self.new_note(project, text, category, USER_AUTHOR, NoteStatus::Approved)
    }

    /// A note suggested by an AI: it waits for the user's approval.
    pub fn propose_note(
        &self,
        project: &Path,
        text: &str,
        category: Category,
        author: &str,
    ) -> AppResult<Note> {
        let project = self.project_at(project)?;
        self.new_note(
            Some(project.path),
            text,
            category,
            author,
            NoteStatus::Proposed,
        )
    }

    fn new_note(
        &self,
        project: Option<PathBuf>,
        text: &str,
        category: Category,
        author: &str,
        status: NoteStatus,
    ) -> AppResult<Note> {
        let text = text.trim();
        if text.is_empty() {
            return Err(AppError::EmptyNote);
        }
        Ok(self.store.add_note(&NewNote {
            project,
            text: text.to_owned(),
            category,
            author: author.to_owned(),
            status,
            created_at: self.clock.now_ms() / 1000,
        })?)
    }

    pub fn approve_note(&self, id: i64) -> AppResult<Note> {
        self.note_or_error(id)?;
        self.store.set_note_status(id, NoteStatus::Approved)?;
        self.note_or_error(id)
    }

    pub fn edit_note(&self, id: i64, text: &str, category: Category) -> AppResult<Note> {
        self.note_or_error(id)?;
        if text.trim().is_empty() {
            return Err(AppError::EmptyNote);
        }
        self.store.update_note(id, text.trim(), category)?;
        self.note_or_error(id)
    }

    /// "Olvidar" (and rejecting a proposal): the note is deleted.
    pub fn forget_note(&self, id: i64) -> AppResult<()> {
        self.note_or_error(id)?;
        Ok(self.store.delete_note(id)?)
    }

    /// Resolves a contradiction: keep one note, forget the other.
    pub fn keep_note(&self, keep: i64, forget: i64) -> AppResult<()> {
        self.note_or_error(keep)?;
        self.forget_note(forget)
    }

    /// "Las dos son válidas": stop showing the pair as a contradiction.
    pub fn dismiss_contradiction(&self, a: i64, b: i64) -> AppResult<()> {
        Ok(self.store.dismiss_pair(a, b)?)
    }

    /// The user's search box: approved and proposed notes in scope.
    pub fn search_memory(&self, path: &Path, query: &str) -> AppResult<Vec<Note>> {
        let project = self.project_at(path)?;
        Ok(self
            .store
            .search_notes(Some(&project.path), Self::with_global(&project), query)?)
    }

    /// "Quién la lee": turn the memory on or off for one account in a project.
    pub fn set_memory_reader(&self, path: &Path, account: &str, reads: bool) -> AppResult<Project> {
        let mut project = self.project_at(path)?;
        let account = self.account(account)?.id;
        if reads {
            project.memory_off.remove(&account);
        } else {
            project.memory_off.insert(account);
        }
        self.store.save_project(&project)?;
        Ok(project)
    }

    /// What an agent reads: approved notes matching `query` (all when empty).
    /// Every note returned counts as used, by this task when known.
    pub fn read_memory_for_agent(
        &self,
        path: &Path,
        task: Option<&TaskId>,
        query: &str,
    ) -> AppResult<Vec<Note>> {
        let notes: Vec<Note> = self
            .search_memory(path, query)?
            .into_iter()
            .filter(|n| n.status == NoteStatus::Approved)
            .take(30)
            .collect();
        let ids: Vec<i64> = notes.iter().map(|n| n.id).collect();
        self.store
            .record_uses(&ids, task, self.clock.now_ms() / 1000)?;
        Ok(notes)
    }

    /// The memory an agent gets for a task, unless the account is turned off.
    pub(super) fn memory_link(
        &self,
        project: &Project,
        task: &TaskId,
        account: &Account,
    ) -> Option<MemoryLink> {
        if project.memory_off.contains(&account.id) {
            return None;
        }
        let author = account.provider.agent_label(None);
        let server = self.memory_server.as_ref().map(|command| McpServer {
            name: "hedy-memory".into(),
            command: command.program.clone(),
            args: command
                .args
                .iter()
                .cloned()
                .chain([
                    "--project".into(),
                    project.path.to_string_lossy().into_owned(),
                    "--task".into(),
                    task.to_string(),
                    "--agent".into(),
                    author.clone(),
                ])
                .collect(),
            env: Vec::new(),
        });
        Some(MemoryLink {
            server,
            tools: Arc::new(TaskMemory {
                hedy: self.clone(),
                project: project.path.clone(),
                task: task.clone(),
                author,
            }),
        })
    }
}

/// Text an agent reads for a list of notes.
pub fn format_notes(notes: &[Note]) -> String {
    if notes.is_empty() {
        return "No hay notas sobre eso en la memoria del proyecto.".into();
    }
    notes
        .iter()
        .map(|n| {
            let scope = if n.project.is_some() {
                "proyecto"
            } else {
                "global"
            };
            format!("- [{}·{scope}] {}", n.category.as_str(), n.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Memory tools for an agent that runs its tools through Hedy (API agents).
struct TaskMemory {
    hedy: Hedy,
    project: PathBuf,
    task: TaskId,
    author: String,
}

impl MemoryTools for TaskMemory {
    fn search(&self, query: &str) -> String {
        match self
            .hedy
            .read_memory_for_agent(&self.project, Some(&self.task), query)
        {
            Ok(notes) => format_notes(&notes),
            Err(e) => format!("No se pudo leer la memoria: {e}"),
        }
    }

    fn propose(&self, text: &str, category: &str) -> String {
        let category = category.parse().unwrap_or(Category::Convention);
        match self
            .hedy
            .propose_note(&self.project, text, category, &self.author)
        {
            Ok(_) => "Propuesta guardada; el usuario la revisará.".into(),
            Err(e) => format!("No se pudo proponer la nota: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::testing::Fixture;

    fn setup() -> Fixture {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        f
    }

    #[test]
    fn user_notes_are_approved_and_ai_notes_are_proposals() {
        let f = setup();
        let moira = f.dir("/code/moira");
        f.hedy
            .add_note(Some(&moira), "Tests con pytest", Category::Convention)
            .unwrap();
        f.hedy
            .add_note(None, "Respuestas cortas y en español", Category::Preference)
            .unwrap();
        let proposal = f
            .hedy
            .propose_note(&moira, "Precios en USD", Category::Decision, "Codex")
            .unwrap();
        assert!(matches!(
            f.hedy.add_note(Some(&moira), "  ", Category::Rule),
            Err(AppError::EmptyNote)
        ));

        let view = f.hedy.memory(&moira).unwrap();
        assert_eq!(view.notes.len(), 2, "project + global");
        assert_eq!(view.proposed.len(), 1);
        // Agents never see proposals.
        assert_eq!(
            f.hedy
                .read_memory_for_agent(&moira, None, "")
                .unwrap()
                .len(),
            2
        );

        f.hedy.approve_note(proposal.id).unwrap();
        assert_eq!(
            f.hedy.read_memory_for_agent(&moira, None, "USD").unwrap()[0].author,
            "Codex"
        );
    }

    #[test]
    fn protected_projects_do_not_mix_with_global_notes() {
        let f = setup();
        f.hedy
            .set_setting(
                crate::application::Setting::DefaultAccount,
                Some("personal"),
            )
            .unwrap();
        let work = f.dir("/code/homming");
        f.hedy.use_account(&work, "trabajo", Some(true)).unwrap();
        f.hedy
            .add_note(None, "Preferencia personal", Category::Preference)
            .unwrap();
        f.hedy
            .add_note(Some(&work), "Regla del trabajo", Category::Rule)
            .unwrap();
        let view = f.hedy.memory(&work).unwrap();
        assert_eq!(
            view.notes
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Regla del trabajo"]
        );
        assert!(view.protected);
        let moira = f.hedy.memory(&f.dir("/code/moira")).unwrap();
        assert!(moira.notes.iter().all(|n| n.text != "Regla del trabajo"));
    }

    #[test]
    fn contradictions_are_resolved_or_dismissed() {
        let f = setup();
        let moira = f.dir("/code/moira");
        let a = f
            .hedy
            .add_note(
                Some(&moira),
                "Los recordatorios se envían 24 h antes",
                Category::Decision,
            )
            .unwrap();
        let b = f
            .hedy
            .add_note(
                Some(&moira),
                "Los recordatorios se envían 2 h antes",
                Category::Decision,
            )
            .unwrap();
        assert_eq!(f.hedy.memory(&moira).unwrap().contradictions.len(), 1);

        f.hedy.dismiss_contradiction(b.id, a.id).unwrap();
        assert!(f.hedy.memory(&moira).unwrap().contradictions.is_empty());

        f.hedy.keep_note(a.id, b.id).unwrap();
        assert_eq!(f.hedy.memory(&moira).unwrap().notes.len(), 1);
    }

    #[test]
    fn agents_reading_count_as_uses_of_the_task() {
        let f = setup();
        let moira = f.dir("/code/moira");
        f.hedy
            .add_note(Some(&moira), "Precios en USD", Category::Decision)
            .unwrap();
        let task = TaskId::new("t-1");
        let account = f.hedy.account("personal").unwrap();
        let project = f.hedy.project_at(&moira).unwrap();
        let link = f.hedy.memory_link(&project, &task, &account).unwrap();
        assert!(
            link.server.is_none(),
            "no server binary configured in tests"
        );
        assert!(link.tools.search("usd").contains("Precios en USD"));
        assert!(
            link.tools
                .propose("Zona horaria del salón", "decisión")
                .contains("Propuesta")
        );
        assert_eq!(f.hedy.memory(&moira).unwrap().proposed[0].author, "Claude");

        let note = &f.hedy.memory(&moira).unwrap().notes[0];
        assert_eq!(note.uses, 1);
    }

    #[test]
    fn memory_can_be_turned_off_per_account() {
        let f = setup();
        let moira = f.dir("/code/moira");
        let f2 = f.hedy.clone().with_memory_server(MemoryServerCommand {
            program: "/opt/hedy".into(),
            args: vec!["memory-mcp".into(), "--data-dir".into(), "/data".into()],
        });
        let account = f2.account("personal").unwrap();
        let link = f2
            .memory_link(
                &f2.project_at(&moira).unwrap(),
                &TaskId::new("t-9"),
                &account,
            )
            .unwrap();
        let server = link.server.unwrap();
        assert_eq!(server.command, "/opt/hedy");
        assert_eq!(
            server.args,
            [
                "memory-mcp",
                "--data-dir",
                "/data",
                "--project",
                "/code/moira",
                "--task",
                "t-9",
                "--agent",
                "Claude"
            ]
        );

        f2.set_memory_reader(&moira, "personal", false).unwrap();
        let project = f2.project_at(&moira).unwrap();
        assert!(
            f2.memory_link(&project, &TaskId::new("t"), &account)
                .is_none()
        );
        assert!(!f2.memory(&moira).unwrap().readers[0].1);
    }
}
