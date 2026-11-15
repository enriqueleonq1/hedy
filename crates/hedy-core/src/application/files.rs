//! The project's files, for `@file` mentions in the composer: what git
//! tracks plus untracked files it does not ignore, never a protected file
//! (`.env`, keys...), at most [`MAX_PROJECT_FILES`].

use std::path::Path;

use super::{AppResult, Hedy};
use crate::domain::permissions::is_protected_path;

/// Paths offered at most; a bigger repository is cut, in path order.
pub const MAX_PROJECT_FILES: usize = 5_000;

impl Hedy {
    /// The registered project's files, relative to its root, sorted.
    pub fn project_files(&self, project: &Path) -> AppResult<Vec<String>> {
        let project = self.project_at(project)?;
        let mut files: Vec<String> = self
            .git
            .list_files(&project.path)?
            .into_iter()
            .filter(|path| !is_protected_path(path) && !path.starts_with(".hedy/"))
            .collect();
        files.sort();
        files.dedup();
        files.truncate(MAX_PROJECT_FILES);
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::AppError;
    use crate::application::testing::Fixture;

    #[test]
    fn lists_the_project_files_without_secrets() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/moira");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        f.git.set_files(&[
            "src/app.py",
            ".env",
            "config/.env.production",
            "deploy/server.pem",
            "README.md",
            ".hedy/worktrees/t1/app.py",
            "src/app.py",
        ]);
        assert_eq!(
            f.hedy.project_files(&f.dir("/code/moira")).unwrap(),
            ["README.md", "src/app.py"]
        );
    }

    #[test]
    fn caps_big_repositories_and_needs_a_registered_project() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/moira");
        f.hedy
            .use_account(&f.dir("/code/moira"), "personal", None)
            .unwrap();
        let many: Vec<String> = (0..MAX_PROJECT_FILES + 10)
            .map(|i| format!("f{i:05}.rs"))
            .collect();
        f.git
            .set_files(&many.iter().map(String::as_str).collect::<Vec<_>>());
        let files = f.hedy.project_files(&f.dir("/code/moira")).unwrap();
        assert_eq!(files.len(), MAX_PROJECT_FILES);
        assert_eq!(files[0], "f00000.rs");
        assert!(matches!(
            f.hedy.project_files(&f.dir("/code/otro")),
            Err(AppError::ProjectNotFound(_))
        ));
    }
}
