use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{AccountId, Color, DomainError, PhasePlan};

/// A folder (usually a git repository root) registered in Hedy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// Absolute, canonical path. It is the project's identity.
    pub path: PathBuf,
    pub name: String,
    pub account: Option<AccountId>,
    /// Opt-in (off by default): only `account` may be used here, and that account
    /// is not offered to other projects.
    pub protected: bool,
    pub color: Option<Color>,
    /// Who plans, implements, writes tests and reviews.
    pub phases: PhasePlan,
    /// Accounts that do not read the project's memory ("Quién la lee").
    pub memory_off: BTreeSet<AccountId>,
    /// Who the work is for, to add up cost per client.
    pub client: Option<String>,
    /// Files the AI does not read here ("Archivos que la IA no lee"); empty
    /// by default, as in Claude Code.
    pub blocked_files: super::permissions::BlockedFiles,
    /// The permission mode its new tasks start with.
    pub permission_mode: super::task::PermissionMode,
    /// The flow its new «Cambiar código» tasks start with (a flow's key);
    /// `None`: one AI (docs/decisions.md §56).
    pub default_flow: Option<String>,
    /// How its «Cambiar código» tasks work with one AI: the plan and the
    /// checks Hedy runs (docs/decisions.md §60). Kept per PC.
    pub working: super::WorkingStyle,
}

impl Project {
    pub fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Self {
            path,
            name,
            account: None,
            protected: false,
            color: None,
            phases: PhasePlan::new(),
            memory_off: BTreeSet::new(),
            client: None,
            blocked_files: Default::default(),
            permission_mode: Default::default(),
            default_flow: None,
            working: Default::default(),
        }
    }

    pub fn contains(&self, dir: &Path) -> bool {
        dir.starts_with(&self.path)
    }
}

/// Longest folder name a new project gets.
pub const MAX_FOLDER_NAME: usize = 64;

/// The folder a new project called `name` gets: lowercase letters, digits,
/// dashes, dots and underscores, accents folded ("Mi tienda online" →
/// `mi-tienda-online`). Safe on every platform and in a shell.
pub fn folder_name(name: &str) -> Result<String, DomainError> {
    let mut folder = String::new();
    for c in name.trim().chars() {
        let c = super::account::fold_accent(c).to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            folder.push(c);
        } else if !folder.is_empty() && !folder.ends_with('-') {
            folder.push('-');
        }
    }
    let folder: String = folder.chars().take(MAX_FOLDER_NAME).collect();
    let folder = folder.trim_matches(|c| c == '-' || c == '.');
    if folder.is_empty() || !folder.chars().any(|c| c.is_ascii_alphanumeric()) {
        return Err(DomainError::InvalidProjectName(name.trim().to_owned()));
    }
    Ok(folder.to_owned())
}

/// The innermost registered project containing `dir`.
pub fn find_for_dir<'a>(projects: &'a [Project], dir: &Path) -> Option<&'a Project> {
    projects
        .iter()
        .filter(|p| p.contains(dir))
        .max_by_key(|p| p.path.components().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_comes_from_folder() {
        assert_eq!(Project::new("/home/e/code/tienda".into()).name, "tienda");
    }

    #[test]
    fn folder_names_are_safe() {
        let ok = |n: &str| folder_name(n).unwrap();
        assert_eq!(ok("Mi tienda online"), "mi-tienda-online");
        assert_eq!(ok("  Año 2027: ¡Plan!  "), "ano-2027-plan");
        assert_eq!(ok("api_v2.web"), "api_v2.web");
        assert_eq!(ok("../etc/passwd"), "etc-passwd");
        assert_eq!(ok(".hidden"), "hidden");
        assert_eq!(ok(&"x".repeat(100)).len(), MAX_FOLDER_NAME);
        for bad in ["", "   ", "¿¿??", "..", "-_-"] {
            assert!(
                matches!(folder_name(bad), Err(DomainError::InvalidProjectName(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn finds_innermost_project() {
        let projects = vec![
            Project::new("/code/mono".into()),
            Project::new("/code/mono/packages/web".into()),
            Project::new("/code/other".into()),
        ];
        let found = |d: &str| find_for_dir(&projects, Path::new(d)).map(|p| p.name.clone());
        assert_eq!(found("/code/mono/packages/web/src"), Some("web".into()));
        assert_eq!(found("/code/mono/docs"), Some("mono".into()));
        assert_eq!(found("/code/monorepo"), None);
        assert_eq!(found("/elsewhere"), None);
    }
}
