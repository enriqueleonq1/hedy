//! In-memory fake of [`CodeReader`], for the code navigation use cases.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::ports::{CodeReader, PortError, PortResult};
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
