use std::fs;
use std::io::{ErrorKind, Read as _, Write as _};
use std::path::{Path, PathBuf};

use crate::application::ports::{FileSystem, PortError, PortResult};
use crate::domain::AccountId;

pub struct LocalFileSystem {
    accounts_dir: PathBuf,
    chats_dir: PathBuf,
    attachments_dir: PathBuf,
    outputs_dir: PathBuf,
}

impl LocalFileSystem {
    /// Attachments and outputs live next to the chats folder (all in Hedy's
    /// data folder).
    pub fn new(accounts_dir: PathBuf, chats_dir: PathBuf) -> Self {
        Self {
            accounts_dir,
            attachments_dir: chats_dir.with_file_name("attachments"),
            outputs_dir: chats_dir.with_file_name("outputs"),
            chats_dir,
        }
    }
}

/// Makes `path` readable only by the user.
fn make_private(path: &Path, mode: u32) -> PortResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| io_error("proteger", path, e))?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
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

    fn attachments_dir(&self) -> PathBuf {
        self.attachments_dir.clone()
    }

    fn outputs_dir(&self) -> PathBuf {
        self.outputs_dir.clone()
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
        make_private(path, 0o700)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_symlink(&self, path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
    }

    fn entries(&self, dir: &Path) -> PortResult<Vec<PathBuf>> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_error("leer", dir, e)),
        };
        entries
            .map(|entry| {
                entry
                    .map(|e| e.path())
                    .map_err(|e| io_error("leer", dir, e))
            })
            .collect()
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
        // Never through a link put where the file goes: it would write
        // wherever the link points.
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
        let mut file = options
            .open(path)
            .map_err(|e| io_error("escribir", path, e))?;
        file.write_all(contents.as_bytes())
            .map_err(|e| io_error("escribir", path, e))
    }

    fn create_text(&self, path: &Path, contents: &str) -> PortResult<bool> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| io_error("crear", dir, e))?;
        }
        // `create_new` fails on anything there, a dangling link included.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => {
                file.write_all(contents.as_bytes())
                    .map_err(|e| io_error("escribir", path, e))?;
                Ok(true)
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(io_error("crear", path, e)),
        }
    }

    fn read_regular_text(&self, path: &Path, max_bytes: u64) -> PortResult<Option<String>> {
        let meta = match fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_error("leer", path, e)),
        };
        if !meta.file_type().is_file() {
            return Err(PortError::new(format!(
                "{} no es un archivo normal",
                path.display()
            )));
        }
        let file = fs::File::open(path).map_err(|e| io_error("leer", path, e))?;
        let mut bytes = Vec::new();
        file.take(max_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error("leer", path, e))?;
        if bytes.len() as u64 > max_bytes {
            return Err(PortError::new(format!(
                "{} es demasiado grande",
                path.display()
            )));
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| PortError::new(format!("{} no es texto UTF-8", path.display())))
    }

    fn write_private_file(&self, path: &Path, contents: &[u8]) -> PortResult<()> {
        if let Some(dir) = path.parent() {
            self.create_private_dir(dir)?;
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(|e| io_error("crear", path, e))?;
        file.write_all(contents)
            .map_err(|e| io_error("escribir", path, e))
    }

    fn read_bytes(&self, path: &Path) -> PortResult<Option<Vec<u8>>> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_error("leer", path, e)),
        }
    }

    fn files_beneath(&self, dir: &Path) -> PortResult<Vec<(String, u64)>> {
        // Enough for any document or page; a runaway folder is cut here
        // (files listed, and everything looked at, empty folders too).
        const MAX: usize = 2_000;
        const VISITED: usize = 20_000;
        let mut found = Vec::new();
        let mut visited = 0;
        let mut pending = vec![dir.to_owned()];
        while let Some(folder) = pending.pop() {
            let entries = match fs::read_dir(&folder) {
                Ok(entries) => entries,
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(io_error("leer", &folder, e)),
            };
            for entry in entries.flatten() {
                visited += 1;
                if visited > VISITED {
                    pending.clear();
                    break;
                }
                let path = entry.path();
                // Not followed: a link could lead anywhere on the disk.
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if meta.is_dir() {
                    pending.push(path);
                } else if meta.is_file() && found.len() < MAX {
                    let rel = path.strip_prefix(dir).unwrap_or(&path);
                    let rel: Vec<String> = rel
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect();
                    found.push((rel.join("/"), meta.len()));
                }
            }
        }
        found.sort();
        Ok(found)
    }

    fn copy_to_new(&self, from: &Path, to: &Path) -> PortResult<()> {
        let meta = fs::symlink_metadata(from).map_err(|e| io_error("leer", from, e))?;
        if !meta.is_file() {
            return Err(PortError::new(format!(
                "{} no es un archivo normal",
                from.display()
            )));
        }
        let mut source = fs::File::open(from).map_err(|e| io_error("leer", from, e))?;
        // Swapped for a link between the check and the open: not the file
        // that was checked.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let opened = source.metadata().map_err(|e| io_error("leer", from, e))?;
            if (opened.dev(), opened.ino()) != (meta.dev(), meta.ino()) {
                return Err(PortError::new(format!(
                    "{} cambió mientras se copiaba",
                    from.display()
                )));
            }
        }
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|e| io_error("crear", to, e))?;
        std::io::copy(&mut source, &mut target).map_err(|e| io_error("copiar", to, e))?;
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> PortResult<()> {
        match fs::remove_file(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(io_error("borrar", path, e)),
            _ => Ok(()),
        }
    }

    fn write_text_beneath(&self, root: &Path, rel: &Path, contents: &str) -> PortResult<()> {
        let path = root.join(rel);
        beneath::write(root, rel, contents.as_bytes()).map_err(|e| io_error("escribir", &path, e))
    }

    fn remove_file_beneath(&self, root: &Path, rel: &Path) -> PortResult<()> {
        let path = root.join(rel);
        match beneath::remove(root, rel) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(io_error("borrar", &path, e)),
            _ => Ok(()),
        }
    }

    fn remove_empty_dir_beneath(&self, root: &Path, rel: &Path) -> PortResult<()> {
        let path = root.join(rel);
        match beneath::remove_dir(root, rel) {
            Err(e) if !matches!(e.kind(), ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty) => {
                Err(io_error("borrar", &path, e))
            }
            _ => Ok(()),
        }
    }

    fn remove_empty_dir(&self, path: &Path) -> PortResult<()> {
        if self.is_symlink(path) {
            return Ok(());
        }
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

/// Writing and deleting inside a folder without ever following a link on
/// the way: each folder is opened from the one before with `O_NOFOLLOW`, so a
/// folder swapped for a link meanwhile makes the call fail, never write
/// elsewhere.
mod beneath {
    use std::io;
    use std::path::{Component, Path};

    /// The names of `rel`'s parts; `..`, a root or a prefix are refused.
    fn parts(rel: &Path) -> io::Result<Vec<&std::ffi::OsStr>> {
        rel.components()
            .map(|c| match c {
                Component::Normal(name) => Ok(name),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "not a plain relative path",
                )),
            })
            .collect()
    }

    #[cfg(unix)]
    mod unix {
        use std::ffi::{CString, OsStr};
        use std::io;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::os::unix::ffi::OsStrExt;
        use std::path::Path;

        fn c(name: &OsStr) -> io::Result<CString> {
            CString::new(name.as_bytes())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in a name"))
        }

        fn open_root(root: &Path) -> io::Result<OwnedFd> {
            let path = c(root.as_os_str())?;
            // SAFETY: a valid C string; the descriptor is owned right away.
            let fd = unsafe {
                libc::open(
                    path.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `fd` was just opened and is not used elsewhere.
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }

        /// The folder `name` inside `dir`, never a link; made when `create`.
        fn step(dir: &OwnedFd, name: &OsStr, create: bool) -> io::Result<OwnedFd> {
            let name = c(name)?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            // SAFETY: a valid descriptor and C string.
            let mut fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 && create && io::Error::last_os_error().kind() == io::ErrorKind::NotFound {
                // SAFETY: as above; a folder that appears meanwhile is opened next.
                let made = unsafe { libc::mkdirat(dir.as_raw_fd(), name.as_ptr(), 0o755) };
                if made < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                    return Err(io::Error::last_os_error());
                }
                // SAFETY: as above.
                fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
            }
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `fd` was just opened and is not used elsewhere.
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }

        pub fn write(root: &Path, parts: &[&OsStr], bytes: &[u8]) -> io::Result<()> {
            let (file, folders) = parts.split_last().ok_or(io::ErrorKind::InvalidInput)?;
            let mut dir = open_root(root)?;
            for folder in folders {
                dir = step(&dir, folder, true)?;
            }
            let name = c(file)?;
            let flags =
                libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            // SAFETY: a valid descriptor and C string.
            let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags, 0o644) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `fd` was just opened and is not used elsewhere.
            let mut out = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) });
            std::io::Write::write_all(&mut out, bytes)
        }

        pub fn remove(root: &Path, parts: &[&OsStr], folder: bool) -> io::Result<()> {
            let (file, folders) = parts.split_last().ok_or(io::ErrorKind::InvalidInput)?;
            let mut dir = open_root(root)?;
            for folder in folders {
                dir = step(&dir, folder, false)?;
            }
            let name = c(file)?;
            // A folder only when empty; a link there is never a folder.
            let flags = if folder { libc::AT_REMOVEDIR } else { 0 };
            // SAFETY: a valid descriptor and C string; unlinkat never follows
            // a link at the name itself.
            if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), flags) } < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    pub fn write(root: &Path, rel: &Path, bytes: &[u8]) -> io::Result<()> {
        let parts = parts(rel)?;
        #[cfg(unix)]
        return unix::write(root, &parts, bytes);
        #[cfg(not(unix))]
        {
            check(root, &parts)?;
            let path = root.join(rel);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, bytes)
        }
    }

    pub fn remove(root: &Path, rel: &Path) -> io::Result<()> {
        let parts = parts(rel)?;
        #[cfg(unix)]
        return unix::remove(root, &parts, false);
        #[cfg(not(unix))]
        {
            check(root, &parts)?;
            std::fs::remove_file(root.join(rel))
        }
    }

    pub fn remove_dir(root: &Path, rel: &Path) -> io::Result<()> {
        let parts = parts(rel)?;
        #[cfg(unix)]
        return unix::remove(root, &parts, true);
        #[cfg(not(unix))]
        {
            check(root, &parts)?;
            std::fs::remove_dir(root.join(rel))
        }
    }

    /// Without `openat`: a check, then the call (best effort).
    #[cfg(not(unix))]
    fn check(root: &Path, parts: &[&std::ffi::OsStr]) -> io::Result<()> {
        let mut path = root.to_path_buf();
        for part in parts {
            path.push(part);
            if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a link is in the way",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn writes_beneath_a_folder_never_go_through_a_linked_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));
        let project = tmp.path().join("project");
        let elsewhere = tmp.path().join("elsewhere");
        std::fs::create_dir_all(project.join(".claude/skills")).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("SKILL.md"), "theirs").unwrap();
        // The skill's folder is a link to another folder.
        std::os::unix::fs::symlink(&elsewhere, project.join(".claude/skills/pr")).unwrap();
        let rel = Path::new(".claude/skills/pr/SKILL.md");
        assert!(fs_.write_text_beneath(&project, rel, "hedy").is_err());
        assert!(fs_.remove_file_beneath(&project, rel).is_err());
        assert_eq!(
            std::fs::read_to_string(elsewhere.join("SKILL.md")).unwrap(),
            "theirs"
        );
        // Folders that are missing are made; a plain file is written and removed.
        let fine = Path::new(".agents/skills/pr/SKILL.md");
        fs_.write_text_beneath(&project, fine, "one").unwrap();
        fs_.write_text_beneath(&project, fine, "two").unwrap();
        assert_eq!(std::fs::read_to_string(project.join(fine)).unwrap(), "two");
        fs_.remove_file_beneath(&project, fine).unwrap();
        assert!(!project.join(fine).exists());
        fs_.remove_file_beneath(&project, fine).unwrap();
        assert!(
            fs_.write_text_beneath(&project, Path::new("../escape"), "x")
                .is_err()
        );
        // Empty folders go; one with anything in it, or a link, stays.
        fs_.remove_empty_dir_beneath(&project, Path::new(".agents/skills/pr"))
            .unwrap();
        assert!(!project.join(".agents/skills/pr").exists());
        fs_.remove_empty_dir_beneath(&project, Path::new(".claude"))
            .unwrap();
        assert!(project.join(".claude/skills").exists(), "not empty");
        assert!(
            fs_.remove_empty_dir_beneath(&project, Path::new(".claude/skills/pr"))
                .is_err()
        );
        assert!(elsewhere.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_is_never_written_through_a_link_in_its_place() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));
        let target = tmp.path().join("private.txt");
        std::fs::write(&target, "mine").unwrap();
        let link = tmp.path().join("skills/x/SKILL.md");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(fs_.write_text(&link, "hedy").is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "mine");
        let plain = tmp.path().join("skills/y/SKILL.md");
        fs_.write_text(&plain, "one").unwrap();
        fs_.write_text(&plain, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&plain).unwrap(), "two");
    }

    #[test]
    fn outputs_are_listed_without_links_and_copied_without_overwriting() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));
        let dir = fs_.outputs_dir().join("t1");
        assert_eq!(dir, tmp.path().join("outputs/t1"));
        assert!(
            fs_.files_beneath(&dir).unwrap().is_empty(),
            "missing: empty"
        );
        fs::create_dir_all(dir.join("sitio/css")).unwrap();
        fs::write(dir.join("informe.md"), "# Hola\n").unwrap();
        fs::write(dir.join("sitio/css/a.css"), "a{}").unwrap();
        let secret = tmp.path().join("secreto.txt");
        fs::write(&secret, "no").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, dir.join("enlace.txt")).unwrap();
            std::os::unix::fs::symlink(tmp.path(), dir.join("todo")).unwrap();
        }
        assert_eq!(
            fs_.files_beneath(&dir).unwrap(),
            vec![("informe.md".into(), 7), ("sitio/css/a.css".into(), 3)]
        );

        let saved = tmp.path().join("informe.md");
        fs_.copy_to_new(&dir.join("informe.md"), &saved).unwrap();
        assert_eq!(fs::read_to_string(&saved).unwrap(), "# Hola\n");
        assert!(fs_.copy_to_new(&dir.join("informe.md"), &saved).is_err());
        #[cfg(unix)]
        assert!(
            fs_.copy_to_new(&dir.join("enlace.txt"), &tmp.path().join("x.txt"))
                .is_err(),
            "a link is not copied"
        );
    }

    #[test]
    fn attachments_are_private_and_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));
        assert_eq!(fs_.attachments_dir(), tmp.path().join("attachments"));
        let file = fs_.attachments_dir().join("t1/a-0.png");
        fs_.write_private_file(&file, b"img").unwrap();
        assert_eq!(fs_.read_bytes(&file).unwrap(), Some(b"img".to_vec()));
        assert!(fs_.write_private_file(&file, b"other").is_err());
        assert_eq!(fs_.read_bytes(&file.with_file_name("x.png")).unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&file), 0o600);
            assert_eq!(mode(file.parent().unwrap()), 0o700);
        }
    }

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
    fn someone_elses_files_are_read_only_when_regular_and_small_and_never_written_through() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));
        let file = tmp.path().join("SKILL.md");
        fs::write(&file, "hola").unwrap();
        assert_eq!(
            fs_.read_regular_text(&file, 4).unwrap().as_deref(),
            Some("hola")
        );
        assert!(fs_.read_regular_text(&file, 3).is_err(), "too big");
        assert_eq!(
            fs_.read_regular_text(&tmp.path().join("missing"), 9)
                .unwrap(),
            None
        );
        let link = tmp.path().join("zero.md");
        std::os::unix::fs::symlink("/dev/zero", &link).unwrap();
        assert!(fs_.read_regular_text(&link, 9).is_err());
        assert!(fs_.read_regular_text(Path::new("/dev/zero"), 9).is_err());
        fs::write(tmp.path().join("bin"), [0xff, 0xfe]).unwrap();
        assert!(fs_.read_regular_text(&tmp.path().join("bin"), 9).is_err());

        let out = tmp.path().join("out/lint/SKILL.md");
        assert!(fs_.create_text(&out, "uno").unwrap());
        assert!(!fs_.create_text(&out, "dos").unwrap());
        assert_eq!(fs::read_to_string(&out).unwrap(), "uno");
        let target = tmp.path().join("elsewhere");
        let dangling = tmp.path().join("out/x.md");
        std::os::unix::fs::symlink(&target, &dangling).unwrap();
        assert!(!fs_.create_text(&dangling, "tres").unwrap());
        assert!(!target.exists(), "never through a link");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_seen_and_never_removed_as_empty_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let fs_ = LocalFileSystem::new(tmp.path().join("accounts"), tmp.path().join("chats"));
        let target = tmp.path().join("shared");
        fs::create_dir(&target).unwrap();
        let link = tmp.path().join("skills");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(fs_.is_symlink(&link));
        assert!(!fs_.is_symlink(&target));
        assert!(!fs_.is_symlink(&tmp.path().join("missing")));

        fs_.remove_empty_dir(&link).unwrap();
        assert!(fs_.is_symlink(&link), "a link to an empty folder stays");

        let mut entries = fs_.entries(tmp.path()).unwrap();
        entries.sort();
        assert_eq!(entries, vec![target, link]);
        assert!(fs_.entries(&tmp.path().join("missing")).unwrap().is_empty());
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
