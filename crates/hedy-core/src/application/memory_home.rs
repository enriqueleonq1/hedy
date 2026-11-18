//! Where each project's memory lives (docs/decisions.md §34): the user's
//! own choice for a project, else what its repos' owners say
//! ([`derive_home`]). Answers are kept per owner and server, so one answer
//! covers every repo of that owner, on this PC and in later projects. Nobody
//! is the user's until the user says so.

use std::path::Path;

use super::{AppResult, Hedy};
use crate::domain::sync::{DerivedHome, MemoryHome, Owners, RepoIdentity, derive_home};

/// The answers about repo owners (JSON [`Owners`]).
const OWNERS: &str = "memory.owners";

/// What the app shows for a project's memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryHomeStatus {
    /// Where it lives now; `None`: not decided, it stays on this PC.
    pub home: Option<MemoryHome>,
    /// The user chose it for this project (it wins over its owner).
    pub chosen: bool,
    /// The owner to ask about ("¿La memoria de … es solo tuya?"), as its
    /// owner key (`github.com/empresa-org`).
    pub ask: Option<String>,
}

impl Hedy {
    pub fn repo_owners(&self) -> AppResult<Owners> {
        Ok(self
            .store
            .get_setting(OWNERS)?
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default())
    }

    fn save_owners(&self, owners: &Owners) -> AppResult<()> {
        let text = serde_json::to_string(owners).expect("owners serialize");
        Ok(self.store.set_setting(OWNERS, Some(&text))?)
    }

    /// Whether `owner`'s repos hold the user's own memory or a group's.
    pub fn answer_owner(&self, owner: &str, mine: bool) -> AppResult<()> {
        let mut owners = self.repo_owners()?;
        owners.answer(owner, mine);
        self.save_owners(&owners)
    }

    /// Forgets the answer about `owner`: its projects ask again.
    pub fn forget_owner(&self, owner: &str) -> AppResult<()> {
        let mut owners = self.repo_owners()?;
        owners.forget(owner);
        self.save_owners(&owners)
    }

    /// Who owns each remote of the project in `path`; `None` for one that
    /// does not say (or cannot be read), which keeps the project here.
    fn remote_owners(&self, path: &Path) -> Vec<Option<RepoIdentity>> {
        let Ok(names) = self.source.remotes(path) else {
            return vec![None];
        };
        names
            .iter()
            .map(|name| {
                let url = self.source.remote_url(path, name).ok().flatten()?;
                RepoIdentity::from_url(&url)
            })
            .collect()
    }

    fn derived_home(&self, path: &Path) -> AppResult<DerivedHome> {
        Ok(derive_home(&self.remote_owners(path), &self.repo_owners()?))
    }

    pub fn memory_home_status(&self, project: &Path) -> AppResult<MemoryHomeStatus> {
        let project = self.project_at(project)?;
        if let Some(home) = self.store.memory_home(&project.path)? {
            return Ok(MemoryHomeStatus {
                home: Some(home),
                chosen: true,
                ask: None,
            });
        }
        Ok(match self.derived_home(&project.path)? {
            DerivedHome::Known(home) => MemoryHomeStatus {
                home: Some(home),
                chosen: false,
                ask: None,
            },
            DerivedHome::Ask(owner) => MemoryHomeStatus {
                home: None,
                chosen: false,
                ask: Some(owner),
            },
            DerivedHome::NoRemote => MemoryHomeStatus {
                home: None,
                chosen: false,
                ask: None,
            },
        })
    }

    /// Where the project's memory lives now (`None`: on this PC only).
    pub(super) fn effective_memory_home(&self, path: &Path) -> AppResult<Option<MemoryHome>> {
        if let Some(home) = self.store.memory_home(path)? {
            return Ok(Some(home));
        }
        Ok(match self.derived_home(path)? {
            DerivedHome::Known(home) => Some(home),
            DerivedHome::Ask(_) | DerivedHome::NoRemote => None,
        })
    }

    /// The user's choice for `project` (`None`: follow its owner again).
    pub fn set_memory_home(&self, project: &Path, home: Option<&MemoryHome>) -> AppResult<()> {
        let project = self.project_at(project)?;
        Ok(self.store.set_memory_home(&project.path, home)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::testing::Fixture;

    fn project(f: &Fixture, path: &str, remotes: &[(&str, &str)]) -> std::path::PathBuf {
        f.add_claude("Personal");
        f.fs.add_git_root(path);
        let dir = f.dir(path);
        f.hedy.use_account(&dir, "personal", None).unwrap();
        f.source.set_remotes(remotes);
        dir
    }

    #[test]
    fn a_new_owner_asks_once_and_the_answer_covers_its_repos() {
        let f = Fixture::new();
        let app = project(
            &f,
            "/w/app",
            &[("origin", "git@github.com:empresa-org/app.git")],
        );
        let status = f.hedy.memory_home_status(&app).unwrap();
        assert_eq!(status.ask.as_deref(), Some("github.com/empresa-org"));
        assert_eq!(status.home, None);

        f.hedy
            .answer_owner("github.com/empresa-org", false)
            .unwrap();
        let status = f.hedy.memory_home_status(&app).unwrap();
        assert_eq!(
            status.home,
            Some(MemoryHome::Shared("github.com/empresa-org".into()))
        );
        assert!(!status.chosen && status.ask.is_none());

        // The user's own choice for the project wins, until cleared.
        f.hedy
            .set_memory_home(&app, Some(&MemoryHome::Local))
            .unwrap();
        let status = f.hedy.memory_home_status(&app).unwrap();
        assert_eq!(
            (status.home, status.chosen),
            (Some(MemoryHome::Local), true)
        );
        f.hedy.set_memory_home(&app, None).unwrap();
        f.hedy.forget_owner("github.com/empresa-org").unwrap();
        assert!(f.hedy.memory_home_status(&app).unwrap().ask.is_some());
    }

    #[test]
    fn a_fork_follows_any_remote_of_a_group() {
        let f = Fixture::new();
        let fork = project(
            &f,
            "/w/app",
            &[
                ("origin", "git@github.com:tu-usuario/app.git"),
                ("empresa", "https://github.com/empresa-org/app"),
            ],
        );
        f.hedy.answer_owner("github.com/tu-usuario", true).unwrap();
        let status = f.hedy.memory_home_status(&fork).unwrap();
        assert_eq!(status.ask.as_deref(), Some("github.com/empresa-org"));
        f.hedy
            .answer_owner("github.com/empresa-org", false)
            .unwrap();
        assert_eq!(
            f.hedy.effective_memory_home(&fork).unwrap(),
            Some(MemoryHome::Shared("github.com/empresa-org".into()))
        );
    }

    #[test]
    fn a_remote_that_does_not_say_its_owner_keeps_the_project_here() {
        let f = Fixture::new();
        let app = project(
            &f,
            "/w/app",
            &[
                ("origin", "git@github.com:tu-usuario/app.git"),
                ("upstream", "git@git.empresa.com:app.git"),
            ],
        );
        f.hedy.answer_owner("github.com/tu-usuario", true).unwrap();
        assert_eq!(f.hedy.effective_memory_home(&app).unwrap(), None);
    }
}
