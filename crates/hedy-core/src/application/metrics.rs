//! Project status and usage per project and client.

use std::path::Path;

use super::{AppResult, Hedy, Setting};
use crate::domain::memory::is_stale;
use crate::domain::metrics::{ClientUsage, ProjectUsage, by_client};
use crate::domain::{Account, AgentEvent, NoteStatus, Project, UsageSample};
use crate::tr;

const DAY_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq)]
pub struct UsageSummary {
    /// Start of the period (seconds).
    pub since: i64,
    /// Busiest first.
    pub projects: Vec<ProjectUsage>,
    pub clients: Vec<ClientUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRun {
    pub command: String,
    pub passed: bool,
    pub at: i64,
}

/// The header and indicators of the project status screen.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectStatus {
    pub project: Project,
    /// `None` when the folder is not a git repository (or HEAD is detached).
    pub branch: Option<String>,
    pub uncommitted: Option<usize>,
    /// The latest test run of any task in the project.
    pub last_tests: Option<TestRun>,
    /// Accounts that work here: the project's and the ones in its phases.
    pub agents: Vec<Account>,
    pub code_map: bool,
    pub memory_notes: usize,
    pub memory_stale: usize,
    pub memory_proposed: usize,
}

impl Hedy {
    /// Who the project's work is for. Empty clears it.
    pub fn set_project_client(&self, path: &Path, client: Option<&str>) -> AppResult<Project> {
        let mut project = self.project_at(path)?;
        project.client = client
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        self.store.save_project(&project)?;
        Ok(project)
    }

    /// Usage of the last `days` days, per project and per client.
    pub fn usage_summary(&self, days: u32) -> AppResult<UsageSummary> {
        let since = self.now_secs() - i64::from(days) * DAY_SECS;
        let registered = self.projects()?;
        // Conversations outside any project add up to one row, with no client.
        let chats = self.fs.chats_dir();
        let mut rows = Vec::new();
        let mut conversations: Option<UsageSample> = None;
        for (path, usage) in self.store.usage_by_project(since)? {
            if path.starts_with(&chats) {
                conversations
                    .get_or_insert_with(UsageSample::default)
                    .add(&usage);
            } else {
                rows.push((path, usage));
            }
        }
        rows.extend(conversations.map(|usage| (chats.clone(), usage)));
        let mut projects: Vec<ProjectUsage> = rows
            .into_iter()
            .map(|(path, usage)| {
                if path == chats {
                    return ProjectUsage {
                        name: tr!("Conversaciones", "Conversations"),
                        client: None,
                        project: path,
                        usage,
                    };
                }
                let known = registered.iter().find(|p| p.path == path);
                let fallback = Project::new(path.clone());
                let project = known.unwrap_or(&fallback);
                ProjectUsage {
                    name: project.name.clone(),
                    client: project.client.clone(),
                    project: path,
                    usage,
                }
            })
            .collect();
        projects.sort_by_key(|p| std::cmp::Reverse(p.usage.total_tokens()));
        Ok(UsageSummary {
            since,
            clients: by_client(&projects),
            projects,
        })
    }

    /// Every usage record of a project in the last `days` days, oldest first.
    /// The UI groups them by local day.
    pub fn project_usage(&self, path: &Path, days: u32) -> AppResult<Vec<(i64, UsageSample)>> {
        let project = self.project_at(path)?;
        let since = self.now_secs() - i64::from(days) * DAY_SECS;
        Ok(self.store.usage_points(&project.path, since)?)
    }

    pub fn project_status(&self, path: &Path) -> AppResult<ProjectStatus> {
        let project = self.project_at(path)?;
        let notes = self.store.notes(Some(&project.path), !project.protected)?;
        let now = self.now_secs();
        let approved = notes.iter().filter(|n| n.status == NoteStatus::Approved);
        Ok(ProjectStatus {
            branch: self.git.current_branch(&project.path).ok(),
            uncommitted: self.git.uncommitted_changes(&project.path).ok(),
            last_tests: self.last_tests(&project.path)?,
            agents: self.project_agents(&project)?,
            code_map: self
                .setting(Setting::CodeMap)?
                .is_some_and(|p| !p.trim().is_empty()),
            memory_notes: approved.clone().count(),
            memory_stale: approved.filter(|n| is_stale(n, now)).count(),
            memory_proposed: notes
                .iter()
                .filter(|n| n.status == NoteStatus::Proposed)
                .count(),
            project,
        })
    }

    fn last_tests(&self, project: &Path) -> AppResult<Option<TestRun>> {
        // Recent tasks only: the screen shows the latest run, not history.
        for task in self.tasks(project)?.into_iter().take(20) {
            let found = self
                .store
                .timed_events(&task.id)?
                .into_iter()
                .rev()
                .find_map(|(at, event)| match event {
                    AgentEvent::TestsRun { command, passed } => Some(TestRun {
                        command,
                        passed,
                        at,
                    }),
                    _ => None,
                });
            if found.is_some() {
                return Ok(found);
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{UsageRecord, UsageRepository};
    use crate::application::testing::Fixture;
    use crate::domain::{Category, TaskId};

    fn usage(tokens: u64, cost: Option<f64>) -> UsageSample {
        UsageSample {
            input_tokens: tokens,
            cost_usd: cost,
            ..UsageSample::default()
        }
    }

    #[test]
    fn usage_per_project_and_client() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let (moira, citas) = (f.dir("/code/moira"), f.dir("/code/citas"));
        f.hedy.use_account(&moira, "personal", None).unwrap();
        f.hedy.use_account(&citas, "personal", None).unwrap();
        f.hedy
            .set_project_client(&moira, Some(" León Blanco "))
            .unwrap();
        f.hedy
            .set_project_client(&citas, Some("León Blanco"))
            .unwrap();
        let now = f.hedy.now_secs();
        for (project, at, tokens) in [
            (&moira, now, 100),
            (&citas, now, 40),
            (&moira, now - 9 * DAY_SECS, 999),
        ] {
            f.store
                .record_usage(&UsageRecord {
                    at,
                    account: f.hedy.account("personal").unwrap().id,
                    project: project.clone(),
                    task: TaskId::new("t"),
                    sample: usage(tokens, Some(0.5)),
                })
                .unwrap();
        }
        let summary = f.hedy.usage_summary(7).unwrap();
        assert_eq!(summary.projects[0].name, "moira");
        assert_eq!(
            summary.projects[0].usage.total_tokens(),
            100,
            "older records are left out"
        );
        assert_eq!(summary.clients.len(), 1);
        assert_eq!(summary.clients[0].client.as_deref(), Some("León Blanco"));
        assert_eq!(summary.clients[0].usage.cost_usd, Some(1.0));
        assert_eq!(f.hedy.project_usage(&moira, 30).unwrap().len(), 2);

        f.hedy.set_project_client(&citas, Some("  ")).unwrap();
        assert_eq!(f.hedy.project_at(&citas).unwrap().client, None);
    }

    #[test]
    fn status_of_a_project() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let moira = f.dir("/code/moira");
        f.hedy.use_account(&moira, "personal", None).unwrap();
        f.hedy
            .add_note(Some(&moira), "Precios en USD", Category::Decision)
            .unwrap();
        f.hedy
            .propose_note(
                &moira,
                "Tests con pytest",
                Category::Convention,
                "Codex",
                None,
            )
            .unwrap();
        let status = f.hedy.project_status(&moira).unwrap();
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.uncommitted, Some(3));
        assert_eq!(status.memory_notes, 1);
        assert_eq!(status.memory_proposed, 1);
        assert_eq!(status.agents.len(), 1);
        assert!(status.last_tests.is_none());
        assert!(!status.code_map);
    }
}
