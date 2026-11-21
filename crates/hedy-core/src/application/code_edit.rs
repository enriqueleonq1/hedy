//! Light editing of a copy's files: save a file the user changed in the
//! viewer, create, rename, duplicate and delete files and folders.
//!
//! Only live copies (the user's folder or an open task's worktree) are
//! written, never an accepted task's snapshot, and never while the task's
//! agent works on that copy. Protected files (`.env`, keys...) and `.git`
//! are never touched, nor folders holding them. A save carries the version
//! the editor loaded: when the file changed on disk meanwhile, nothing is
//! written and the current contents come back as a conflict.
//!
//! Writing in a task's worktree makes the edit part of the task's changes:
//! its review and diff read the worktree, so they show it on the next read.

use super::code::{CodeCopy, Resolved, checked_path, hidden};
use super::ports::PathKind;
use super::{AppError, AppResult, Hedy};
use crate::domain::attachment::ImageType;
use crate::domain::code::{
    MAX_TEXT_BYTES, MAX_TREE_ENTRIES, MISSING_VERSION, content_version, is_valid_file_name,
    looks_binary, text_for_disk,
};
use crate::tr;

/// A file written: its new version and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub version: String,
    pub size: u64,
}

/// What a save did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    Saved(Saved),
    /// The file changed on disk since the editor loaded it; nothing was
    /// written. `text: None` with [`MISSING_VERSION`]: it was deleted. Saving
    /// again with `version` overwrites it ("Sobrescribir").
    Conflict {
        text: Option<String>,
        version: String,
    },
}

/// The last part of a path.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `a/b/c.py` → (`a/b/c`, `.py`); a leading dot is part of the name.
fn split_extension(path: &str) -> (&str, &str) {
    let name = file_name(path);
    match name.rfind('.') {
        Some(dot) if dot > 0 => path.split_at(path.len() - (name.len() - dot)),
        _ => (path, ""),
    }
}

impl Hedy {
    /// A copy whose files can be written now.
    fn writable_copy(&self, copy: &CodeCopy) -> AppResult<Resolved> {
        let copy = self.resolve_copy(copy)?;
        if copy.at.is_some() || !copy.info.live {
            return Err(AppError::CopyReadOnly);
        }
        if copy.info.agent_working
            && let Some(task) = &copy.info.task
        {
            return Err(AppError::CopyBusy(task.clone()));
        }
        Ok(copy)
    }

    /// A path Hedy may write: inside the copy, not `.git`, `.hedy` or a
    /// protected file.
    fn writable_path(copy: &Resolved, path: &str) -> AppResult<String> {
        let path = checked_path(path)?;
        if hidden(&path) {
            return Err(AppError::InvalidCodePath(path));
        }
        if copy.blocked.blocks_path(&path) {
            return Err(AppError::FileLocked(path));
        }
        Ok(path)
    }

    /// A path for something new: a valid name, and nothing there yet.
    fn new_path(&self, copy: &Resolved, path: &str) -> AppResult<String> {
        let path = Self::writable_path(copy, path)?;
        if !is_valid_file_name(file_name(&path)) {
            return Err(AppError::InvalidFileName(file_name(&path).to_owned()));
        }
        if self.writer.kind(&copy.dir, &path)?.is_some() {
            return Err(AppError::PathExists(path));
        }
        Ok(path)
    }

    /// What is at `path`, refusing a folder that holds protected files, a
    /// nested repository (`.git`) or too many entries to check.
    fn movable(&self, copy: &Resolved, path: &str) -> AppResult<PathKind> {
        let kind = self
            .writer
            .kind(&copy.dir, path)?
            .ok_or_else(|| AppError::PathMissing(path.to_owned()))?;
        if kind == PathKind::Folder {
            let (inside, more) = self.writer.walk(&copy.dir, path, MAX_TREE_ENTRIES)?;
            if more {
                return Err(AppError::Port(super::ports::PortError::new(tr!(
                    "{path} tiene demasiados archivos para moverlo desde Hedy",
                    "{path} has too many files to move from Hedy"
                ))));
            }
            for entry in inside {
                if entry
                    .split('/')
                    .any(|part| part.eq_ignore_ascii_case(".git"))
                {
                    return Err(AppError::InvalidCodePath(entry));
                }
                if copy.blocked.blocks_path(&entry) {
                    return Err(AppError::FileLocked(entry));
                }
            }
        }
        Ok(kind)
    }

    /// Saves `text` over the copy's file when it is still the `expected`
    /// version the editor loaded (or, with [`MISSING_VERSION`], writes it
    /// again after it was deleted). The file's line endings, byte-order
    /// mark, final newline and permissions are kept.
    pub fn save_code_file(
        &self,
        copy: &CodeCopy,
        path: &str,
        text: &str,
        expected: &str,
    ) -> AppResult<WriteOutcome> {
        let copy = self.writable_copy(copy)?;
        let path = Self::writable_path(&copy, path)?;
        let current = self
            .code
            .read_file(&copy.dir, None, &path, MAX_TEXT_BYTES + 1)?;
        let Some(file) = current else {
            if expected != MISSING_VERSION {
                return Ok(WriteOutcome::Conflict {
                    text: None,
                    version: MISSING_VERSION.to_owned(),
                });
            }
            let bytes = text_for_disk(None, text);
            self.writer.create_file(&copy.dir, &path, &bytes)?;
            return Ok(WriteOutcome::Saved(Saved {
                version: content_version(&bytes),
                size: bytes.len() as u64,
            }));
        };
        // Through a symbolic link, the target decides.
        if copy.blocked.blocks_path(&file.resolved) {
            return Err(AppError::FileLocked(path));
        }
        let not_text = file.size > MAX_TEXT_BYTES
            || ImageType::sniff(&file.bytes).is_some()
            || looks_binary(&file.bytes);
        let Some(current_text) = (!not_text)
            .then(|| std::str::from_utf8(&file.bytes).ok())
            .flatten()
        else {
            return Err(AppError::FileNotEditable(path));
        };
        let version = content_version(&file.bytes);
        if version != expected {
            return Ok(WriteOutcome::Conflict {
                text: Some(current_text.to_owned()),
                version,
            });
        }
        let bytes = text_for_disk(Some(&file.bytes), text);
        if bytes.len() as u64 > MAX_TEXT_BYTES {
            return Err(AppError::FileNotEditable(path));
        }
        if bytes != file.bytes {
            self.writer
                .replace_file(&copy.dir, &file.resolved, &bytes)?;
        }
        Ok(WriteOutcome::Saved(Saved {
            version: content_version(&bytes),
            size: bytes.len() as u64,
        }))
    }

    /// Creates an empty file (and the folders above it).
    pub fn create_code_file(&self, copy: &CodeCopy, path: &str) -> AppResult<Saved> {
        let copy = self.writable_copy(copy)?;
        let path = self.new_path(&copy, path)?;
        self.writer.create_file(&copy.dir, &path, b"")?;
        Ok(Saved {
            version: content_version(b""),
            size: 0,
        })
    }

    /// Creates a folder (git lists no empty folders: the UI keeps showing
    /// the ones the user created until a file lands in them).
    pub fn create_code_folder(&self, copy: &CodeCopy, path: &str) -> AppResult<()> {
        let copy = self.writable_copy(copy)?;
        let path = self.new_path(&copy, path)?;
        Ok(self.writer.create_folder(&copy.dir, &path)?)
    }

    /// Renames (or moves) a file or folder inside the copy.
    pub fn rename_code_path(&self, copy: &CodeCopy, from: &str, to: &str) -> AppResult<()> {
        let copy = self.writable_copy(copy)?;
        let from = Self::writable_path(&copy, from)?;
        let to = Self::writable_path(&copy, to)?;
        if from == to {
            return Ok(());
        }
        if !is_valid_file_name(file_name(&to)) {
            return Err(AppError::InvalidFileName(file_name(&to).to_owned()));
        }
        self.movable(&copy, &from)?;
        // A change of case only is the same file on some disks.
        if !from.eq_ignore_ascii_case(&to) && self.writer.kind(&copy.dir, &to)?.is_some() {
            return Err(AppError::PathExists(to));
        }
        Ok(self.writer.rename(&copy.dir, &from, &to)?)
    }

    /// Moves a file or folder to the system's trash (the user can restore
    /// it from there).
    pub fn delete_code_path(&self, copy: &CodeCopy, path: &str) -> AppResult<()> {
        let copy = self.writable_copy(copy)?;
        let path = Self::writable_path(&copy, path)?;
        self.movable(&copy, &path)?;
        Ok(self.writer.trash(&copy.dir, &path)?)
    }

    /// Copies a file next to itself (`impuestos copia.py`, then
    /// `impuestos copia 2.py`...); returns the new path.
    pub fn duplicate_code_file(&self, copy: &CodeCopy, path: &str) -> AppResult<String> {
        let copy = self.writable_copy(copy)?;
        let path = Self::writable_path(&copy, path)?;
        match self.writer.kind(&copy.dir, &path)? {
            Some(PathKind::File) => {}
            Some(_) => return Err(AppError::FileNotEditable(path)),
            None => return Err(AppError::PathMissing(path)),
        }
        let (stem, ext) = split_extension(&path);
        let word = tr!("copia", "copy");
        for n in 1..=100 {
            let candidate = if n == 1 {
                format!("{stem} {word}{ext}")
            } else {
                format!("{stem} {word} {n}{ext}")
            };
            if self.writer.kind(&copy.dir, &candidate)?.is_none() {
                self.writer.copy_file(&copy.dir, &path, &candidate)?;
                return Ok(candidate);
            }
        }
        Err(AppError::PathExists(format!("{stem} {word}{ext}")))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::application::NewTask;
    use crate::application::testing::Fixture;
    use crate::domain::{AgentEvent, Task, TaskMode};

    const ROOT: &str = "/code/agenda";

    /// A project that keeps the usual files (`.env`, keys) from the AI.
    fn project(f: &Fixture) {
        f.add_claude("Personal");
        f.fs.add_git_root(ROOT);
        f.hedy.use_account(&f.dir(ROOT), "personal", None).unwrap();
        let usual: Vec<String> = crate::domain::permissions::SUGGESTED_BLOCKED
            .iter()
            .map(|p| p.to_string())
            .collect();
        f.hedy
            .set_project_blocked_files(&f.dir(ROOT), &usual)
            .unwrap();
    }

    fn folder() -> CodeCopy {
        CodeCopy::Folder(PathBuf::from(ROOT))
    }

    fn task(f: &Fixture) -> (Task, String) {
        project(f);
        let task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir(ROOT),
                prompt: "Arregla los impuestos".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
                permissions: None,
            })
            .unwrap();
        let wt = task.workspace.clone().unwrap().worktree;
        let wt = wt.to_string_lossy().into_owned();
        f.fs.add_existing(&wt);
        (task, wt)
    }

    fn finish(f: &Fixture) {
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: None,
        });
    }

    fn saved(outcome: WriteOutcome) -> Saved {
        match outcome {
            WriteOutcome::Saved(saved) => saved,
            other => panic!("not saved: {other:?}"),
        }
    }

    #[test]
    fn saves_against_the_loaded_version_and_keeps_crlf() {
        let f = Fixture::new();
        project(&f);
        let original = b"IVA = 0.19\r\nTOTAL = 1\r\n";
        f.code.add_file(ROOT, None, "impuestos.py", original);
        let loaded = content_version(original);
        let done = saved(
            f.hedy
                .save_code_file(
                    &folder(),
                    "impuestos.py",
                    "IVA = 0.21\nTOTAL = 1\n",
                    &loaded,
                )
                .unwrap(),
        );
        let disk = f.code.working(ROOT, "impuestos.py").unwrap();
        assert_eq!(disk, b"IVA = 0.21\r\nTOTAL = 1\r\n");
        assert_eq!(done.version, content_version(&disk));
        assert_eq!(done.size, disk.len() as u64);

        // Saving again with the old version: someone changed it meanwhile.
        let conflict = f
            .hedy
            .save_code_file(&folder(), "impuestos.py", "IVA = 0\n", &loaded)
            .unwrap();
        assert_eq!(
            conflict,
            WriteOutcome::Conflict {
                text: Some("IVA = 0.21\r\nTOTAL = 1\r\n".into()),
                version: done.version.clone(),
            }
        );
        assert_eq!(f.code.working(ROOT, "impuestos.py").unwrap(), disk);
        // "Sobrescribir": the conflict's version.
        saved(
            f.hedy
                .save_code_file(&folder(), "impuestos.py", "IVA = 0\n", &done.version)
                .unwrap(),
        );
        assert_eq!(
            f.code.working(ROOT, "impuestos.py").unwrap(),
            b"IVA = 0\r\n"
        );
    }

    #[test]
    fn a_deleted_file_is_a_conflict_until_written_again() {
        let f = Fixture::new();
        project(&f);
        let outcome = f
            .hedy
            .save_code_file(&folder(), "nada.py", "x\n", &content_version(b"y\n"))
            .unwrap();
        assert_eq!(
            outcome,
            WriteOutcome::Conflict {
                text: None,
                version: MISSING_VERSION.into()
            }
        );
        saved(
            f.hedy
                .save_code_file(&folder(), "nada.py", "x\n", MISSING_VERSION)
                .unwrap(),
        );
        assert_eq!(f.code.working(ROOT, "nada.py").unwrap(), b"x\n");
    }

    #[test]
    fn never_writes_protected_binary_or_outside_files() {
        let f = Fixture::new();
        project(&f);
        f.code.add_file(ROOT, None, ".env", b"TOKEN=x\n");
        f.code.add_file(ROOT, None, "real.env.local", b"x");
        f.code.add_file(ROOT, None, ".env.real", b"TOKEN=x\n");
        f.code.add_link(ROOT, "config.txt", ".env.real");
        f.code.add_file(ROOT, None, "data.bin", b"\0\x01");
        f.code.add_file(ROOT, None, "latin1.txt", b"ca\xf1a\n");
        let save = |path: &str, version: &[u8]| {
            f.hedy
                .save_code_file(&folder(), path, "x", &content_version(version))
        };
        assert_eq!(
            save(".env", b"TOKEN=x\n"),
            Err(AppError::FileLocked(".env".into()))
        );
        assert_eq!(
            save("config.txt", b"TOKEN=x\n"),
            Err(AppError::FileLocked("config.txt".into()))
        );
        assert_eq!(
            save("data.bin", b"\0\x01"),
            Err(AppError::FileNotEditable("data.bin".into()))
        );
        assert_eq!(
            save("latin1.txt", b"ca\xf1a\n"),
            Err(AppError::FileNotEditable("latin1.txt".into()))
        );
        assert!(matches!(
            save("../x.py", b""),
            Err(AppError::InvalidCodePath(_))
        ));
        assert!(matches!(
            save(".git/config", b""),
            Err(AppError::InvalidCodePath(_))
        ));
        assert!(matches!(
            save(".hedy/worktrees/t1/a.py", b""),
            Err(AppError::InvalidCodePath(_))
        ));
        assert!(
            !f.code.calls().iter().any(|c| c.starts_with("write")),
            "{:?}",
            f.code.calls()
        );
    }

    #[test]
    fn a_task_copy_is_locked_while_its_agent_works_and_a_snapshot_always() {
        let f = Fixture::new();
        let (task, wt) = task(&f);
        f.code.add_file(&wt, None, "a.py", b"a\n");
        let copy = CodeCopy::Task(task.id.clone());
        let version = content_version(b"a\n");
        assert_eq!(
            f.hedy.save_code_file(&copy, "a.py", "b\n", &version),
            Err(AppError::CopyBusy(task.id.clone()))
        );
        assert_eq!(
            f.hedy.create_code_file(&copy, "b.py"),
            Err(AppError::CopyBusy(task.id.clone()))
        );
        finish(&f);
        saved(
            f.hedy
                .save_code_file(&copy, "a.py", "b\n", &version)
                .unwrap(),
        );
        assert_eq!(f.code.working(&wt, "a.py").unwrap(), b"b\n");

        f.hedy.accept_task(&task.id).unwrap();
        assert_eq!(
            f.hedy.save_code_file(&copy, "a.py", "c\n", &version),
            Err(AppError::CopyReadOnly)
        );
        assert_eq!(
            f.hedy.delete_code_path(&copy, "a.py"),
            Err(AppError::CopyReadOnly)
        );
    }

    #[test]
    fn creates_renames_duplicates_and_trashes() {
        let f = Fixture::new();
        project(&f);
        f.code.add_file(ROOT, None, "src/app.py", b"print(1)\n");
        f.code.add_file(ROOT, None, "src/.env", b"x");

        f.hedy.create_code_file(&folder(), "src/nuevo.py").unwrap();
        assert_eq!(f.code.working(ROOT, "src/nuevo.py").unwrap(), b"");
        assert_eq!(
            f.hedy.create_code_file(&folder(), "src/app.py"),
            Err(AppError::PathExists("src/app.py".into()))
        );
        assert_eq!(
            f.hedy.create_code_file(&folder(), "src/a?.py"),
            Err(AppError::InvalidFileName("a?.py".into()))
        );
        assert_eq!(
            f.hedy.create_code_file(&folder(), "key.pem"),
            Err(AppError::FileLocked("key.pem".into()))
        );
        f.hedy.create_code_folder(&folder(), "docs").unwrap();
        assert_eq!(
            f.hedy.create_code_folder(&folder(), "src"),
            Err(AppError::PathExists("src".into()))
        );

        f.hedy
            .rename_code_path(&folder(), "src/nuevo.py", "src/otro.py")
            .unwrap();
        assert!(f.code.working(ROOT, "src/otro.py").is_some());
        assert!(f.code.working(ROOT, "src/nuevo.py").is_none());
        assert_eq!(
            f.hedy
                .rename_code_path(&folder(), "src/otro.py", "src/app.py"),
            Err(AppError::PathExists("src/app.py".into()))
        );
        assert_eq!(
            f.hedy.rename_code_path(&folder(), "src/otro.py", ".env"),
            Err(AppError::FileLocked(".env".into()))
        );
        assert_eq!(
            f.hedy.rename_code_path(&folder(), "nada.py", "x.py"),
            Err(AppError::PathMissing("nada.py".into()))
        );
        // A folder holding a protected file stays where it is.
        assert_eq!(
            f.hedy.rename_code_path(&folder(), "src", "lib"),
            Err(AppError::FileLocked("src/.env".into()))
        );
        assert_eq!(
            f.hedy.delete_code_path(&folder(), "src"),
            Err(AppError::FileLocked("src/.env".into()))
        );

        let copy_path = f.hedy.duplicate_code_file(&folder(), "src/app.py").unwrap();
        assert_eq!(copy_path, "src/app copia.py");
        assert_eq!(
            f.hedy.duplicate_code_file(&folder(), "src/app.py").unwrap(),
            "src/app copia 2.py"
        );
        assert_eq!(f.code.working(ROOT, &copy_path).unwrap(), b"print(1)\n");

        f.hedy.delete_code_path(&folder(), "src/otro.py").unwrap();
        assert!(f.code.working(ROOT, "src/otro.py").is_none());
        assert_eq!(f.code.trashed(), [format!("{ROOT}/src/otro.py")]);
        assert_eq!(
            f.hedy.delete_code_path(&folder(), "src/.env"),
            Err(AppError::FileLocked("src/.env".into()))
        );
    }

    #[test]
    fn extensions_split_like_file_managers() {
        assert_eq!(split_extension("a/b/c.py"), ("a/b/c", ".py"));
        assert_eq!(split_extension("a/.gitignore"), ("a/.gitignore", ""));
        assert_eq!(split_extension("Makefile"), ("Makefile", ""));
        assert_eq!(split_extension("x.tar.gz"), ("x.tar", ".gz"));
    }
}
