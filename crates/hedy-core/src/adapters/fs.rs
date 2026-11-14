use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use crate::application::ports::{FileSystem, PortError, PortResult};
use crate::domain::AccountId;

pub struct LocalFileSystem {
    accounts_dir: PathBuf,
}

impl LocalFileSystem {
    pub fn new(accounts_dir: PathBuf) -> Self {
        Self { accounts_dir }
    }
}

fn io_error(action: &str, path: &Path, err: std::io::Error) -> PortError {
    PortError::new(format!("no se pudo {action} {}: {err}", path.display()))
}

impl FileSystem for LocalFileSystem {
    fn account_folder(&self, id: &AccountId) -> PathBuf {
        self.accounts_dir.join(id.as_str())
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
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"));

        assert_eq!(fs_.git_root(&nested), Some(root.clone()));
        assert_eq!(fs_.git_root(tmp.path()), None);

        let file = root.join("config.toml");
        fs_.write_if_missing(&file, "first").unwrap();
        fs_.write_if_missing(&file, "second").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "first");
    }

    #[cfg(unix)]
    #[test]
    fn account_folders_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().to_owned());
        let folder = fs_.account_folder(&AccountId::new("personal").unwrap());
        fs_.create_private_dir(&folder).unwrap();
        let mode = fs::metadata(&folder).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
