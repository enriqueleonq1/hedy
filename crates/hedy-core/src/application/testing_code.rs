//! In-memory fake of [`CodeReader`] and [`CodeWriter`], for the code
//! navigation and editing use cases.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::ports::{CodeReader, CodeWriter, PathKind, PortError, PortResult};
use crate::domain::FileDiff;
use crate::domain::code::{
    Blame, CommitInfo, FileBytes, LineMatch, MatchRange, PathChange, TextQuery,
};

/// Files per (folder, revision) — `None` = the working files — plus canned
/// answers for status, diffs, history and blame. Records what it was asked.
/// Path → contents.
type Files = BTreeMap<String, Vec<u8>>;

#[derive(Default)]
pub struct FakeCode {
    files: Mutex<HashMap<(PathBuf, Option<String>), Files>>,
    /// Symbolic links: (folder, link) → target path in the same folder.
    links: Mutex<HashMap<(PathBuf, String), String>>,
    changes: Mutex<HashMap<PathBuf, Vec<PathChange>>>,
    ignored: Mutex<Vec<String>>,
    diff: Mutex<Option<FileDiff>>,
    log: Mutex<Vec<CommitInfo>>,
    blame: Mutex<Blame>,
    calls: Mutex<Vec<String>>,
    /// Empty folders created in the working copy: (folder, path).
    folders: Mutex<BTreeSet<(PathBuf, String)>>,
    /// Absolute paths moved to the trash.
    trashed: Mutex<Vec<String>>,
}

impl FakeCode {
    /// A file in `dir`, in the working copy (`rev: None`) or in a commit.
    pub fn add_file(&self, dir: &str, rev: Option<&str>, path: &str, contents: &[u8]) {
        self.files
            .lock()
            .unwrap()
            .entry((dir.into(), rev.map(str::to_owned)))
            .or_default()
            .insert(path.into(), contents.to_vec());
    }
    /// `link` in the working copy of `dir` points to `target`.
    pub fn add_link(&self, dir: &str, link: &str, target: &str) {
        self.links
            .lock()
            .unwrap()
            .insert((dir.into(), link.into()), target.into());
    }
    pub fn set_changes(&self, dir: &str, changes: Vec<PathChange>) {
        self.changes.lock().unwrap().insert(dir.into(), changes);
    }
    pub fn set_ignored(&self, paths: &[&str]) {
        *self.ignored.lock().unwrap() = paths.iter().map(|p| p.to_string()).collect();
    }
    pub fn set_diff(&self, diff: Option<FileDiff>) {
        *self.diff.lock().unwrap() = diff;
    }
    pub fn set_log(&self, log: Vec<CommitInfo>) {
        *self.log.lock().unwrap() = log;
    }
    pub fn set_blame(&self, blame: Blame) {
        *self.blame.lock().unwrap() = blame;
    }
    /// A working file's bytes, as the writer left them.
    pub fn working(&self, dir: &str, path: &str) -> Option<Vec<u8>> {
        self.files
            .lock()
            .unwrap()
            .get(&(PathBuf::from(dir), None))?
            .get(path)
            .cloned()
    }
    pub fn trashed(&self) -> Vec<String> {
        self.trashed.lock().unwrap().clone()
    }
    fn work_files(
        &self,
        dir: &Path,
    ) -> std::sync::MutexGuard<'_, HashMap<(PathBuf, Option<String>), Files>> {
        let mut files = self.files.lock().unwrap();
        files.entry((dir.to_owned(), None)).or_default();
        files
    }
    /// What was asked, one line per call.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn file(&self, dir: &Path, rev: Option<&str>, path: &str) -> Option<(Vec<u8>, String)> {
        let resolved = match rev {
            None => self
                .links
                .lock()
                .unwrap()
                .get(&(dir.to_owned(), path.to_owned()))
                .cloned()
                .unwrap_or_else(|| path.to_owned()),
            Some(_) => path.to_owned(),
        };
        let files = self.files.lock().unwrap();
        let bytes = files
            .get(&(dir.to_owned(), rev.map(str::to_owned)))?
            .get(&resolved)?
            .clone();
        Some((bytes, resolved))
    }
}

impl CodeReader for FakeCode {
    fn files_at(&self, dir: &Path, rev: &str) -> PortResult<Vec<String>> {
        self.record(format!("files_at {} {rev}", dir.display()));
        let files = self.files.lock().unwrap();
        files
            .get(&(dir.to_owned(), Some(rev.to_owned())))
            .map(|f| f.keys().cloned().collect())
            .ok_or_else(|| PortError::new(format!("no existe {rev}")))
    }
    fn ignored_files(&self, _: &Path) -> PortResult<Vec<String>> {
        Ok(self.ignored.lock().unwrap().clone())
    }
    fn changes(&self, dir: &Path, base: &str, to: Option<&str>) -> PortResult<Vec<PathChange>> {
        self.record(format!(
            "changes {} {base}..{}",
            dir.display(),
            to.unwrap_or("work")
        ));
        Ok(self
            .changes
            .lock()
            .unwrap()
            .get(dir)
            .cloned()
            .unwrap_or_default())
    }
    fn read_file(
        &self,
        dir: &Path,
        rev: Option<&str>,
        path: &str,
        max: u64,
    ) -> PortResult<Option<FileBytes>> {
        self.record(format!(
            "read {} {} {path}",
            dir.display(),
            rev.unwrap_or("work")
        ));
        Ok(self.file(dir, rev, path).map(|(bytes, resolved)| {
            let size = bytes.len() as u64;
            FileBytes {
                bytes: bytes.into_iter().take(max as usize).collect(),
                size,
                resolved,
            }
        }))
    }
    fn file_size(&self, dir: &Path, rev: Option<&str>, path: &str) -> PortResult<Option<u64>> {
        Ok(self.file(dir, rev, path).map(|(b, _)| b.len() as u64))
    }
    fn diff_file(
        &self,
        dir: &Path,
        base: &str,
        to: Option<&str>,
        path: &str,
        old_path: Option<&str>,
        context: u32,
    ) -> PortResult<Option<FileDiff>> {
        self.record(format!(
            "diff {} {base}..{} {}{path} -U{context}",
            dir.display(),
            to.unwrap_or("work"),
            old_path.map(|o| format!("{o} → ")).unwrap_or_default()
        ));
        Ok(self.diff.lock().unwrap().clone())
    }
    fn grep(
        &self,
        dir: &Path,
        rev: Option<&str>,
        query: &TextQuery,
        exclude: &[&str],
        exclude_dirs: &[&str],
        max: usize,
    ) -> PortResult<(Vec<(String, LineMatch)>, bool)> {
        self.record(format!(
            "grep {} {} {} excluding {} names and {exclude_dirs:?}",
            dir.display(),
            rev.unwrap_or("work"),
            query.query,
            exclude.len()
        ));
        let files = self.files.lock().unwrap();
        let mut lines = Vec::new();
        for (path, bytes) in files
            .get(&(dir.to_owned(), rev.map(str::to_owned)))
            .into_iter()
            .flatten()
        {
            // The real adapter excludes them through git; the fake trusts
            // the use case to filter (this proves it does).
            let text = String::from_utf8_lossy(bytes);
            for (i, line) in text.lines().enumerate() {
                if let Some(start) = line.find(&query.query) {
                    lines.push((
                        path.clone(),
                        LineMatch {
                            line: i as u32 + 1,
                            text: line.to_owned(),
                            ranges: vec![MatchRange {
                                start: start as u32,
                                end: (start + query.query.len()) as u32,
                            }],
                        },
                    ));
                }
            }
        }
        let truncated = lines.len() > max;
        lines.truncate(max);
        Ok((lines, truncated))
    }
    fn file_log(
        &self,
        dir: &Path,
        rev: Option<&str>,
        path: &str,
        lines: Option<(u32, u32)>,
        max: usize,
    ) -> PortResult<Vec<CommitInfo>> {
        self.record(format!(
            "log {} {} {path} {lines:?} -n{max}",
            dir.display(),
            rev.unwrap_or("HEAD")
        ));
        Ok(self.log.lock().unwrap().clone())
    }
    fn blame(
        &self,
        dir: &Path,
        rev: Option<&str>,
        path: &str,
        lines: Option<(u32, u32)>,
    ) -> PortResult<Blame> {
        self.record(format!(
            "blame {} {} {path} {lines:?}",
            dir.display(),
            rev.unwrap_or("work")
        ));
        Ok(self.blame.lock().unwrap().clone())
    }
}

impl CodeWriter for FakeCode {
    fn kind(&self, dir: &Path, path: &str) -> PortResult<Option<PathKind>> {
        if self
            .links
            .lock()
            .unwrap()
            .contains_key(&(dir.to_owned(), path.to_owned()))
        {
            return Ok(Some(PathKind::Link));
        }
        let files = self.work_files(dir);
        let work = &files[&(dir.to_owned(), None)];
        let prefix = format!("{path}/");
        Ok(if work.contains_key(path) {
            Some(PathKind::File)
        } else if work.keys().any(|p| p.starts_with(&prefix))
            || self
                .folders
                .lock()
                .unwrap()
                .contains(&(dir.to_owned(), path.to_owned()))
        {
            Some(PathKind::Folder)
        } else {
            None
        })
    }
    fn replace_file(&self, dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()> {
        self.record(format!("write {} {path}", dir.display()));
        let mut files = self.work_files(dir);
        let work = files.get_mut(&(dir.to_owned(), None)).unwrap();
        match work.get_mut(path) {
            Some(old) => {
                *old = bytes.to_vec();
                Ok(())
            }
            None => Err(PortError::new(format!("no existe {path}"))),
        }
    }
    fn create_file(&self, dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()> {
        if self.kind(dir, path)?.is_some() {
            return Err(PortError::new(format!("ya existe {path}")));
        }
        self.record(format!("write {} {path} (new)", dir.display()));
        let mut files = self.work_files(dir);
        files
            .get_mut(&(dir.to_owned(), None))
            .unwrap()
            .insert(path.to_owned(), bytes.to_vec());
        Ok(())
    }
    fn create_folder(&self, dir: &Path, path: &str) -> PortResult<()> {
        if self.kind(dir, path)?.is_some() {
            return Err(PortError::new(format!("ya existe {path}")));
        }
        self.folders
            .lock()
            .unwrap()
            .insert((dir.to_owned(), path.to_owned()));
        Ok(())
    }
    fn rename(&self, dir: &Path, from: &str, to: &str) -> PortResult<()> {
        self.record(format!("rename {} {from} → {to}", dir.display()));
        let mut files = self.work_files(dir);
        let work = files.get_mut(&(dir.to_owned(), None)).unwrap();
        let prefix = format!("{from}/");
        let moved: Vec<String> = work
            .keys()
            .filter(|p| *p == from || p.starts_with(&prefix))
            .cloned()
            .collect();
        if moved.is_empty() {
            return Err(PortError::new(format!("no existe {from}")));
        }
        for old in moved {
            let bytes = work.remove(&old).unwrap();
            work.insert(format!("{to}{}", &old[from.len()..]), bytes);
        }
        Ok(())
    }
    fn copy_file(&self, dir: &Path, from: &str, to: &str) -> PortResult<()> {
        let bytes = self
            .working(&dir.to_string_lossy(), from)
            .ok_or_else(|| PortError::new(format!("no existe {from}")))?;
        self.create_file(dir, to, &bytes)
    }
    fn walk(&self, dir: &Path, folder: &str, max: usize) -> PortResult<(Vec<String>, bool)> {
        let files = self.work_files(dir);
        let prefix = format!("{folder}/");
        let mut all: Vec<String> = files[&(dir.to_owned(), None)]
            .keys()
            .filter(|p| p.starts_with(&prefix))
            .cloned()
            .collect();
        let more = all.len() > max;
        all.truncate(max);
        Ok((all, more))
    }
    fn trash(&self, dir: &Path, path: &str) -> PortResult<()> {
        let mut files = self.work_files(dir);
        let work = files.get_mut(&(dir.to_owned(), None)).unwrap();
        let prefix = format!("{path}/");
        let before = work.len();
        work.retain(|p, _| p != path && !p.starts_with(&prefix));
        let folder = self
            .folders
            .lock()
            .unwrap()
            .remove(&(dir.to_owned(), path.to_owned()));
        if work.len() == before && !folder {
            return Err(PortError::new(format!("no existe {path}")));
        }
        self.trashed
            .lock()
            .unwrap()
            .push(format!("{}/{path}", dir.display()));
        Ok(())
    }
}
