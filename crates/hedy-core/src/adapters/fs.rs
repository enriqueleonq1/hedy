use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use crate::application::ports::{FileSystem, PortError, PortResult};
use crate::domain::AccountId;

pub struct LocalFileSystem {
    accounts_dir: PathBuf,
    chats_dir: PathBuf,
}

impl LocalFileSystem {
    pub fn new(accounts_dir: PathBuf, chats_dir: PathBuf) -> Self {
        Self {
            accounts_dir,
            chats_dir,
        }
    }
}

fn io_error(action: &str, path: &Path, err: std::io::Error) -> PortError {
    PortError::new(format!("no se pudo {action} {}: {err}", path.display()))
}

impl FileSystem for LocalFileSystem {
    fn account_folder(&self, id: &AccountId) -> PathBuf {
        self.accounts_dir.join(id.as_str())
    }

    fn chats_dir(&self) -> PathBuf {
        self.chats_dir.clone()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        directories::BaseDirs::new().map(|d| d.home_dir().to_owned())
    }

    fn create_new_dir(&self, path: &Path) -> PortResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error("crear", parent, e))?;
        }
        // `create_dir` fails on an existing folder or file: never overwrite.
        fs::create_dir(path).map_err(|e| io_error("crear", path, e))
    }

    fn remove_dir_all(&self, path: &Path) -> PortResult<()> {
        match fs::remove_dir_all(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(io_error("borrar", path, e)),
            _ => Ok(()),
        }
    }

    fn create_private_dir(&self, path: &Path) -> PortResult<()> {
        fs::create_dir_all(path).map_err(|e| io_error("crear", path, e))?;
        // Account folders hold CLI logins: private to the user.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .map_err(|e| io_error("proteger", path, e))?;
        }
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn check_writable(&self, dir: &Path) -> PortResult<()> {
        let probe = dir.join(format!(".hedy-doctor-{}", std::process::id()));
        fs::write(&probe, b"").map_err(|e| io_error("escribir en", dir, e))?;
        fs::remove_file(&probe).map_err(|e| io_error("borrar", &probe, e))
    }

    fn symlink_dir(&self, target: &Path, link: &Path) -> PortResult<()> {
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(target, link);
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(target, link);
        linked.map_err(|e| io_error("enlazar", link, e))
    }

    fn write_if_missing(&self, path: &Path, contents: &str) -> PortResult<()> {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => file
                .write_all(contents.as_bytes())
                .map_err(|e| io_error("escribir", path, e)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(io_error("crear", path, e)),
        }
    }

    fn read_text(&self, path: &Path) -> PortResult<Option<String>> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_error("leer", path, e)),
        }
    }

    fn write_text(&self, path: &Path, contents: &str) -> PortResult<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| io_error("crear", dir, e))?;
        }
        fs::write(path, contents).map_err(|e| io_error("escribir", path, e))
    }

    fn remove_file(&self, path: &Path) -> PortResult<()> {
        match fs::remove_file(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(io_error("borrar", path, e)),
            _ => Ok(()),
        }
    }

    fn remove_empty_dir(&self, path: &Path) -> PortResult<()> {
        let empty = fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none());
        if empty {
            fs::remove_dir(path).map_err(|e| io_error("borrar", path, e))?;
        }
        Ok(())
    }

    fn canonicalize(&self, dir: &Path) -> PortResult<PathBuf> {
        dir.canonicalize().map_err(|e| io_error("resolver", dir, e))
    }

    fn git_root(&self, dir: &Path) -> Option<PathBuf> {
        // `.git` is a folder in a normal clone and a file in a worktree.
        dir.ancestors()
            .find(|d| d.join(".git").exists())
            .map(Path::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_git_root_and_writes_once() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        let nested = root.join("a/b");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));

        assert_eq!(fs_.git_root(&nested), Some(root.clone()));
        assert_eq!(fs_.git_root(tmp.path()), None);

        let file = root.join("config.toml");
        fs_.write_if_missing(&file, "first").unwrap();
        fs_.write_if_missing(&file, "second").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "first");
    }

    #[test]
    fn text_files_are_written_read_and_removed_with_their_empty_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));
        let file = tmp.path().join(".claude/skills/run-tests/SKILL.md");
        assert_eq!(fs_.read_text(&file).unwrap(), None);
        fs_.write_text(&file, "uno").unwrap();
        fs_.write_text(&file, "dos").unwrap();
        assert_eq!(fs_.read_text(&file).unwrap().as_deref(), Some("dos"));

        let skills = tmp.path().join(".claude/skills");
        fs::write(skills.join("mine.md"), "user's").unwrap();
        fs_.remove_file(&file).unwrap();
        fs_.remove_file(&file).unwrap();
        fs_.remove_empty_dir(&skills.join("run-tests")).unwrap();
        fs_.remove_empty_dir(&skills).unwrap();
        fs_.remove_empty_dir(&tmp.path().join("missing")).unwrap();
        assert!(!skills.join("run-tests").exists());
        assert!(skills.join("mine.md").exists(), "folders with files stay");
    }

    #[test]
    fn writable_check_leaves_nothing_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().to_owned(), tmp.path().join("chats"));
        fs_.check_writable(tmp.path()).unwrap();
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
        assert!(fs_.check_writable(&tmp.path().join("missing")).is_err());
    }

    #[test]
    fn new_folders_never_replace_existing_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));
        let project = tmp.path().join("Proyectos/tienda");
        fs_.create_new_dir(&project).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}").unwrap();
        assert!(fs_.create_new_dir(&project).is_err());
        assert!(project.join("main.rs").exists(), "untouched");

        fs_.remove_dir_all(&project).unwrap();
        assert!(!project.exists());
        fs_.remove_dir_all(&project).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn account_folders_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().to_owned(), tmp.path().join("chats"));
        let folder = fs_.account_folder(&AccountId::new("personal").unwrap());
        fs_.create_private_dir(&folder).unwrap();
        let mode = fs::metadata(&folder).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
