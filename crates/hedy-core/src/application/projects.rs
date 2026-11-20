use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{AppError, AppResult, Hedy};
use crate::domain::{AccountId, Color, PhasePlan, Project, policy, project};
use crate::tr;

/// The empty first commit of a new project: tasks fork their worktrees from it.
pub const INITIAL_COMMIT: &str = "Initial commit";

/// Folders under the home folder where people usually keep their projects,
/// in order of preference.
const USUAL_PARENTS: &[&str] = &[
    "Proyectos",
    "Projects",
    "projects",
    "code",
    "Code",
    "dev",
    "src",
];

/// A project started from scratch ("Nuevo proyecto").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProject {
    /// As the user wrote it ("Mi tienda"); the folder gets a safe version.
    pub name: String,
    /// Absolute. Created when missing.
    pub parent: PathBuf,
    /// `None`: the default account, if there is one.
    pub account: Option<String>,
    pub protect: bool,
}

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

    /// Who plans, implements, writes tests and reviews in this project.
    pub fn set_project_phases(&self, path: &Path, phases: PhasePlan) -> AppResult<Project> {
        let mut project = self.project_at(path)?;
        for assignment in phases.values() {
            self.account(assignment.account.as_str())?;
        }
        project.phases = phases;
        policy::check_phases(&project, &self.projects()?)?;
        self.store.save_project(&project)?;
        Ok(project)
    }

    /// Unregisters the project. Nothing in its folder is touched.
    pub fn remove_project(&self, path: &Path) -> AppResult<Project> {
        let project = self.project_at(path)?;
        self.forget_outside_in(&project.path);
        self.store.delete_project(&project.path)?;
        Ok(project)
    }

    /// Where a new project goes unless the user picks another folder: where
    /// most registered projects live, else a usual projects folder in the
    /// home folder (`~/Proyectos`, `~/code`…), else `~/Proyectos`.
    pub fn default_project_parent(&self) -> AppResult<Option<PathBuf>> {
        let mut parents: BTreeMap<PathBuf, usize> = BTreeMap::new();
        for p in self.projects()? {
            if let Some(parent) = p.path.parent() {
                *parents.entry(parent.to_owned()).or_default() += 1;
            }
        }
        // The most common one; ties go to the first in path order.
        let mut best: Option<(PathBuf, usize)> = None;
        for (parent, n) in parents {
            if best.as_ref().is_none_or(|(_, most)| n > *most) {
                best = Some((parent, n));
            }
        }
        if let Some((parent, _)) = best {
            return Ok(Some(parent));
        }
        let Some(home) = self.fs.home_dir() else {
            return Ok(None);
        };
        Ok(Some(
            USUAL_PARENTS
                .iter()
                .map(|name| home.join(name))
                .find(|dir| self.fs.exists(dir))
                .unwrap_or_else(|| home.join(tr!("Proyectos", "Projects"))),
        ))
    }

    /// The folder [`Hedy::new_project`] would create for `name` in `parent`,
    /// after checking it can: a usable name, a folder that does not exist
    /// yet and a parent where files can be created. Creates nothing.
    pub fn new_project_path(&self, name: &str, parent: &Path) -> AppResult<PathBuf> {
        let folder = project::folder_name(name)?;
        if !parent.is_absolute() {
            return Err(AppError::RelativeProjectParent(parent.to_owned()));
        }
        let exists = self.fs.exists(parent);
        let parent = if exists {
            self.fs.canonicalize(parent)?
        } else {
            parent.to_owned()
        };
        let path = parent.join(folder);
        if self.fs.exists(&path) {
            return Err(AppError::ProjectFolderExists(path));
        }
        if exists {
            self.fs.check_writable(&parent)?;
        }
        Ok(path)
    }

    /// Starts a project from scratch: a new folder (never an existing one)
    /// with a git repository and an empty first commit, registered with its
    /// account. Every rule is checked before anything is created; if git
    /// fails, the new folder is removed again.
    pub fn new_project(&self, request: NewProject) -> AppResult<Project> {
        let path = self.new_project_path(&request.name, &request.parent)?;
        let account = match request.account.as_deref().filter(|a| !a.is_empty()) {
            Some(id) => Some(self.account(id)?.id),
            None => self.default_account_id()?,
        };
        let projects = self.projects()?;
        let mut project = Project::new(path.clone());
        project.name = request.name.trim().to_owned();
        if let Some(account) = &account {
            policy::check_assignment(&project, account, &projects)?;
        }
        project.account = account;
        project.protected = request.protect;
        self.check_protection(&project, &projects)?;

        self.fs.create_new_dir(&path)?;
        if let Err(error) = self.git.init_repo(&path, INITIAL_COMMIT) {
            // Only the folder just created: it did not exist a moment ago.
            let _ = self.fs.remove_dir_all(&path);
            return Err(error.into());
        }
        project.path = self.fs.canonicalize(&path)?;
        self.store.save_project(&project)?;
        Ok(project)
    }

    /// A protected project needs its own account, not shared and not the
    /// default one (which is offered everywhere).
    fn check_protection(&self, project: &Project, projects: &[Project]) -> AppResult<()> {
        if !project.protected {
            return Ok(());
        }
        policy::check_protect(project, projects)?;
        policy::check_phases(project, projects)?;
        if let Some(account) = &project.account
            && self.default_account_id()?.as_ref() == Some(account)
        {
            return Err(AppError::DefaultCannotBeProtected(account.clone()));
        }
        Ok(())
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
        project.account = Some(account.id.clone());
        if let Some(protect) = protect {
            project.protected = protect;
        }
        self.check_protection(&project, &projects)?;
        self.store.save_project(&project)?;
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{INITIAL_COMMIT, NewProject};
    use crate::application::testing::Fixture;
    use crate::application::{AppError, NewTask, Setting};
    use crate::domain::{DomainError, TaskMode};

    fn new(name: &str, parent: &str) -> NewProject {
        NewProject {
            name: name.into(),
            parent: parent.into(),
            account: Some("personal".into()),
            protect: false,
        }
    }

    #[test]
    fn a_new_project_gets_a_folder_a_repository_and_its_account() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let p = f
            .hedy
            .new_project(new("Mi tienda online", "/home/e/Proyectos"))
            .unwrap();
        let path = PathBuf::from("/home/e/Proyectos/mi-tienda-online");
        assert_eq!(p.path, path);
        assert_eq!(p.name, "Mi tienda online");
        assert_eq!(p.account.as_ref().unwrap().as_str(), "personal");
        assert!(!p.protected);
        assert_eq!(f.fs.created(), vec![path.clone()]);
        assert_eq!(
            f.git.log(),
            vec![format!("init {} {INITIAL_COMMIT}", path.display())]
        );
        assert_eq!(f.hedy.projects().unwrap(), vec![p]);

        // Tasks start right away: the empty commit is their base.
        f.hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: path,
                prompt: "Una página de inicio".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: true,
            })
            .unwrap();
        assert_eq!(f.git.worktrees().len(), 1);
    }

    #[test]
    fn an_existing_folder_is_never_touched() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_existing("/home/e/Proyectos/tienda");
        let err = f.hedy.new_project(new("Tienda", "/home/e/Proyectos"));
        assert!(matches!(err, Err(AppError::ProjectFolderExists(_))));
        assert!(f.fs.created().is_empty() && f.git.log().is_empty());
        assert!(f.hedy.projects().unwrap().is_empty());
    }

    #[test]
    fn names_and_parents_are_checked_before_creating_anything() {
        let f = Fixture::new();
        f.add_claude("Personal");
        assert!(matches!(
            f.hedy.new_project_path("¿?", Path::new("/home/e")),
            Err(AppError::Domain(DomainError::InvalidProjectName(_)))
        ));
        assert!(matches!(
            f.hedy.new_project_path("tienda", Path::new("Proyectos")),
            Err(AppError::RelativeProjectParent(_))
        ));
        f.fs.add_existing("/srv");
        f.fs.make_read_only("/srv");
        assert!(matches!(
            f.hedy.new_project_path("tienda", Path::new("/srv")),
            Err(AppError::Port(_))
        ));
        assert_eq!(
            f.hedy
                .new_project_path("Año nuevo", Path::new("/home/e/code"))
                .unwrap(),
            PathBuf::from("/home/e/code/ano-nuevo")
        );
        assert!(f.fs.created().is_empty());
    }

    #[test]
    fn a_failed_git_init_removes_the_new_folder() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.git.fail_init("git no sabe tu nombre");
        let err = f
            .hedy
            .new_project(new("Tienda", "/home/e/Proyectos"))
            .unwrap_err();
        assert!(err.to_string().contains("git no sabe tu nombre"));
        let path = PathBuf::from("/home/e/Proyectos/tienda");
        assert_eq!(f.fs.deleted_trees(), vec![path]);
        assert!(f.hedy.projects().unwrap().is_empty());
    }

    #[test]
    fn new_projects_follow_the_protection_rules() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("personal"))
            .unwrap();
        f.hedy
            .use_account(&f.dir("/code/tienda"), "trabajo", Some(true))
            .unwrap();
        let reserved = f.hedy.new_project(NewProject {
            account: Some("trabajo".into()),
            ..new("Hobby", "/code")
        });
        assert!(matches!(
            reserved,
            Err(AppError::Domain(DomainError::AccountReserved { .. }))
        ));
        let protected_default = f.hedy.new_project(NewProject {
            protect: true,
            ..new("Hobby", "/code")
        });
        assert!(matches!(
            protected_default,
            Err(AppError::DefaultCannotBeProtected(_))
        ));
        assert!(f.fs.created().is_empty(), "nothing created on a refusal");

        // Without an account, the default one.
        let p = f
            .hedy
            .new_project(NewProject {
                account: None,
                ..new("Hobby", "/code")
            })
            .unwrap();
        assert_eq!(p.account.unwrap().as_str(), "personal");
    }

    #[test]
    fn new_projects_go_where_the_others_live() {
        let f = Fixture::new();
        f.add_claude("Personal");
        assert_eq!(
            f.hedy.default_project_parent().unwrap(),
            Some(PathBuf::from("/home/e/Proyectos"))
        );
        f.fs.add_existing("/home/e/code");
        assert_eq!(
            f.hedy.default_project_parent().unwrap(),
            Some(PathBuf::from("/home/e/code"))
        );
        for dir in ["/work/a", "/work/b", "/home/e/x"] {
            f.hedy.use_account(&f.dir(dir), "personal", None).unwrap();
        }
        assert_eq!(
            f.hedy.default_project_parent().unwrap(),
            Some(PathBuf::from("/work"))
        );
    }

    use std::path::Path;

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
            .use_account(&f.dir("/code/tienda"), "trabajo", Some(true))
            .unwrap();
        let ids = |dir: &str| -> Vec<String> {
            f.hedy
                .offerable_accounts(&f.dir(dir))
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        assert_eq!(ids("/code/tienda"), vec!["trabajo"]);
        assert_eq!(ids("/elsewhere"), vec!["personal"]);

        let color = crate::domain::Color::nth_in_palette(4);
        let p = f
            .hedy
            .set_project_color(&f.dir("/code/tienda"), Some(color))
            .unwrap();
        assert_eq!(p.color, Some(color));
        assert!(matches!(
            f.hedy.set_project_color(&f.dir("/code/nope"), None),
            Err(AppError::ProjectNotFound(_))
        ));

        f.hedy.remove_project(&f.dir("/code/tienda")).unwrap();
        assert!(f.hedy.projects().unwrap().is_empty());
        assert_eq!(ids("/elsewhere"), vec!["personal", "trabajo"]);
    }

    #[test]
    fn protected_account_cannot_be_used_elsewhere() {
        let f = Fixture::new();
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "trabajo", Some(true))
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
