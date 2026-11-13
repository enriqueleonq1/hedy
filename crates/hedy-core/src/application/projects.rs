use std::path::Path;

use super::{AppError, AppResult, Hedy};
use crate::domain::{AccountId, Color, Project, policy, project};

impl Hedy {
    pub fn projects(&self) -> AppResult<Vec<Project>> {
        Ok(self.store.list_projects()?)
    }

    /// The registered project containing `dir`, if any.
    pub fn project_for(&self, dir: &Path) -> AppResult<Option<Project>> {
        let dir = self.fs.canonicalize(dir)?;
        Ok(project::find_for_dir(&self.projects()?, &dir).cloned())
    }

    /// The project registered exactly at `path`.
    pub fn project_at(&self, path: &Path) -> AppResult<Project> {
        let path = self.fs.canonicalize(path)?;
        self.projects()?
            .into_iter()
            .find(|p| p.path == path)
            .ok_or(AppError::ProjectNotFound(path))
    }

    /// Accounts that may be offered in the project containing `dir` (or outside
    /// any project), following the protection rules.
    pub fn offerable_accounts(&self, dir: &Path) -> AppResult<Vec<AccountId>> {
        let project = self.project_for(dir)?;
        let (accounts, projects) = (self.accounts()?, self.projects()?);
        Ok(policy::offerable(project.as_ref(), &accounts, &projects)
            .into_iter()
            .map(|a| a.id.clone())
            .collect())
    }

    pub fn set_project_color(&self, path: &Path, color: Option<Color>) -> AppResult<Project> {
        let mut project = self.project_at(path)?;
        project.color = color;
        self.store.save_project(&project)?;
        Ok(project)
    }

    /// Unregisters the project. Nothing in its folder is touched.
    pub fn remove_project(&self, path: &Path) -> AppResult<Project> {
        let project = self.project_at(path)?;
        self.store.delete_project(&project.path)?;
        Ok(project)
    }

    /// Assigns `account` to the project containing `dir`. Registers the project
    /// (at the git root, or at `dir` outside git) when it is new.
    /// `protect`: `Some(true)` / `Some(false)` changes protection, `None` keeps it.
    pub fn use_account(
        &self,
        dir: &Path,
        account: &str,
        protect: Option<bool>,
    ) -> AppResult<Project> {
        let account = self.account(account)?;
        let projects = self.projects()?;
        let mut project = match self.project_for(dir)? {
            Some(p) => p,
            None => {
                let dir = self.fs.canonicalize(dir)?;
                Project::new(self.fs.git_root(&dir).unwrap_or(dir))
            }
        };

        policy::check_assignment(&project, &account.id, &projects)?;
        project.account = Some(account.id);
        if let Some(protect) = protect {
            project.protected = protect;
        }
        if project.protected {
            policy::check_protect(&project, &projects)?;
            // The default account is offered everywhere, so it cannot be reserved.
            if self.default_account_id()? == project.account {
                return Err(AppError::DefaultCannotBeProtected(account_id(&project)));
            }
        }
        self.store.save_project(&project)?;
        Ok(project)
    }
}

fn account_id(project: &Project) -> AccountId {
    project
        .account
        .clone()
        .expect("checked by policy::check_protect")
}

#[cfg(test)]
mod tests {
    use crate::application::AppError;
    use crate::application::testing::Fixture;
    use crate::domain::DomainError;

    #[test]
    fn registers_the_git_root_as_project() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/hedy");
        let p = f
            .hedy
            .use_account(&f.dir("/code/hedy/crates/core"), "personal", None)
            .unwrap();
        assert_eq!(p.path, f.dir("/code/hedy"));
        assert_eq!(p.name, "hedy");
        assert!(!p.protected, "protection is opt-in");
        assert_eq!(f.hedy.projects().unwrap(), vec![p]);
    }

    #[test]
    fn reassigning_updates_the_same_project() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/x"), "personal", None)
            .unwrap();
        let p = f
            .hedy
            .use_account(&f.dir("/code/x/src"), "trabajo", None)
            .unwrap();
        assert_eq!(p.account.unwrap().as_str(), "trabajo");
        assert_eq!(f.hedy.projects().unwrap().len(), 1);
    }

    #[test]
    fn color_offer_and_removal() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/homming"), "trabajo", Some(true))
            .unwrap();
        let ids = |dir: &str| -> Vec<String> {
            f.hedy
                .offerable_accounts(&f.dir(dir))
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        assert_eq!(ids("/code/homming"), vec!["trabajo"]);
        assert_eq!(ids("/elsewhere"), vec!["personal"]);

        let color = crate::domain::Color::nth_in_palette(4);
        let p = f
            .hedy
            .set_project_color(&f.dir("/code/homming"), Some(color))
            .unwrap();
        assert_eq!(p.color, Some(color));
        assert!(matches!(
            f.hedy.set_project_color(&f.dir("/code/nope"), None),
            Err(AppError::ProjectNotFound(_))
        ));

        f.hedy.remove_project(&f.dir("/code/homming")).unwrap();
        assert!(f.hedy.projects().unwrap().is_empty());
        assert_eq!(ids("/elsewhere"), vec!["personal", "trabajo"]);
    }

    #[test]
    fn protected_account_cannot_be_used_elsewhere() {
        let f = Fixture::new();
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/homming"), "trabajo", Some(true))
            .unwrap();
        let err = f
            .hedy
            .use_account(&f.dir("/code/hobby"), "trabajo", None)
            .unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::AccountReserved { .. })
        ));
    }

    #[test]
    fn protecting_a_shared_account_fails_until_unshared() {
        let f = Fixture::new();
        f.add_claude("Trabajo");
        f.add_claude("Personal");
        f.hedy
            .use_account(&f.dir("/code/a"), "trabajo", None)
            .unwrap();
        f.hedy
            .use_account(&f.dir("/code/b"), "trabajo", None)
            .unwrap();
        assert!(matches!(
            f.hedy.use_account(&f.dir("/code/a"), "trabajo", Some(true)),
            Err(AppError::Domain(DomainError::AccountShared { .. }))
        ));
        f.hedy
            .use_account(&f.dir("/code/b"), "personal", None)
            .unwrap();
        let a = f
            .hedy
            .use_account(&f.dir("/code/a"), "trabajo", Some(true))
            .unwrap();
        assert!(a.protected);
        let a = f
            .hedy
            .use_account(&f.dir("/code/a"), "trabajo", Some(false))
            .unwrap();
        assert!(!a.protected);
    }
}
