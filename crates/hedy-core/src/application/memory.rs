//! Project memory: notes every AI of a project reads, which the user curates.
//! Notes from AIs are proposals until the user approves them (spec rule 6).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::ports::{McpServer, MemoryLink, MemoryTools, NewNote, NoteRevision};
use super::{AppError, AppResult, Hedy};
use crate::domain::memory::{
    USER_AUTHOR, contradictions, is_stale, key_words, normalize_topic, preview, similar,
};
use crate::domain::recall::{NOTES_MAX, NOTES_PAGE, PageRequest, Paged, paginate, rank_notes};
use crate::domain::{Account, AccountId, Category, Note, NoteStatus, Project, TaskId};
use crate::experiments::{self, Experiment};
use crate::tr;

/// At most this many similar notes are shown next to a proposal.
const SIMILAR_MAX: usize = 3;
/// Notes a search returns without the `search-8` experiment (today's).
pub const LEGACY_SEARCH_MAX: usize = 30;
/// Characters of each note an agent sees in search results.
const PREVIEW_CHARS: usize = 140;

/// A note an AI proposed, with what the user needs to decide on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub note: Note,
    /// The approved note with the same topic: approving the proposal replaces
    /// its text (it keeps its id and uses); rejecting it changes nothing.
    pub replaces: Option<Note>,
    /// Approved notes that look like this one, best first (a local full-text
    /// search, no AI). Only a hint: they may say the same, or something else.
    pub similar: Vec<Note>,
}

/// Everything the memory screen shows for a project.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryView {
    /// Approved notes: the project's and, unless it is protected, global ones.
    pub notes: Vec<Note>,
    /// Proposed by AIs, waiting for the user.
    pub proposed: Vec<Proposal>,
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

/// The approved note a proposal would change: the one with its topic in its
/// own scope.
fn topic_target(approved: &[Note], proposal: &Note) -> Option<Note> {
    let topic = proposal.topic.as_ref()?;
    approved
        .iter()
        .find(|n| {
            n.id != proposal.id
                && n.status == NoteStatus::Approved
                && n.project == proposal.project
                && n.topic.as_ref() == Some(topic)
        })
        .cloned()
}

impl Hedy {
    /// Where the memory MCP server comes from (the app binary serves it).
    pub fn with_memory_server(mut self, command: MemoryServerCommand) -> Self {
        self.memory_server = Some(command);
        self
    }

    pub(super) fn with_global(project: &Project) -> bool {
        !project.protected
    }

    fn note_or_error(&self, id: i64) -> AppResult<Note> {
        self.store.note(id)?.ok_or(AppError::NoteNotFound(id))
    }

    fn proposal_or_error(&self, id: i64) -> AppResult<Note> {
        let note = self.note_or_error(id)?;
        if note.status != NoteStatus::Proposed {
            return Err(AppError::NotAProposal(id));
        }
        Ok(note)
    }

    /// Approved notes in the same scope as `note` (its project, or global).
    fn approved_beside(&self, note: &Note) -> AppResult<Vec<Note>> {
        Ok(self
            .store
            .notes(note.project.as_deref(), false)?
            .into_iter()
            .filter(|n| n.status == NoteStatus::Approved)
            .collect())
    }

    /// Accounts that work in a project: its own and the ones in its phases.
    pub(super) fn project_agents(&self, project: &Project) -> AppResult<Vec<Account>> {
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
        let proposed = proposed
            .into_iter()
            .map(|note| {
                let replaces = topic_target(&notes, &note);
                let skip = replaces.as_ref().map(|n| n.id);
                let similar = self.similar_notes(&project, &note, skip)?;
                Ok(Proposal {
                    note,
                    replaces,
                    similar,
                })
            })
            .collect::<AppResult<_>>()?;
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

    /// Approved notes that look like `note`: the full-text index finds
    /// candidates by its key words, and [`similar`] keeps the close ones.
    fn similar_notes(
        &self,
        project: &Project,
        note: &Note,
        skip: Option<i64>,
    ) -> AppResult<Vec<Note>> {
        let query = key_words(&note.text).join(" ");
        if query.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .store
            .search_notes(Some(&project.path), Self::with_global(project), &query)?
            .into_iter()
            .filter(|n| {
                n.status == NoteStatus::Approved
                    && n.id != note.id
                    && Some(n.id) != skip
                    && similar(&n.text, &note.text)
            })
            .take(SIMILAR_MAX)
            .collect())
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
        self.new_note(
            project,
            text,
            category,
            USER_AUTHOR,
            NoteStatus::Approved,
            None,
        )
    }

    /// A note suggested by an AI: it waits for the user's approval. Under the
    /// topic of an approved note, it is a proposed change to that note.
    pub fn propose_note(
        &self,
        project: &Path,
        text: &str,
        category: Category,
        author: &str,
        topic: Option<&str>,
    ) -> AppResult<Note> {
        let project = self.project_at(project)?;
        let topic = match topic {
            Some(raw) => normalize_topic(raw)?,
            None => None,
        };
        self.new_note(
            Some(project.path),
            text,
            category,
            author,
            NoteStatus::Proposed,
            topic,
        )
    }

    fn new_note(
        &self,
        project: Option<PathBuf>,
        text: &str,
        category: Category,
        author: &str,
        status: NoteStatus,
        topic: Option<String>,
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
            topic,
        })?)
    }

    /// Approves a proposal. Under the topic of an approved note, the proposal
    /// replaces that note's text (the note keeps its id and history) and the
    /// returned note is the updated one.
    pub fn approve_note(&self, id: i64) -> AppResult<Note> {
        let note = self.note_or_error(id)?;
        if note.status == NoteStatus::Approved {
            return Ok(note);
        }
        if let Some(target) = topic_target(&self.approved_beside(&note)?, &note) {
            let topic = note.topic.clone();
            return self.revise_with(&note, &target, topic);
        }
        self.store.set_note_status(id, NoteStatus::Approved)?;
        self.note_or_error(id)
    }

    /// "Reemplazar esa": the proposal becomes the new version of an approved
    /// note (usually one shown as similar), which keeps its id and uses.
    pub fn replace_note(&self, proposal: i64, target: i64) -> AppResult<Note> {
        let proposal = self.proposal_or_error(proposal)?;
        let target_note = self.note_or_error(target)?;
        let protected = proposal
            .project
            .as_deref()
            .and_then(|p| self.project_at(p).ok())
            .is_some_and(|p| p.protected);
        let in_scope = target_note.project == proposal.project
            || (target_note.project.is_none() && !protected);
        if target_note.status != NoteStatus::Approved || !in_scope {
            return Err(AppError::NoteNotFound(target));
        }
        // The target keeps its topic; it takes the proposal's only when no
        // other approved note has it.
        let topic = match (&target_note.topic, &proposal.topic) {
            (Some(own), _) => Some(own.clone()),
            (None, Some(topic)) => {
                let taken = topic_target(&self.approved_beside(&proposal)?, &proposal)
                    .is_some_and(|n| n.id != target);
                (!taken).then(|| topic.clone())
            }
            (None, None) => None,
        };
        self.revise_with(&proposal, &target_note, topic)
    }

    fn revise_with(
        &self,
        proposal: &Note,
        target: &Note,
        topic: Option<String>,
    ) -> AppResult<Note> {
        self.store.revise_note(
            target.id,
            &NoteRevision {
                text: proposal.text.clone(),
                category: proposal.category,
                author: proposal.author.clone(),
                topic,
                updated_at: self.clock.now_ms() / 1000,
            },
        )?;
        self.store.delete_note(proposal.id)?;
        self.note_or_error(target.id)
    }

    /// Rejects a proposal: it is deleted and nothing else changes.
    pub fn reject_note(&self, id: i64) -> AppResult<()> {
        self.proposal_or_error(id)?;
        Ok(self.store.delete_note(id)?)
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

    /// What an agent reads: the first page of approved notes matching
    /// `query` (all when empty), best first. See [`Self::read_memory_page`].
    pub fn read_memory_for_agent(
        &self,
        path: &Path,
        task: Option<&TaskId>,
        query: &str,
    ) -> AppResult<Vec<Note>> {
        Ok(self
            .read_memory_page(path, task, query, PageRequest::default())?
            .items)
    }

    /// One page of the approved notes matching `query` (all when empty), in
    /// full-text order. Only the notes returned count as used, by this task
    /// when known. By default it is the first [`LEGACY_SEARCH_MAX`] and `page`
    /// is ignored; two experiments change that (docs/decisions.md §33):
    /// - `search-8`: [`NOTES_PAGE`] by default, `page` honored up to
    ///   [`NOTES_MAX`];
    /// - `rank-usage`: ranked by [`rank_notes`] (a small lift for notes this
    ///   project's tasks read, notes "para revisar" last).
    pub fn read_memory_page(
        &self,
        path: &Path,
        task: Option<&TaskId>,
        query: &str,
        page: PageRequest,
    ) -> AppResult<Paged<Note>> {
        let project = self.project_at(path)?;
        let candidates: Vec<Note> = self
            .store
            .search_notes(Some(&project.path), Self::with_global(&project), query)?
            .into_iter()
            .filter(|n| n.status == NoteStatus::Approved)
            .collect();
        let uses: HashMap<_, _> = if experiments::on(Experiment::RankUsage) {
            self.store
                .note_use_stats(&project.path)?
                .into_iter()
                .collect()
        } else {
            HashMap::new()
        };
        let now = self.now_secs();
        let ranked = if experiments::on(Experiment::RankUsage) {
            rank_notes(candidates, &uses, now)
        } else {
            candidates
        };
        let page = if experiments::on(Experiment::Search8) {
            paginate(ranked, page, NOTES_PAGE, NOTES_MAX)
        } else {
            paginate(
                ranked,
                PageRequest::default(),
                LEGACY_SEARCH_MAX,
                LEGACY_SEARCH_MAX,
            )
        };
        let ids: Vec<i64> = page.items.iter().map(|n| n.id).collect();
        self.store.record_uses(&ids, task, now)?;
        Ok(page)
    }

    /// Approved notes an agent may read in the project containing `path`.
    fn agent_notes(&self, path: &Path) -> AppResult<Vec<Note>> {
        let project = self.project_at(path)?;
        Ok(self
            .store
            .notes(Some(&project.path), Self::with_global(&project))?
            .into_iter()
            .filter(|n| n.status == NoteStatus::Approved)
            .collect())
    }

    /// One approved note for an agent, by id or by topic (the id wins). It
    /// counts as used. `None` when there is no such note in scope.
    pub fn read_note_for_agent(
        &self,
        path: &Path,
        task: Option<&TaskId>,
        id: Option<i64>,
        topic: Option<&str>,
    ) -> AppResult<Option<Note>> {
        let notes = self.agent_notes(path)?;
        let found = match (id, topic) {
            (Some(id), _) => notes.into_iter().find(|n| n.id == id),
            (None, Some(raw)) => match normalize_topic(raw)? {
                // The project's own note first, then a global one.
                Some(topic) => notes
                    .into_iter()
                    .filter(|n| n.topic.as_ref() == Some(&topic))
                    .min_by_key(|n| n.project.is_none()),
                None => None,
            },
            (None, None) => None,
        };
        if let Some(note) = &found {
            self.store
                .record_uses(&[note.id], task, self.clock.now_ms() / 1000)?;
        }
        Ok(found)
    }

    /// Approved notes with a topic, sorted by topic: what agents propose
    /// changes under.
    pub fn memory_topics(&self, path: &Path) -> AppResult<Vec<Note>> {
        let mut notes: Vec<Note> = self
            .agent_notes(path)?
            .into_iter()
            .filter(|n| n.topic.is_some())
            .collect();
        notes.sort_by(|a, b| a.topic.cmp(&b.topic));
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
            // The server runs with the same experiments (docs/decisions.md §33).
            env: vec![experiments::child_env()],
        });
        Some(MemoryLink {
            server,
            skills: Some(self.task_skills(project, task, &author)),
            team: self.team_context(&project.path),
            tools: Arc::new(TaskMemory {
                hedy: self.clone(),
                project: project.path.clone(),
                task: Some(task.clone()),
                author,
            }),
        })
    }

    /// The memory tools an agent uses in the project containing `path`: what
    /// the memory MCP server serves. Reads count as uses of `task` when known;
    /// proposals are signed by `author`.
    pub fn memory_tools(
        &self,
        path: &Path,
        task: Option<&TaskId>,
        author: &str,
    ) -> AppResult<Arc<dyn MemoryTools>> {
        Ok(Arc::new(TaskMemory {
            hedy: self.clone(),
            project: self.project_at(path)?.path,
            task: task.cloned(),
            author: author.to_owned(),
        }))
    }
}

/// "decision · proyecto", the label of a note for agents.
fn note_label(note: &Note) -> String {
    let scope = if note.project.is_some() {
        tr!("proyecto", "project")
    } else {
        "global".to_owned()
    };
    format!("{} · {scope}", note.category.as_str())
}

/// What an agent reads for a search: one compact line per note (id, label,
/// topic and the start of its text), then how to ask for more when the page
/// is not the end. [`format_note`] has the whole note.
pub fn format_note_page(page: &Paged<Note>) -> String {
    let mut text = format_notes(&page.items);
    if page.remaining > 0 && !page.items.is_empty() {
        let (n, offset) = (page.remaining, page.next_offset());
        text.push('\n');
        text.push_str(&tr!(
            "… {n} más: llama a memory_search con offset {offset}",
            "… {n} more: call memory_search with offset {offset}"
        ));
    }
    text
}

/// One compact line per note; see [`format_note_page`].
pub fn format_notes(notes: &[Note]) -> String {
    if notes.is_empty() {
        return tr!(
            "No hay notas sobre eso en la memoria del proyecto.",
            "The project's memory has no notes about that."
        );
    }
    let mut cut = false;
    let mut lines: Vec<String> = notes
        .iter()
        .map(|n| {
            let (text, more) = preview(&n.text, PREVIEW_CHARS);
            cut |= more;
            let topic = n
                .topic
                .as_ref()
                .map(|t| format!(" {t}:"))
                .unwrap_or_default();
            format!("- #{} [{}]{topic} {text}", n.id, note_label(n))
        })
        .collect();
    if cut {
        lines.push(tr!(
            "(… = nota recortada: memory_get con su id la devuelve completa.)",
            "(… = note cut short: memory_get with its id returns all of it.)"
        ));
    }
    lines.join("\n")
}

/// One whole note, for `memory_get`.
pub fn format_note(note: &Note) -> String {
    let topic = match &note.topic {
        Some(topic) => tr!(" · tema: {topic}", " · topic: {topic}"),
        None => String::new(),
    };
    format!("#{} [{}]{topic}\n{}", note.id, note_label(note), note.text)
}

/// The topics in the memory, for `memory_topics`.
pub fn format_topics(notes: &[Note]) -> String {
    if notes.is_empty() {
        return tr!(
            "Todavía no hay temas en la memoria del proyecto.",
            "The project's memory has no topics yet."
        );
    }
    notes
        .iter()
        .filter_map(|n| {
            let topic = n.topic.as_ref()?;
            let (text, _) = preview(&n.text, 80);
            Some(format!("- {topic} → #{}: {text}", n.id))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Memory tools for an agent: the API agent calls them through Hedy, CLI
/// agents through the memory MCP server.
struct TaskMemory {
    hedy: Hedy,
    project: PathBuf,
    task: Option<TaskId>,
    author: String,
}

impl MemoryTools for TaskMemory {
    fn search(&self, query: &str) -> String {
        self.search_page(query, PageRequest::default())
    }

    fn search_page(&self, query: &str, page: PageRequest) -> String {
        let first = page.offset.unwrap_or(0) == 0;
        let page = match self
            .hedy
            .read_memory_page(&self.project, self.task.as_ref(), query, page)
        {
            Ok(page) => page,
            Err(e) => {
                return tr!(
                    "No se pudo leer la memoria: {e}",
                    "Could not read the memory: {e}"
                );
            }
        };
        let text = match experiments::on(Experiment::Search8) {
            true => format_note_page(&page),
            false => format_notes(&page.items),
        };
        // The team's notes (a project of a team) open the first page, so its
        // paging footer stays last; without notes of its own, they are the
        // answer.
        match first
            .then(|| self.hedy.team_search(&self.project, query))
            .flatten()
        {
            Some(team) if page.items.is_empty() => team,
            Some(team) => format!("{team}\n\n{text}"),
            None => text,
        }
    }

    fn get(&self, id: Option<i64>, topic: Option<&str>) -> String {
        let topic = topic.filter(|t| !t.trim().is_empty());
        if id.is_none() && topic.is_none() {
            return tr!(
                "Indica el id de la nota (de memory_search) o su tema.",
                "Give the note's id (from memory_search) or its topic."
            );
        }
        match self
            .hedy
            .read_note_for_agent(&self.project, self.task.as_ref(), id, topic)
        {
            Ok(Some(note)) => format_note(&note),
            Ok(None) => tr!(
                "No hay una nota aprobada con ese id o tema en este proyecto.",
                "There is no approved note with that id or topic in this project."
            ),
            Err(e) => tr!(
                "No se pudo leer la memoria: {e}",
                "Could not read the memory: {e}"
            ),
        }
    }

    fn topics(&self) -> String {
        match self.hedy.memory_topics(&self.project) {
            Ok(notes) => format_topics(&notes),
            Err(e) => tr!(
                "No se pudo leer la memoria: {e}",
                "Could not read the memory: {e}"
            ),
        }
    }

    fn propose(&self, text: &str, category: &str, topic: Option<&str>) -> String {
        let Ok(category) = category.parse::<Category>() else {
            let valid: Vec<&str> = Category::ALL.iter().map(|c| c.as_str()).collect();
            let (category, valid) = (category.trim(), valid.join(", "));
            return tr!(
                "Categoría desconocida: «{category}». No se guardó la nota; usa una de estas: {valid}.",
                "Unknown category: «{category}». The note was not saved; use one of: {valid}."
            );
        };
        let proposed = self
            .hedy
            .propose_note(&self.project, text, category, &self.author, topic);
        let target = proposed.as_ref().ok().and_then(|note| {
            let approved = self.hedy.approved_beside(note).ok()?;
            topic_target(&approved, note)
        });
        match (proposed, target) {
            (Ok(_), Some(target)) => {
                let id = target.id;
                tr!(
                    "Propuesta guardada como cambio de la nota #{id} (mismo tema); el usuario la revisará.",
                    "Proposal saved as a change to note #{id} (same topic); the user will review it."
                )
            }
            (Ok(_), None) => tr!(
                "Propuesta guardada; el usuario la revisará.",
                "Proposal saved; the user will review it."
            ),
            (Err(e), _) => tr!(
                "No se pudo proponer la nota: {e}",
                "Could not propose the note: {e}"
            ),
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
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f
    }

    #[test]
    fn user_notes_are_approved_and_ai_notes_are_proposals() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        f.hedy
            .add_note(Some(&agenda), "Tests con pytest", Category::Convention)
            .unwrap();
        f.hedy
            .add_note(None, "Respuestas cortas y en español", Category::Preference)
            .unwrap();
        let proposal = f
            .hedy
            .propose_note(&agenda, "Precios en USD", Category::Decision, "Codex", None)
            .unwrap();
        assert!(matches!(
            f.hedy.add_note(Some(&agenda), "  ", Category::Rule),
            Err(AppError::EmptyNote)
        ));

        let view = f.hedy.memory(&agenda).unwrap();
        assert_eq!(view.notes.len(), 2, "project + global");
        assert_eq!(view.proposed.len(), 1);
        // Agents never see proposals.
        assert_eq!(
            f.hedy
                .read_memory_for_agent(&agenda, None, "")
                .unwrap()
                .len(),
            2
        );

        f.hedy.approve_note(proposal.id).unwrap();
        assert_eq!(
            f.hedy.read_memory_for_agent(&agenda, None, "USD").unwrap()[0].author,
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
        let work = f.dir("/code/tienda");
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
        let agenda = f.hedy.memory(&f.dir("/code/agenda")).unwrap();
        assert!(agenda.notes.iter().all(|n| n.text != "Regla del trabajo"));
    }

    #[test]
    fn contradictions_are_resolved_or_dismissed() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let a = f
            .hedy
            .add_note(
                Some(&agenda),
                "Los recordatorios se envían 24 h antes",
                Category::Decision,
            )
            .unwrap();
        let b = f
            .hedy
            .add_note(
                Some(&agenda),
                "Los recordatorios se envían 2 h antes",
                Category::Decision,
            )
            .unwrap();
        assert_eq!(f.hedy.memory(&agenda).unwrap().contradictions.len(), 1);

        f.hedy.dismiss_contradiction(b.id, a.id).unwrap();
        assert!(f.hedy.memory(&agenda).unwrap().contradictions.is_empty());

        f.hedy.keep_note(a.id, b.id).unwrap();
        assert_eq!(f.hedy.memory(&agenda).unwrap().notes.len(), 1);
    }

    #[test]
    fn agents_reading_count_as_uses_of_the_task() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        f.hedy
            .add_note(Some(&agenda), "Precios en USD", Category::Decision)
            .unwrap();
        let task = TaskId::new("t-1");
        let account = f.hedy.account("personal").unwrap();
        let project = f.hedy.project_at(&agenda).unwrap();
        let link = f.hedy.memory_link(&project, &task, &account).unwrap();
        assert!(
            link.server.is_none(),
            "no server binary configured in tests"
        );
        assert!(link.tools.search("usd").contains("Precios en USD"));
        assert!(
            link.tools
                .propose("Zona horaria del salón", "decisión", None)
                .contains("Propuesta")
        );
        assert_eq!(
            f.hedy.memory(&agenda).unwrap().proposed[0].note.author,
            "Claude"
        );

        let note = &f.hedy.memory(&agenda).unwrap().notes[0];
        assert_eq!(note.uses, 1);
    }

    #[test]
    fn unknown_categories_are_not_saved() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let tools = f.hedy.memory_tools(&agenda, None, "Codex").unwrap();
        let answer = tools.propose("Precios en USD", "idea", None);
        assert!(answer.contains("decision, convention, rule, preference"));
        assert!(f.hedy.memory(&agenda).unwrap().proposed.is_empty());

        assert!(
            tools
                .propose("Precios en USD", "Regla", None)
                .contains("Propuesta")
        );
        let proposed = f.hedy.memory(&agenda).unwrap().proposed;
        assert_eq!(proposed[0].note.category, Category::Rule);
        assert_eq!(proposed[0].note.author, "Codex");
        assert!(
            tools
                .propose("Precios en USD", "rule", Some("¿?"))
                .contains("tema no válido")
        );
        assert!(
            f.hedy
                .memory_tools(&f.dir("/elsewhere"), None, "Codex")
                .is_err()
        );
    }

    /// An approved note under `topic`, as agents leave them: proposed, then approved.
    fn approved_with_topic(f: &Fixture, project: &Path, text: &str, topic: &str) -> Note {
        let note = f
            .hedy
            .propose_note(project, text, Category::Decision, "Claude", Some(topic))
            .unwrap();
        f.hedy.approve_note(note.id).unwrap()
    }

    #[test]
    fn a_proposal_under_a_topic_changes_the_approved_note() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let old = approved_with_topic(
            &f,
            &agenda,
            "Los recordatorios se envían por WhatsApp",
            "Recordatorios/Canal",
        );
        assert_eq!(old.topic.as_deref(), Some("recordatorios/canal"));
        f.hedy.read_memory_for_agent(&agenda, None, "").unwrap();

        let change = f
            .hedy
            .propose_note(
                &agenda,
                "Los recordatorios se envían por WhatsApp y, si falla, por SMS",
                Category::Decision,
                "Codex",
                Some("recordatorios/canal"),
            )
            .unwrap();
        let view = f.hedy.memory(&agenda).unwrap();
        assert_eq!(
            view.proposed[0].replaces.as_ref().map(|n| n.id),
            Some(old.id)
        );
        assert!(
            view.proposed[0].similar.iter().all(|n| n.id != old.id),
            "the note it changes is not listed again as similar"
        );
        // Agents keep reading the approved version until the user decides.
        assert!(
            f.hedy
                .read_memory_for_agent(&agenda, None, "SMS")
                .unwrap()
                .is_empty()
        );

        // Rejecting changes nothing.
        f.hedy.reject_note(change.id).unwrap();
        let view = f.hedy.memory(&agenda).unwrap();
        assert!(view.proposed.is_empty());
        assert_eq!(view.notes, vec![f.hedy.note_or_error(old.id).unwrap()]);
        assert_eq!(
            view.notes[0].text,
            "Los recordatorios se envían por WhatsApp"
        );

        // Approving replaces the text and keeps the note's id and history.
        let change = f
            .hedy
            .propose_note(
                &agenda,
                "Los recordatorios se envían por SMS",
                Category::Rule,
                "Codex",
                Some("recordatorios/canal"),
            )
            .unwrap();
        let updated = f.hedy.approve_note(change.id).unwrap();
        assert_eq!(updated.id, old.id);
        assert_eq!(updated.text, "Los recordatorios se envían por SMS");
        assert_eq!(
            (updated.category, updated.author.as_str()),
            (Category::Rule, "Codex")
        );
        assert_eq!(updated.uses, 1);
        assert!(updated.updated_at.is_some_and(|at| at > old.created_at));
        let view = f.hedy.memory(&agenda).unwrap();
        assert_eq!(view.notes.len(), 1);
        assert!(view.proposed.is_empty());
        assert!(matches!(
            f.hedy.note_or_error(change.id),
            Err(AppError::NoteNotFound(_))
        ));
        assert!(matches!(
            f.hedy.reject_note(old.id),
            Err(AppError::NotAProposal(_))
        ));
    }

    #[test]
    fn topics_belong_to_their_project() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let other = f.dir("/code/otro");
        f.hedy.use_account(&other, "personal", None).unwrap();
        approved_with_topic(&f, &agenda, "Pagos con Stripe", "pagos");
        let proposal = f
            .hedy
            .propose_note(
                &other,
                "Pagos con PayPal",
                Category::Decision,
                "Codex",
                Some("pagos"),
            )
            .unwrap();
        assert!(
            f.hedy.memory(&other).unwrap().proposed[0]
                .replaces
                .is_none()
        );
        let approved = f.hedy.approve_note(proposal.id).unwrap();
        assert_eq!(approved.id, proposal.id, "a new note in the other project");
        assert_eq!(
            f.hedy.memory(&agenda).unwrap().notes[0].text,
            "Pagos con Stripe"
        );
    }

    #[test]
    fn similar_notes_are_shown_next_to_a_proposal() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let prices = f
            .hedy
            .add_note(
                Some(&agenda),
                "Los precios se guardan en USD y se muestran también en Bs",
                Category::Decision,
            )
            .unwrap();
        f.hedy
            .add_note(Some(&agenda), "Tests con pytest", Category::Convention)
            .unwrap();
        let global = f
            .hedy
            .add_note(None, "Los precios siempre en USD", Category::Preference)
            .unwrap();
        let usd = f
            .hedy
            .propose_note(
                &agenda,
                "Los precios se guardan en USD",
                Category::Decision,
                "Codex",
                None,
            )
            .unwrap();
        let zod = f
            .hedy
            .propose_note(
                &agenda,
                "Los formularios validan con zod",
                Category::Convention,
                "Codex",
                None,
            )
            .unwrap();
        let view = f.hedy.memory(&agenda).unwrap();
        let proposal = |id: i64| view.proposed.iter().find(|p| p.note.id == id).unwrap();
        let similar: Vec<i64> = proposal(usd.id).similar.iter().map(|n| n.id).collect();
        assert!(similar.contains(&prices.id) && similar.contains(&global.id));
        assert!(proposal(usd.id).replaces.is_none());
        assert!(proposal(zod.id).similar.is_empty());

        // "Reemplazar esa": the similar note takes the proposal's text.
        let replaced = f.hedy.replace_note(usd.id, prices.id).unwrap();
        assert_eq!(replaced.id, prices.id);
        assert_eq!(replaced.text, "Los precios se guardan en USD");
        assert!(replaced.updated_at.is_some());
        let view = f.hedy.memory(&agenda).unwrap();
        assert_eq!(view.proposed.len(), 1);
        assert_eq!(view.notes.len(), 3);

        // Only proposals replace, and only approved notes in scope.
        assert!(matches!(
            f.hedy.replace_note(prices.id, global.id),
            Err(AppError::NotAProposal(_))
        ));
        assert!(matches!(
            f.hedy.replace_note(zod.id, zod.id),
            Err(AppError::NoteNotFound(_))
        ));
    }

    #[test]
    fn agents_search_compactly_and_read_whole_notes() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let long = "Los pagos se procesan con Stripe. ".repeat(8);
        let pay = approved_with_topic(&f, &agenda, &long, "arquitectura/pagos");
        let short = f
            .hedy
            .add_note(Some(&agenda), "Tests con pytest", Category::Convention)
            .unwrap();
        f.hedy
            .propose_note(
                &agenda,
                "Pagos secretos",
                Category::Rule,
                "Codex",
                Some("x"),
            )
            .unwrap();
        let tools = f
            .hedy
            .memory_tools(&agenda, Some(&TaskId::new("t-1")), "Codex")
            .unwrap();

        let found = tools.search("");
        assert!(found.contains(&format!(
            "- #{} [convention · proyecto] Tests con pytest",
            short.id
        )));
        assert!(found.contains(&format!(
            "- #{} [decision · proyecto] arquitectura/pagos:",
            pay.id
        )));
        assert!(!found.contains(long.trim()), "long notes are cut short");
        assert!(found.contains("memory_get"));
        assert!(!found.contains("secretos"), "never proposals");

        let whole = tools.get(Some(pay.id), None);
        assert!(whole.contains(long.trim()) && whole.contains("tema: arquitectura/pagos"));
        assert_eq!(tools.get(None, Some("Arquitectura/Pagos")), whole);
        assert!(tools.get(Some(9_999), None).contains("No hay una nota"));
        assert!(tools.get(None, None).contains("Indica el id"));
        assert!(
            tools.get(None, Some("x")).contains("No hay una nota"),
            "proposals stay hidden"
        );

        let topics = tools.topics();
        assert!(topics.starts_with(&format!(
            "- arquitectura/pagos → #{}: Los pagos se procesan",
            pay.id
        )));
        assert!(topics.ends_with('…') && !topics.contains('\n'));
        let answer = tools.propose(
            "Pagos con Mercado Pago",
            "decision",
            Some("arquitectura/pagos"),
        );
        assert!(
            answer.contains(&format!("cambio de la nota #{}", pay.id)),
            "{answer}"
        );

        let note = f.hedy.note_or_error(pay.id).unwrap();
        assert_eq!(note.uses, 3, "search, get by id and get by topic");
    }

    #[test]
    fn memory_can_be_turned_off_per_account() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let f2 = f.hedy.clone().with_memory_server(MemoryServerCommand {
            program: "/opt/hedy".into(),
            args: vec!["memory-mcp".into(), "--data-dir".into(), "/data".into()],
        });
        let account = f2.account("personal").unwrap();
        let link = f2
            .memory_link(
                &f2.project_at(&agenda).unwrap(),
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
                "/code/agenda",
                "--task",
                "t-9",
                "--agent",
                "Claude"
            ]
        );

        f2.set_memory_reader(&agenda, "personal", false).unwrap();
        let project = f2.project_at(&agenda).unwrap();
        assert!(
            f2.memory_link(&project, &TaskId::new("t"), &account)
                .is_none()
        );
        assert!(!f2.memory(&agenda).unwrap().readers[0].1);
    }
}
