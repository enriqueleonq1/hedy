//! A task: one request worked on in its own git worktree and branch.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::{AccountId, DomainError};

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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Ask => "ask",
            Self::Research => "research",
        }
    }

    pub fn changes_files(self) -> bool {
        self == Self::Project
    }
}

impl FromStr for TaskMode {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "project" => Ok(Self::Project),
            "ask" => Ok(Self::Ask),
            "research" => Ok(Self::Research),
            other => Err(DomainError::UnknownTaskMode(other.to_owned())),
        }
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::WaitingApproval => "waitingApproval",
            Self::Review => "review",
            Self::Accepted => "accepted",
            Self::Discarded => "discarded",
            Self::Failed => "failed",
        }
    }

    /// Accepted and discarded tasks are closed: no more messages, no worktree.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Accepted | Self::Discarded)
    }
}

impl FromStr for TaskStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        [
            Self::Running,
            Self::WaitingApproval,
            Self::Review,
            Self::Accepted,
            Self::Discarded,
            Self::Failed,
        ]
        .into_iter()
        .find(|status| status.as_str() == s)
        .ok_or_else(|| DomainError::UnknownTaskStatus(s.to_owned()))
    }
}

/// Where a `Project` task works: its branch, forked from `base_branch` at `base_commit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub worktree: PathBuf,
    pub branch: String,
    pub base_branch: String,
    pub base_commit: String,
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
    /// The CLI's session, to resume it after a restart.
    pub session_id: Option<String>,
    pub status: TaskStatus,
    pub created_at: i64,
}

impl Task {
    /// Folder the agent works in: the worktree, or the project for read-only modes.
    pub fn work_dir(&self) -> PathBuf {
        self.workspace
            .as_ref()
            .map_or_else(|| self.project.clone(), |w| w.worktree.clone())
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
        let c = match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        };
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
    fn statuses_round_trip() {
        for s in [
            "running",
            "waitingApproval",
            "review",
            "accepted",
            "discarded",
            "failed",
        ] {
            assert_eq!(s.parse::<TaskStatus>().unwrap().as_str(), s);
        }
        assert!(TaskStatus::Accepted.is_closed());
        assert!(!TaskStatus::Review.is_closed());
        assert_eq!("ask".parse::<TaskMode>().unwrap(), TaskMode::Ask);
    }
}
