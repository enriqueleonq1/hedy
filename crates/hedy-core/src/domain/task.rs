//! A task: one request worked on in its own git worktree and branch.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{AccountId, Stage};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The top-bar modes. Only `Project` changes files (in a worktree).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskMode {
    Project,
    Ask,
    Research,
}

impl TaskMode {
    pub fn changes_files(self) -> bool {
        self == Self::Project
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    /// The agent is working.
    Running,
    /// A plan or a permission card waits for the user.
    WaitingApproval,
    /// The agent finished; changes wait for review (or a follow-up).
    Review,
    Accepted,
    Discarded,
    Failed,
}

impl TaskStatus {
    /// Accepted and discarded tasks are closed: no more messages, no worktree.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Accepted | Self::Discarded)
    }
}

/// Dependency folders shared with the worktree through a symlink when git
/// ignores them, so builds and tests work without reinstalling. Secrets such
/// as `.env` are deliberately not shared: the task never sees them.
pub const SHARED_DEPENDENCY_DIRS: &[&str] = &["node_modules", ".venv", "venv", "vendor"];

/// Where a `Project` task works: its branch, forked from `base_branch` at `base_commit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub worktree: PathBuf,
    pub branch: String,
    pub base_branch: String,
    pub base_commit: String,
}

/// How much an agent does without asking in a task that changes files, as
/// Claude Code's modes. Whatever it is, Hedy keeps its floor: nothing outside
/// the task's copy, nothing in `.git`, the agents' own settings always ask, and
/// the project's blocked files stay blocked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Every edit and command asks (as before).
    #[default]
    Manual,
    /// Edits inside the copy go through; commands ask.
    AcceptEdits,
    /// Edits and commands go through; a command that names a path outside
    /// the copy or does something risky (`sudo`, `git push`) still asks.
    Auto,
}

impl PermissionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AcceptEdits => "acceptEdits",
            Self::Auto => "auto",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "manual" => Some(Self::Manual),
            "acceptEdits" => Some(Self::AcceptEdits),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

/// A file a read-only task wrote in its own folder, outside the project (a
/// document, a page): `path` is relative to that folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputFile {
    pub path: String,
    pub bytes: u64,
    /// Whether Hedy opens it with the system's app: only documents, pages,
    /// images and data, never something that could run (`.desktop`, `.jar`).
    pub opens: bool,
}

/// Types a question's file may be opened as, from its name.
const OPENABLE: &[&str] = &[
    "md", "markdown", "txt", "html", "htm", "pdf", "png", "jpg", "jpeg", "gif", "webp", "svg",
    "csv", "tsv", "json",
];

impl OutputFile {
    pub fn new(path: String, bytes: u64) -> Self {
        let opens = path
            .rsplit_once('.')
            .filter(|(stem, _)| !stem.ends_with('/') && !stem.is_empty())
            .is_some_and(|(_, ext)| OPENABLE.contains(&ext.to_lowercase().as_str()));
        Self { path, bytes, opens }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: TaskId,
    /// Project root (the main repository).
    pub project: PathBuf,
    pub mode: TaskMode,
    pub title: String,
    pub prompt: String,
    pub account: AccountId,
    pub model: Option<String>,
    pub workspace: Option<Workspace>,
    /// A read-only task's throwaway copy, without a branch: there while an
    /// agent that reads without asking (Codex, ACP) works on a turn, deleted
    /// when the turn ends. Never set with `workspace`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch: Option<PathBuf>,
    /// What the agent does without asking (tasks that change files).
    #[serde(default)]
    pub permissions: PermissionMode,
    /// The orchestrator's flow this task runs, and where it is
    /// (docs/decisions.md §42). `None`: the project's phases, as always.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<super::flow::FlowRun>,
    /// The CLI's session, to resume it after a restart.
    pub session_id: Option<String>,
    pub status: TaskStatus,
    pub created_at: i64,
    /// The phases this task runs through, and the current one.
    #[serde(default)]
    pub stages: Vec<Stage>,
    #[serde(default)]
    pub stage: usize,
    /// The latest proposed (and then approved) plan.
    #[serde(default)]
    pub plan: Option<String>,
    /// What accepting did to the base branch, to undo it ("Deshacer").
    #[serde(default)]
    pub merge: Option<MergeRecord>,
    /// The user chose to wait for the account's limit to renew.
    #[serde(default)]
    pub wait: Option<LimitWait>,
    /// A loose question or research outside any project ("Pregunta rápida"):
    /// `project` is then its own private, empty scratch folder in Hedy's data
    /// folder, and it never has a workspace, memory or code map.
    #[serde(default)]
    pub conversation: bool,
    /// Images attached to the request itself, so every phase (and a
    /// handoff) gets them too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<super::Attachment>,
    /// Where accepting put the work (merged, or kept on its branch) and what
    /// Hedy published of it (push, pull request).
    #[serde(default)]
    pub ship: Option<super::Shipment>,
    /// The conversation this task continues ("Llevar a un proyecto"): its
    /// prompt opens with that conversation's context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<super::TaskOrigin>,
    /// Whether the code map was on when the task started, to compare tokens
    /// with and without it (docs/decisions.md §33). `None`: an older task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_map: Option<bool>,
    /// How this task works with one AI, taken from its project when it
    /// started (docs/decisions.md §60). `None`: a flow, a question or an
    /// older task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working: Option<super::WorkingStyle>,
}

/// What "Aceptar" changed in the main checkout, kept so it can be undone
/// while nothing else happened on the base branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRecord {
    /// The base branch's commit before the merge.
    pub before: String,
    /// The base branch's commit right after it (a merge commit, or the
    /// task's own commit after a fast-forward).
    pub after: String,
    /// The task branch's last commit, to bring the branch back on undo.
    pub tip: String,
    /// Unix seconds.
    pub at: i64,
}

/// "Esperar": the task waits for its account's limit to renew, then Hedy
/// resumes it on the same account (never another one: rule 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWait {
    /// The account whose limit was reached; the task resumes only on it.
    pub account: AccountId,
    /// When the limit renews (Unix seconds), when known.
    pub until: Option<i64>,
    /// When the user chose to wait (Unix seconds).
    pub since: i64,
    /// The reset passed while Hedy was closed (or resuming failed): the user
    /// resumes with "Continuar"; nothing is sent by itself.
    #[serde(default)]
    pub ready: bool,
}

impl LimitWait {
    /// Hedy may resume the task by itself now.
    pub fn due(&self, now: i64) -> bool {
        !self.ready && self.until.is_some_and(|until| until <= now)
    }
}

/// The commit message "Aceptar" proposes: the task's title and, when the agent
/// left a summary, its first paragraph (shortened).
pub fn default_commit_message(title: &str, summary: Option<&str>) -> String {
    const MAX: usize = 300;
    let paragraph = summary
        .unwrap_or("")
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty())
        .unwrap_or("");
    if paragraph.is_empty() || paragraph == title.trim() {
        return title.to_owned();
    }
    let body = if paragraph.chars().count() <= MAX {
        paragraph.to_owned()
    } else {
        let cut: String = paragraph.chars().take(MAX).collect();
        let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
        format!("{cut}…")
    };
    format!("{title}\n\n{body}")
}

impl Task {
    /// Folder the agent works in: the worktree, or the project for read-only modes.
    pub fn work_dir(&self) -> PathBuf {
        self.workspace
            .as_ref()
            .map(|w| w.worktree.clone())
            .or_else(|| self.scratch.clone())
            .unwrap_or_else(|| self.project.clone())
    }

    /// What the user asked, without the context a carried conversation put
    /// before it.
    pub fn instruction(&self) -> &str {
        self.origin
            .as_ref()
            .and_then(|o| self.prompt.strip_prefix(o.context.as_str()))
            .map_or(self.prompt.as_str(), str::trim_start)
    }
}

/// First line of the prompt, shortened: "Agrega un recordatorio por WhatsApp…".
pub fn title_from_prompt(prompt: &str) -> String {
    const MAX: usize = 60;
    let line = prompt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("Tarea");
    let line = line.trim_end_matches(['.', ':']);
    if line.chars().count() <= MAX {
        return line.to_owned();
    }
    let cut: String = line.chars().take(MAX).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}…")
}

/// `hedy/recordatorio-por-whatsapp-3f9a1c`.
pub fn branch_name(title: &str, id: &TaskId) -> String {
    let mut slug = String::new();
    for c in title.to_lowercase().chars() {
        let c = super::account::fold_accent(c);
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    let suffix: String = id
        .as_str()
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if slug.is_empty() {
        format!("hedy/tarea-{suffix}")
    } else {
        format!("hedy/{slug}-{suffix}")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_documents_pages_images_and_data_open() {
        for path in [
            "informe.md",
            "sitio/index.HTML",
            "a.pdf",
            "datos.csv",
            "logo.svg",
        ] {
            assert!(OutputFile::new(path.into(), 1).opens, "{path}");
        }
        for path in [
            "x.desktop",
            "a.jar",
            "run.command",
            "sin-tipo",
            ".md",
            "dir/.html",
            "a.sh",
        ] {
            assert!(!OutputFile::new(path.into(), 1).opens, "{path}");
        }
    }

    use super::*;

    #[test]
    fn titles_are_short_single_lines() {
        assert_eq!(
            title_from_prompt("\n  Agrega un recordatorio.\nDetalles…"),
            "Agrega un recordatorio"
        );
        let long = "Agrega un recordatorio por WhatsApp veinticuatro horas antes de cada cita del calendario";
        let title = title_from_prompt(long);
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 61);
        assert_eq!(title_from_prompt("   "), "Tarea");
    }

    #[test]
    fn branch_names_are_safe() {
        let id = TaskId::new("t-0193abcdef");
        assert_eq!(
            branch_name("Añade recordatorios: ¡24 h antes!", &id),
            "hedy/anade-recordatorios-24-h-antes-abcdef"
        );
        assert_eq!(branch_name("¿¿??", &id), "hedy/tarea-abcdef");
    }

    #[test]
    fn commit_messages_take_the_summary_s_first_paragraph() {
        assert_eq!(
            default_commit_message("Recordatorios", None),
            "Recordatorios"
        );
        assert_eq!(
            default_commit_message(
                "Recordatorios",
                Some("  \n\nListo: se programan 24 h antes.\n\nDetalles…")
            ),
            "Recordatorios\n\nListo: se programan 24 h antes."
        );
        assert_eq!(
            default_commit_message("Recordatorios", Some("Recordatorios")),
            "Recordatorios"
        );
        let long = "palabra ".repeat(80);
        let message = default_commit_message("T", Some(&long));
        assert!(message.ends_with('…'));
        assert!(message.chars().count() < 310);
    }

    #[test]
    fn a_wait_is_due_only_when_its_time_passed_and_nobody_must_confirm() {
        let wait = LimitWait {
            account: AccountId::new("personal").unwrap(),
            until: Some(100),
            since: 0,
            ready: false,
        };
        assert!(!wait.due(99));
        assert!(wait.due(100));
        assert!(
            !LimitWait {
                ready: true,
                ..wait.clone()
            }
            .due(200)
        );
        assert!(
            !LimitWait {
                until: None,
                ..wait
            }
            .due(200)
        );
    }

    #[test]
    fn closed_statuses() {
        assert!(TaskStatus::Accepted.is_closed());
        assert!(!TaskStatus::Review.is_closed());
    }
}
