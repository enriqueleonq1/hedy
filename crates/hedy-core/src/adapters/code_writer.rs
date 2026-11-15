//! Light editing of a copy's working files ([`CodeWriter`]): save a file in
//! place (keeping its permissions), create files and folders, rename, copy,
//! and move to the system's trash. Every path is joined to the copy's folder
//! and its parent folder must resolve inside it, so a symbolic link to a
//! folder elsewhere cannot be used to write outside the copy.

use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use crate::application::ports::{CodeWriter, PathKind, PortError, PortResult};
use crate::tr;

/// Moves a path to the trash (the system's, unless a test swaps it).
type TrashFn = fn(&Path) -> Result<(), String>;

fn system_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

/// Writes a copy's working files on the local disk.
pub struct LocalCodeWriter {
    trash: TrashFn,
}

impl Default for LocalCodeWriter {
    fn default() -> Self {
        Self {
            trash: system_trash,
        }
    }
}

impl LocalCodeWriter {
    /// With another trash (tests: never the user's own).
    #[cfg(test)]
    fn with_trash(trash: TrashFn) -> Self {
        Self { trash }
    }
}

fn io_error(path: &str, err: std::io::Error) -> PortError {
    PortError::new(format!("{path}: {err}"))
}

fn exists_error(path: &str) -> PortError {
    PortError::new(tr!("«{path}» ya existe", "«{path}» already exists"))
}

/// `dir/path`, once its parent folder is known to stay inside `dir` (after
/// following links). Missing parents are fine: they are created inside.
fn inside(dir: &Path, path: &str) -> PortResult<PathBuf> {
    let root = dir.canonicalize().map_err(|e| io_error(".", e))?;
    let full = dir.join(path);
    // The deepest parent that exists decides where the path really goes.
    let mut parent = full.parent();
    while let Some(p) = parent {
        if p.exists() {
            let real = p.canonicalize().map_err(|e| io_error(path, e))?;
            if !real.starts_with(&root) {
                return Err(PortError::new(tr!(
                    "{path} apunta fuera de la carpeta del proyecto",
                    "{path} points outside the project folder"
                )));
            }
            break;
        }
        parent = p.parent();
    }
    Ok(full)
}

fn create_parents(full: &Path, path: &str) -> PortResult<()> {
    match full.parent() {
        Some(parent) => fs::create_dir_all(parent).map_err(|e| io_error(path, e)),
        None => Ok(()),
    }
}

/// A sibling name for the temporary file a save writes first.
fn temp_sibling(full: &Path) -> PathBuf {
    let name = full
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    full.with_file_name(format!(".{name}.hedy-{}.tmp", std::process::id()))
}

impl CodeWriter for LocalCodeWriter {
    fn kind(&self, dir: &Path, path: &str) -> PortResult<Option<PathKind>> {
        let full = inside(dir, path)?;
        Ok(match fs::symlink_metadata(&full) {
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => return Err(io_error(path, e)),
            Ok(meta) if meta.file_type().is_symlink() => Some(PathKind::Link),
            Ok(meta) if meta.is_dir() => Some(PathKind::Folder),
            Ok(_) => Some(PathKind::File),
        })
    }

    fn replace_file(&self, dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()> {
        let full = inside(dir, path)?;
        let meta = fs::symlink_metadata(&full).map_err(|e| io_error(path, e))?;
        if !meta.is_file() {
            return Err(PortError::new(tr!(
                "{path} no es un archivo normal",
                "{path} is not a regular file"
            )));
        }
        let temp = temp_sibling(&full);
        let written = (|| {
            let mut file = fs::File::create(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::set_permissions(&temp, meta.permissions())?;
            fs::rename(&temp, &full)
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&temp);
            return Err(io_error(path, e));
        }
        Ok(())
    }

    fn create_file(&self, dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()> {
        let full = inside(dir, path)?;
        create_parents(&full, path)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&full)
            .map_err(|e| match e.kind() {
                ErrorKind::AlreadyExists => exists_error(path),
                _ => io_error(path, e),
            })?;
        file.write_all(bytes).map_err(|e| io_error(path, e))
    }

    fn create_folder(&self, dir: &Path, path: &str) -> PortResult<()> {
        let full = inside(dir, path)?;
        if fs::symlink_metadata(&full).is_ok() {
            return Err(exists_error(path));
        }
        fs::create_dir_all(&full).map_err(|e| io_error(path, e))
    }

    fn rename(&self, dir: &Path, from: &str, to: &str) -> PortResult<()> {
        let source = inside(dir, from)?;
        let target = inside(dir, to)?;
        fs::symlink_metadata(&source).map_err(|e| io_error(from, e))?;
        if fs::symlink_metadata(&target).is_ok() {
            // A change of case on a case-insensitive disk is the same file.
            let same = source.canonicalize().ok() == target.canonicalize().ok();
            if !same {
                return Err(exists_error(to));
            }
        }
        create_parents(&target, to)?;
        fs::rename(&source, &target).map_err(|e| io_error(from, e))
    }

    fn copy_file(&self, dir: &Path, from: &str, to: &str) -> PortResult<()> {
        let source = inside(dir, from)?;
        let target = inside(dir, to)?;
        if !fs::symlink_metadata(&source).is_ok_and(|m| m.is_file()) {
            return Err(PortError::new(tr!(
                "{from} no es un archivo normal",
                "{from} is not a regular file"
            )));
        }
        create_parents(&target, to)?;
        // `create_new` first, so an existing file is never replaced.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| match e.kind() {
                ErrorKind::AlreadyExists => exists_error(to),
                _ => io_error(to, e),
            })?;
        fs::copy(&source, &target)
            .map(drop)
            .map_err(|e| io_error(to, e))
    }

    fn walk(&self, dir: &Path, folder: &str, max: usize) -> PortResult<(Vec<String>, bool)> {
        let start = inside(dir, folder)?;
        let mut out = Vec::new();
        let mut stack = vec![(start, folder.to_owned())];
        while let Some((full, relative)) = stack.pop() {
            let entries = fs::read_dir(&full).map_err(|e| io_error(&relative, e))?;
            for entry in entries {
                let entry = entry.map_err(|e| io_error(&relative, e))?;
                if out.len() >= max {
                    return Ok((out, true));
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = format!("{relative}/{name}");
                let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
                if is_dir {
                    stack.push((entry.path(), path.clone()));
                }
                out.push(path);
            }
        }
        Ok((out, false))
    }

    fn trash(&self, dir: &Path, path: &str) -> PortResult<()> {
        let full = inside(dir, path)?;
        fs::symlink_metadata(&full).map_err(|e| io_error(path, e))?;
        (self.trash)(&full).map_err(|e| {
            PortError::new(tr!(
                "no se pudo mover {path} a la papelera: {e}",
                "could not move {path} to the trash: {e}"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("copy");
        fs::create_dir_all(&dir).unwrap();
        (tmp, dir)
    }

    /// A trash for tests: a `.trash` folder next to the copy.
    fn test_trash(path: &Path) -> Result<(), String> {
        let bin = path
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == "copy"))
            .and_then(Path::parent)
            .unwrap()
            .join(".trash");
        fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        fs::rename(path, bin.join(path.file_name().unwrap())).map_err(|e| e.to_string())
    }

    #[test]
    fn replaces_in_place_keeping_permissions() {
        let (_tmp, dir) = tmp();
        let writer = LocalCodeWriter::default();
        fs::write(dir.join("run.sh"), "echo 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir.join("run.sh"), fs::Permissions::from_mode(0o750)).unwrap();
        }
        writer.replace_file(&dir, "run.sh", b"echo 2\n").unwrap();
        assert_eq!(fs::read(dir.join("run.sh")).unwrap(), b"echo 2\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join("run.sh"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o750);
        }
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            1,
            "no temporary file left"
        );
        assert!(writer.replace_file(&dir, "nada.txt", b"x").is_err());
    }

    #[test]
    fn creates_without_overwriting() {
        let (_tmp, dir) = tmp();
        let writer = LocalCodeWriter::default();
        writer.create_file(&dir, "src/nuevo.py", b"").unwrap();
        assert!(dir.join("src/nuevo.py").is_file());
        assert!(writer.create_file(&dir, "src/nuevo.py", b"x").is_err());
        assert_eq!(fs::read(dir.join("src/nuevo.py")).unwrap(), b"");
        writer.create_folder(&dir, "docs/api").unwrap();
        assert_eq!(
            writer.kind(&dir, "docs/api").unwrap(),
            Some(PathKind::Folder)
        );
        assert!(writer.create_folder(&dir, "docs/api").is_err());
        assert_eq!(
            writer.kind(&dir, "src/nuevo.py").unwrap(),
            Some(PathKind::File)
        );
        assert_eq!(writer.kind(&dir, "no.txt").unwrap(), None);
    }

    #[test]
    fn renames_copies_and_walks() {
        let (_tmp, dir) = tmp();
        let writer = LocalCodeWriter::default();
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/x.py"), "x").unwrap();
        fs::write(dir.join("a/y.py"), "y").unwrap();
        writer.rename(&dir, "a/y.py", "a/z.py").unwrap();
        assert!(dir.join("a/z.py").is_file());
        assert!(writer.rename(&dir, "a/z.py", "a/b/x.py").is_err());
        writer.copy_file(&dir, "a/z.py", "a/z copia.py").unwrap();
        assert_eq!(fs::read(dir.join("a/z copia.py")).unwrap(), b"y");
        assert!(writer.copy_file(&dir, "a/z.py", "a/b/x.py").is_err());
        assert!(
            writer.copy_file(&dir, "a", "a2").is_err(),
            "folders are not copied"
        );
        let (mut all, more) = writer.walk(&dir, "a", 100).unwrap();
        all.sort();
        assert_eq!(all, ["a/b", "a/b/x.py", "a/z copia.py", "a/z.py"]);
        assert!(!more);
        assert!(writer.walk(&dir, "a", 2).unwrap().1);
        writer.rename(&dir, "a", "c/d").unwrap();
        assert!(dir.join("c/d/b/x.py").is_file());
    }

    #[test]
    fn deleting_moves_to_the_trash() {
        let (tmp, dir) = tmp();
        let writer = LocalCodeWriter::with_trash(test_trash);
        fs::write(dir.join("viejo.py"), "x").unwrap();
        writer.trash(&dir, "viejo.py").unwrap();
        assert!(!dir.join("viejo.py").exists());
        assert!(tmp.path().join(".trash/viejo.py").is_file());
        assert!(writer.trash(&dir, "viejo.py").is_err());
    }

    /// A save in a repository: git (and so the task's review) sees exactly
    /// the edited line, with the file's CRLF endings kept.
    #[test]
    fn a_save_in_a_repository_changes_only_the_edited_line() {
        use crate::adapters::GitCli;
        use crate::application::ports::CodeReader;
        use crate::domain::code::{ChangeKind, content_version, text_for_disk};

        let (_tmp, dir) = tmp();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "test@hedy.local"]);
        git(&["config", "user.name", "Ana"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.autocrlf", "false"]);
        fs::write(dir.join("win.txt"), "uno\r\ndos\r\ntres\r\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "Inicio"]);
        let base = git(&["rev-parse", "HEAD"]).trim().to_owned();

        let read = GitCli
            .read_file(&dir, None, "win.txt", 1 << 20)
            .unwrap()
            .unwrap();
        let loaded = content_version(&read.bytes);
        // The editor holds "\n" lines.
        let bytes = text_for_disk(Some(&read.bytes), "uno\nDOS\ntres\n");
        LocalCodeWriter::default()
            .replace_file(&dir, "win.txt", &bytes)
            .unwrap();
        let now = GitCli
            .read_file(&dir, None, "win.txt", 1 << 20)
            .unwrap()
            .unwrap();
        assert_ne!(content_version(&now.bytes), loaded, "the version moved on");
        assert_eq!(now.bytes, b"uno\r\nDOS\r\ntres\r\n");
        let changes = GitCli.changes(&dir, &base, None).unwrap();
        assert_eq!(changes[0].kind, ChangeKind::Modified);
        let diff = GitCli
            .diff_file(&dir, &base, None, "win.txt", None, 0)
            .unwrap()
            .unwrap();
        assert_eq!((diff.additions, diff.deletions), (1, 1));
    }

    #[cfg(unix)]
    #[test]
    fn never_writes_through_a_link_to_a_folder_outside() {
        let (tmp, dir) = tmp();
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("fuera")).unwrap();
        let writer = LocalCodeWriter::default();
        assert!(writer.create_file(&dir, "fuera/x.txt", b"x").is_err());
        assert!(writer.create_folder(&dir, "fuera/sub/deep").is_err());
        assert!(!outside.join("x.txt").exists());
        // The link itself can be renamed or deleted (it stays inside).
        assert_eq!(writer.kind(&dir, "fuera").unwrap(), Some(PathKind::Link));
    }
}
