//! Read-only code navigation and review over a *copy* of the code: the
//! user's folder or a task's worktree, always named explicitly so the UI
//! never mixes them. A task's copy is compared with the commit the task
//! started from ("antes de la IA"); the folder with `HEAD`.
//!
//! When a task's worktree is gone because it was accepted, its files are
//! read from the task's last commit (kept in its merge record) as a
//! read-only snapshot. A discarded task has no files left.
//!
//! Protected files (`.env`, keys...) are listed, flagged `locked`, and never
//! read, diffed, blamed or searched: the same rule that keeps agents away.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{AppError, AppResult, Hedy};
use crate::domain::attachment::ImageType;
use crate::domain::code::{
    Blame, ChangeKind, CommitInfo, DiffBody, MAX_HISTORY_COMMITS, MAX_PREVIEW_IMAGE_BYTES,
    MAX_SEARCH_MATCHES, MAX_TEXT_BYTES, MAX_TREE_ENTRIES, PathChange, SearchResults, Symbol,
    TextQuery, TreeEntry, content_version, looks_binary, normalize_code_path, outline,
};
use crate::domain::diff::FileStatus;
use crate::domain::permissions::BlockedFiles;
use crate::domain::{LaunchSpec, Task, TaskId, TaskStatus};

/// Git's empty tree: the base of a repository without commits yet.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// Context lines around a change, at most (more is the whole file).
pub const MAX_DIFF_CONTEXT: u32 = 1_000_000;
/// Open tasks whose worktrees a file's history looks into, at most.
const MAX_OPEN_TASKS_CHECKED: usize = 20;
/// The revision that means "the copy's base": the task's starting commit,
/// or the folder's `HEAD`.
pub const BASE_REV: &str = "base";

/// Which copy of the code to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeCopy {
    /// The user's own folder (a registered project), as it is on disk.
    Folder(PathBuf),
    /// A task's worktree (or, once accepted, its last commit).
    Task(TaskId),
}

/// What a copy is, for the header of every code view ("Tu carpeta" /
/// "Tarea: …") and for copying paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyInfo {
    /// The project the copy belongs to.
    pub project: PathBuf,
    /// `None` for the user's folder.
    pub task: Option<TaskId>,
    pub title: Option<String>,
    /// Absolute folder holding the copy's files (for a snapshot, the
    /// project's folder, where the accepted files now live).
    pub root: PathBuf,
    /// Files are read from disk; `false`: from [`CopyInfo::snapshot`].
    pub live: bool,
    /// The commit read when the copy is not live.
    pub snapshot: Option<String>,
    /// What "base" means here: the task's starting commit, or the folder's
    /// `HEAD` (`None` in a repository without commits).
    pub base: Option<String>,
    /// The task's branch, or the folder's current one.
    pub branch: Option<String>,
    /// The branch the task forked from.
    pub base_branch: Option<String>,
    /// The task's agent is working (or waits for an answer) on these files.
    pub agent_working: bool,
}

/// Every file of a copy with its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeTree {
    pub copy: CopyInfo,
    /// Sorted by path; files deleted since the base are included.
    pub entries: Vec<TreeEntry>,
    /// Cut at [`MAX_TREE_ENTRIES`].
    pub truncated: bool,
}

/// A file's contents as the UI can show them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    Text {
        text: String,
        size: u64,
        /// What an editor saves against ([`content_version`]).
        version: String,
    },
    /// An image small enough to preview (PNG, JPEG, GIF, WebP).
    Image {
        media_type: ImageType,
        bytes: Vec<u8>,
        size: u64,
    },
    Binary {
        size: u64,
    },
    TooLarge {
        size: u64,
    },
    /// A protected file: never read.
    Locked,
    /// Not in that copy or revision (e.g. "before the AI" of a new file).
    Missing,
}

/// One file's diff against a revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeDiff {
    pub path: String,
    /// The other side of a rename.
    pub old_path: Option<String>,
    /// `None`: the file is the same on both sides.
    pub status: Option<ChangeKind>,
    /// The commit compared against (resolved), when git knows it.
    pub against: String,
    pub body: DiffBody,
}

/// A commit in a file's history, with the Hedy task it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCommit {
    pub commit: CommitInfo,
    pub task: Option<TaskId>,
}

/// A Hedy task of the project that changed the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTouch {
    pub task: TaskId,
    pub title: String,
    pub status: TaskStatus,
    /// When it was accepted, or created while it is still open (Unix seconds).
    pub at: i64,
    /// Its commit in the history, once accepted.
    pub commit: Option<String>,
    /// Still open: the change lives only in its worktree.
    pub open: bool,
}

/// A file's timeline: commits and the tasks that touched it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistory {
    /// Newest first, at most [`MAX_HISTORY_COMMITS`].
    pub commits: Vec<HistoryCommit>,
    /// Newest first.
    pub tasks: Vec<TaskTouch>,
}

/// A copy resolved to where git runs and what it reads.
pub(super) struct Resolved {
    pub(super) info: CopyInfo,
    /// Where git runs.
    pub(super) dir: PathBuf,
    /// The commit holding the files when the copy is not live.
    pub(super) at: Option<String>,
    /// The revision "base" stands for.
    base: String,
    /// A task copy: new files are `Added`, never `Untracked`.
    task: bool,
    /// The project's files the AI does not read: Hedy does not offer them either.
    pub(super) blocked: BlockedFiles,
}

pub(super) fn checked_path(path: &str) -> AppResult<String> {
    normalize_code_path(path).ok_or_else(|| AppError::InvalidCodePath(path.to_owned()))
}

/// Folders a copy never shows (other tasks' worktrees live there).
pub(super) fn hidden(path: &str) -> bool {
    path == ".hedy" || path.starts_with(".hedy/")
}

/// Lines `(first, last)`, 1-based and inclusive, in order.
fn line_range(lines: Option<(u32, u32)>) -> Option<(u32, u32)> {
    lines.map(|(a, b)| (a.clamp(1, b.max(1)), b.max(a).max(1)))
}

fn change_of(status: FileStatus) -> ChangeKind {
    match status {
        FileStatus::Added => ChangeKind::Added,
        FileStatus::Modified => ChangeKind::Modified,
        FileStatus::Deleted => ChangeKind::Deleted,
        FileStatus::Renamed => ChangeKind::Renamed,
    }
}

impl Hedy {
    pub(super) fn resolve_copy(&self, copy: &CodeCopy) -> AppResult<Resolved> {
        let mut resolved = match copy {
            CodeCopy::Folder(path) => {
                let project = self.project_at(path)?;
                self.folder_copy(&project.path, None)
            }
            CodeCopy::Task(id) => {
                let task = self.task(id)?;
                self.task_copy(&task)
            }
        }?;
        resolved.blocked = self.blocked_files_at(&resolved.info.project);
        Ok(resolved)
    }

    fn folder_copy(&self, path: &Path, task: Option<&Task>) -> AppResult<Resolved> {
        let head = self.git.head_commit(path).ok();
        Ok(Resolved {
            info: CopyInfo {
                project: path.to_owned(),
                task: task.map(|t| t.id.clone()),
                title: task.map(|t| t.title.clone()),
                root: path.to_owned(),
                live: true,
                snapshot: None,
                base: head.clone(),
                branch: self.git.current_branch(path).ok(),
                base_branch: None,
                agent_working: task.is_some_and(agent_working),
            },
            dir: path.to_owned(),
            at: None,
            base: head.unwrap_or_else(|| EMPTY_TREE.to_owned()),
            task: false,
            blocked: BlockedFiles::default(),
        })
    }

    fn task_copy(&self, task: &Task) -> AppResult<Resolved> {
        if task.conversation {
            return Err(AppError::CopyUnavailable(task.id.clone()));
        }
        // A question in the project: it read the user's folder.
        let Some(ws) = &task.workspace else {
            return self.folder_copy(&task.project, Some(task));
        };
        let mut info = CopyInfo {
            project: task.project.clone(),
            task: Some(task.id.clone()),
            title: Some(task.title.clone()),
            root: ws.worktree.clone(),
            live: true,
            snapshot: None,
            base: Some(ws.base_commit.clone()),
            branch: Some(ws.branch.clone()),
            base_branch: Some(ws.base_branch.clone()),
            agent_working: agent_working(task),
        };
        if !task.status.is_closed() && self.fs.exists(&ws.worktree) {
            return Ok(Resolved {
                info,
                dir: ws.worktree.clone(),
                at: None,
                base: ws.base_commit.clone(),
                task: true,
                blocked: BlockedFiles::default(),
            });
        }
        match (&task.status, &task.merge) {
            (TaskStatus::Accepted, Some(record)) => {
                info.root = task.project.clone();
                info.live = false;
                info.snapshot = Some(record.tip.clone());
                Ok(Resolved {
                    info,
                    dir: task.project.clone(),
                    at: Some(record.tip.clone()),
                    base: ws.base_commit.clone(),
                    task: true,
                    blocked: BlockedFiles::default(),
                })
            }
            _ => Err(AppError::CopyUnavailable(task.id.clone())),
        }
    }

    /// The revision to read: `None` = the copy's own files, `"base"` = the
    /// copy's base, anything else a commit (checked).
    fn rev(&self, copy: &Resolved, rev: Option<&str>) -> AppResult<Option<String>> {
        match rev.map(str::trim).filter(|r| !r.is_empty()) {
            None => Ok(copy.at.clone()),
            Some(BASE_REV) => Ok(Some(copy.base.clone())),
            Some(rev) if crate::domain::code::is_safe_rev(rev) => Ok(Some(rev.to_owned())),
            Some(rev) => Err(AppError::InvalidRevision(rev.to_owned())),
        }
    }

    fn copy_changes(&self, copy: &Resolved) -> AppResult<Vec<PathChange>> {
        let mut changes = self
            .code
            .changes(&copy.dir, &copy.base, copy.at.as_deref())?;
        changes.retain(|c| !hidden(&c.path));
        if copy.task {
            for change in &mut changes {
                if change.kind == ChangeKind::Untracked {
                    change.kind = ChangeKind::Added;
                }
            }
        }
        Ok(changes)
    }

    /// What a copy is (for its header), without reading any file.
    pub fn code_copy(&self, copy: &CodeCopy) -> AppResult<CopyInfo> {
        Ok(self.resolve_copy(copy)?.info)
    }

    /// Every file of the copy with what changed since its base; protected
    /// files flagged `locked`. With `ignored`, git-ignored files too (whole
    /// ignored folders as one entry ending with `/`).
    pub fn code_tree(&self, copy: &CodeCopy, ignored: bool) -> AppResult<CodeTree> {
        let copy = self.resolve_copy(copy)?;
        let files = match &copy.at {
            Some(at) => self.code.files_at(&copy.dir, at)?,
            None => self.git.list_files(&copy.dir)?,
        };
        let changes: HashMap<String, PathChange> = self
            .copy_changes(&copy)?
            .into_iter()
            .map(|c| (c.path.clone(), c))
            .collect();
        let entry = |path: String, ignored: bool| {
            let change = changes.get(&path);
            TreeEntry {
                locked: copy.blocked.blocks_path(path.trim_end_matches('/')),
                status: change.map(|c| c.kind),
                old_path: change.and_then(|c| c.old_path.clone()),
                path,
                ignored,
            }
        };
        let mut entries: Vec<TreeEntry> = files
            .into_iter()
            .chain(
                changes
                    .values()
                    .filter(|c| c.kind == ChangeKind::Deleted)
                    .map(|c| c.path.clone()),
            )
            .filter(|p| !hidden(p))
            .map(|p| entry(p, false))
            .collect();
        if ignored && copy.at.is_none() {
            let extra = self.code.ignored_files(&copy.dir)?;
            entries.extend(
                extra
                    .into_iter()
                    .filter(|p| !hidden(p.trim_end_matches('/')))
                    .map(|p| entry(p, true)),
            );
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        entries.dedup_by(|a, b| a.path == b.path);
        let truncated = entries.len() > MAX_TREE_ENTRIES;
        entries.truncate(MAX_TREE_ENTRIES);
        Ok(CodeTree {
            copy: copy.info,
            entries,
            truncated,
        })
    }

    /// A file of the copy (or of `rev`: a commit, or `"base"`), as text,
    /// an image preview or why it cannot be shown.
    pub fn read_code_file(
        &self,
        copy: &CodeCopy,
        path: &str,
        rev: Option<&str>,
    ) -> AppResult<FileContent> {
        let copy = self.resolve_copy(copy)?;
        let path = checked_path(path)?;
        let rev = self.rev(&copy, rev)?;
        self.read_resolved(&copy, &path, rev.as_deref())
    }

    fn read_resolved(
        &self,
        copy: &Resolved,
        path: &str,
        rev: Option<&str>,
    ) -> AppResult<FileContent> {
        if copy.blocked.blocks_path(path) {
            return Ok(FileContent::Locked);
        }
        let max = MAX_TEXT_BYTES.max(MAX_PREVIEW_IMAGE_BYTES);
        let Some(file) = self.code.read_file(&copy.dir, rev, path, max)? else {
            return Ok(FileContent::Missing);
        };
        // A symbolic link to a protected file is protected too.
        if copy.blocked.blocks_path(&file.resolved) {
            return Ok(FileContent::Locked);
        }
        let size = file.size;
        if let Some(media_type) = ImageType::sniff(&file.bytes) {
            return Ok(if size > MAX_PREVIEW_IMAGE_BYTES {
                FileContent::TooLarge { size }
            } else {
                FileContent::Image {
                    media_type,
                    bytes: file.bytes,
                    size,
                }
            });
        }
        if looks_binary(&file.bytes) {
            return Ok(FileContent::Binary { size });
        }
        if size > MAX_TEXT_BYTES {
            return Ok(FileContent::TooLarge { size });
        }
        Ok(FileContent::Text {
            text: String::from_utf8_lossy(&file.bytes).into_owned(),
            version: content_version(&file.bytes),
            size,
        })
    }

    /// One file's diff from `against` (a commit, or `"base"` when `None`)
    /// to the copy, with `context` lines around each change (the UI asks
    /// for [`MAX_DIFF_CONTEXT`] to get the whole file).
    pub fn code_diff(
        &self,
        copy: &CodeCopy,
        path: &str,
        against: Option<&str>,
        context: u32,
    ) -> AppResult<CodeDiff> {
        let copy = self.resolve_copy(copy)?;
        let path = checked_path(path)?;
        let against = against.map(str::trim).filter(|a| !a.is_empty());
        let is_base = against.is_none_or(|a| a == BASE_REV);
        let from = self
            .rev(&copy, Some(against.unwrap_or(BASE_REV)))?
            .unwrap_or_else(|| copy.base.clone());
        let resolved = self
            .git
            .rev_parse(&copy.dir, &from)
            .unwrap_or_else(|_| from.clone());
        // Against the base, the copy's changes say whether it was renamed.
        let change = if is_base {
            self.copy_changes(&copy)?
                .into_iter()
                .find(|c| c.path == path)
        } else {
            None
        };
        let old_path = change.as_ref().and_then(|c| c.old_path.clone());
        let mut diff = CodeDiff {
            path: path.clone(),
            old_path: old_path.clone(),
            status: change.as_ref().map(|c| c.kind),
            against: resolved,
            body: DiffBody::Locked,
        };
        let blocked = |p: &str| copy.blocked.blocks_path(p);
        if blocked(&path) || old_path.as_deref().is_some_and(blocked) {
            return Ok(diff);
        }
        let to = copy.at.as_deref();
        let old_side = old_path.as_deref().unwrap_or(&path);
        let size = self
            .code
            .file_size(&copy.dir, to, &path)?
            .max(self.code.file_size(&copy.dir, Some(&from), old_side)?)
            .unwrap_or(0);
        if size > MAX_TEXT_BYTES {
            diff.body = DiffBody::TooLarge { size };
            return Ok(diff);
        }
        let found = self.code.diff_file(
            &copy.dir,
            &from,
            to,
            &path,
            old_path.as_deref(),
            context.min(MAX_DIFF_CONTEXT),
        )?;
        diff.body = match found {
            None => {
                diff.status = None;
                DiffBody::Text {
                    additions: 0,
                    deletions: 0,
                    hunks: Vec::new(),
                }
            }
            Some(file) => {
                diff.status = diff.status.or(Some(change_of(file.status)));
                if file.binary {
                    DiffBody::Binary
                } else {
                    DiffBody::Text {
                        additions: file.additions,
                        deletions: file.deletions,
                        hunks: file.hunks,
                    }
                }
            }
        };
        Ok(diff)
    }

    /// Text search in the copy's files (`git grep`), untracked ones
    /// included; never in protected files or `.hedy/`. Empty for an empty
    /// query; at most [`MAX_SEARCH_MATCHES`] lines.
    pub fn search_code(&self, copy: &CodeCopy, query: &TextQuery) -> AppResult<SearchResults> {
        let copy = self.resolve_copy(copy)?;
        if query.query.trim().is_empty() {
            return Ok(SearchResults::default());
        }
        let blocked: Vec<&str> = copy.blocked.patterns().iter().map(String::as_str).collect();
        let (mut lines, truncated) = self.code.grep(
            &copy.dir,
            copy.at.as_deref(),
            query,
            &blocked,
            &[".hedy"],
            MAX_SEARCH_MATCHES,
        )?;
        lines.retain(|(path, _)| !copy.blocked.blocks_path(path) && !hidden(path));
        Ok(SearchResults::from_lines(lines, truncated))
    }

    /// The file's commits (following renames) and the project's tasks that
    /// changed it: accepted ones by their commit in the history, open ones
    /// by their worktree. With `lines`, only commits that touched that
    /// range (numbered as in the copy's `HEAD`, or its snapshot).
    pub fn file_history(
        &self,
        copy: &CodeCopy,
        path: &str,
        lines: Option<(u32, u32)>,
    ) -> AppResult<FileHistory> {
        let copy = self.resolve_copy(copy)?;
        let path = checked_path(path)?;
        let log = self.code.file_log(
            &copy.dir,
            copy.at.as_deref(),
            &path,
            line_range(lines),
            MAX_HISTORY_COMMITS,
        )?;
        let mut commits: Vec<HistoryCommit> = log
            .into_iter()
            .map(|commit| HistoryCommit { commit, task: None })
            .collect();
        let mut tasks = Vec::new();
        let mut open_checked = 0;
        for task in self.store.tasks(&copy.info.project)? {
            let Some(ws) = &task.workspace else { continue };
            if task.conversation {
                continue;
            }
            if let (TaskStatus::Accepted, Some(record)) = (task.status, &task.merge) {
                let Some(found) = commits
                    .iter_mut()
                    .find(|c| c.commit.hash == record.tip || c.commit.hash == record.after)
                else {
                    continue;
                };
                found.task = Some(task.id.clone());
                tasks.push(TaskTouch {
                    task: task.id.clone(),
                    title: task.title.clone(),
                    status: task.status,
                    at: record.at,
                    commit: Some(found.commit.hash.clone()),
                    open: false,
                });
            } else if !task.status.is_closed()
                && open_checked < MAX_OPEN_TASKS_CHECKED
                && self.fs.exists(&ws.worktree)
            {
                open_checked += 1;
                let touched = self
                    .code
                    .changes(&ws.worktree, &ws.base_commit, None)
                    .unwrap_or_default()
                    .iter()
                    .any(|c| c.path == path || c.old_path.as_deref() == Some(&path));
                if touched {
                    tasks.push(TaskTouch {
                        task: task.id.clone(),
                        title: task.title.clone(),
                        status: task.status,
                        at: task.created_at,
                        commit: None,
                        open: true,
                    });
                }
            }
        }
        tasks.sort_by_key(|t| std::cmp::Reverse(t.at));
        Ok(FileHistory { commits, tasks })
    }

    /// Who wrote each line of the copy's file (or of `rev`); with `lines`,
    /// only that range. Lines the copy changed and nobody committed yet
    /// come from an `uncommitted` commit.
    pub fn blame_file(
        &self,
        copy: &CodeCopy,
        path: &str,
        rev: Option<&str>,
        lines: Option<(u32, u32)>,
    ) -> AppResult<Blame> {
        let copy = self.resolve_copy(copy)?;
        let path = checked_path(path)?;
        if copy.blocked.blocks_path(&path) {
            return Err(AppError::FileLocked(path));
        }
        let rev = self.rev(&copy, rev)?;
        Ok(self
            .code
            .blame(&copy.dir, rev.as_deref(), &path, line_range(lines))?)
    }

    /// The functions, types and headings of a file (see
    /// [`crate::domain::code::outline`] for what it can and cannot see).
    /// Empty for anything that is not readable text.
    pub fn file_symbols(&self, copy: &CodeCopy, path: &str) -> AppResult<Vec<Symbol>> {
        let resolved = self.resolve_copy(copy)?;
        let path = checked_path(path)?;
        let at = resolved.at.clone();
        Ok(match self.read_resolved(&resolved, &path, at.as_deref())? {
            FileContent::Text { text, .. } => outline(&path, &text),
            _ => Vec::new(),
        })
    }

    /// The file's absolute path in the copy (for a snapshot, in the
    /// project's folder, where the accepted files now live).
    fn absolute(&self, copy: &CodeCopy, path: &str) -> AppResult<PathBuf> {
        let copy = self.resolve_copy(copy)?;
        Ok(copy.info.root.join(checked_path(path)?))
    }

    /// Opens the file in VS Code at `line` (`code -g file:line`), or with the
    /// system's default application when VS Code is not installed.
    pub fn open_code_in_editor(
        &self,
        copy: &CodeCopy,
        path: &str,
        line: Option<u32>,
    ) -> AppResult<()> {
        let file = self.absolute(copy, path)?;
        let file = file.to_string_lossy().into_owned();
        let spec = if self.runner.is_installed("code") {
            let target = match line.filter(|&l| l > 0) {
                Some(line) => format!("{file}:{line}"),
                None => file,
            };
            LaunchSpec::program("code", vec!["-g".into(), target])
        } else {
            LaunchSpec::program(system_opener(), vec![file])
        };
        Ok(self.runner.spawn_detached(&spec)?)
    }

    /// Shows the file in the system's file manager: its folder opened
    /// (`xdg-open`), or the file selected in Finder on macOS.
    pub fn reveal_code_file(&self, copy: &CodeCopy, path: &str) -> AppResult<()> {
        let file = self.absolute(copy, path)?;
        let spec = if cfg!(target_os = "macos") {
            LaunchSpec::program(
                "open",
                vec!["-R".into(), file.to_string_lossy().into_owned()],
            )
        } else {
            let folder = file.parent().unwrap_or(&file);
            LaunchSpec::program(system_opener(), vec![folder.to_string_lossy().into_owned()])
        };
        Ok(self.runner.spawn_detached(&spec)?)
    }
}

fn agent_working(task: &Task) -> bool {
    matches!(
        task.status,
        TaskStatus::Running | TaskStatus::WaitingApproval
    )
}

fn system_opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewTask;
    use crate::application::testing::Fixture;
    use crate::domain::code::{BlameCommit, SymbolKind};
    use crate::domain::diff::{DiffLine, Hunk, LineKind};
    use crate::domain::{AgentEvent, FileDiff, TaskMode};

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

    fn change(path: &str, kind: ChangeKind) -> PathChange {
        PathChange {
            path: path.into(),
            old_path: None,
            kind,
        }
    }

    fn new_task(f: &Fixture, prompt: &str) -> (Task, String) {
        let task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: f.dir(ROOT),
                prompt: prompt.into(),
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

    /// A project task whose worktree exists; returns it and its worktree.
    fn project_task(f: &Fixture) -> (Task, String) {
        project(f);
        new_task(f, "Arregla los impuestos")
    }

    fn finish(f: &Fixture) {
        f.agents.emit(AgentEvent::Finished {
            ok: true,
            summary: "Listo".into(),
            duration_ms: None,
        });
    }

    #[test]
    fn the_folder_tree_has_statuses_locks_and_deleted_files() {
        let f = Fixture::new();
        project(&f);
        f.git.set_files(&[
            "src/app.py",
            "README.md",
            ".env",
            "nuevo.py",
            ".hedy/worktrees/t1/app.py",
        ]);
        f.code.set_changes(
            ROOT,
            vec![
                change("src/app.py", ChangeKind::Modified),
                change("nuevo.py", ChangeKind::Untracked),
                change("viejo.py", ChangeKind::Deleted),
                change(".hedy/worktrees/t1/app.py", ChangeKind::Untracked),
            ],
        );
        f.code.set_ignored(&["build/", ".hedy/", "secrets.pem"]);

        let tree = f.hedy.code_tree(&folder(), false).unwrap();
        let entries: Vec<_> = tree
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e.status, e.locked))
            .collect();
        assert_eq!(
            entries,
            [
                (".env", None, true),
                ("README.md", None, false),
                ("nuevo.py", Some(ChangeKind::Untracked), false),
                ("src/app.py", Some(ChangeKind::Modified), false),
                ("viejo.py", Some(ChangeKind::Deleted), false),
            ]
        );
        assert!(!tree.truncated);
        assert_eq!(tree.copy.task, None);
        assert_eq!(tree.copy.base.as_deref(), Some("abc123"));
        assert_eq!(tree.copy.branch.as_deref(), Some("main"));
        assert!(tree.copy.live);
        assert!(
            f.code
                .calls()
                .contains(&format!("changes {ROOT} abc123..work"))
        );

        let with_ignored = f.hedy.code_tree(&folder(), true).unwrap();
        let ignored: Vec<_> = with_ignored
            .entries
            .iter()
            .filter(|e| e.ignored)
            .map(|e| (e.path.as_str(), e.locked))
            .collect();
        assert_eq!(ignored, [("build/", false), ("secrets.pem", true)]);

        assert!(matches!(
            f.hedy
                .code_tree(&CodeCopy::Folder(f.dir("/code/otro")), false),
            Err(AppError::ProjectNotFound(_))
        ));
    }

    #[test]
    fn a_task_copy_is_its_worktree_against_its_base() {
        let f = Fixture::new();
        let (task, wt) = project_task(&f);
        f.git
            .set_files(&["facturas/impuestos.py", "tests/test_nuevo.py"]);
        f.code.set_changes(
            &wt,
            vec![
                change("facturas/impuestos.py", ChangeKind::Modified),
                change("tests/test_nuevo.py", ChangeKind::Untracked),
            ],
        );
        let copy = CodeCopy::Task(task.id.clone());
        let tree = f.hedy.code_tree(&copy, true).unwrap();
        assert_eq!(
            tree.entries[1].status,
            Some(ChangeKind::Added),
            "never untracked"
        );
        let info = tree.copy;
        assert_eq!(info.root, PathBuf::from(&wt));
        assert_eq!(info.title.as_deref(), Some("Arregla los impuestos"));
        assert_eq!(info.base.as_deref(), Some("abc123"));
        assert_eq!(info.base_branch.as_deref(), Some("main"));
        assert!(info.agent_working);
        finish(&f);
        assert!(!f.hedy.code_copy(&copy).unwrap().agent_working);
    }

    #[test]
    fn an_accepted_task_is_read_from_its_last_commit_and_a_discarded_one_is_gone() {
        let f = Fixture::new();
        let (task, _) = project_task(&f);
        finish(&f);
        f.hedy.accept_task(&task.id).unwrap();
        f.code
            .add_file(ROOT, Some("tip"), "facturas/impuestos.py", b"IVA = 0.19\n");
        f.code.set_changes(
            ROOT,
            vec![change("facturas/impuestos.py", ChangeKind::Modified)],
        );
        let copy = CodeCopy::Task(task.id.clone());
        let tree = f.hedy.code_tree(&copy, true).unwrap();
        assert!(!tree.copy.live);
        assert_eq!(tree.copy.snapshot.as_deref(), Some("tip"));
        assert_eq!(tree.copy.root, PathBuf::from(ROOT));
        assert_eq!(tree.entries.len(), 1);
        assert!(
            f.code
                .calls()
                .contains(&format!("changes {ROOT} abc123..tip"))
        );
        assert_eq!(
            f.hedy
                .read_code_file(&copy, "facturas/impuestos.py", None)
                .unwrap(),
            FileContent::Text {
                text: "IVA = 0.19\n".into(),
                size: 11,
                version: content_version(b"IVA = 0.19\n"),
            }
        );

        let f = Fixture::new();
        let (discarded, _) = project_task(&f);
        f.hedy.discard_task(&discarded.id).unwrap();
        assert_eq!(
            f.hedy.code_copy(&CodeCopy::Task(discarded.id.clone())),
            Err(AppError::CopyUnavailable(discarded.id.clone()))
        );
    }

    #[test]
    fn reads_text_images_and_says_why_it_cannot_show_the_rest() {
        let f = Fixture::new();
        project(&f);
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        f.code.add_file(ROOT, None, "src/app.py", b"print(1)\n");
        f.code
            .add_file(ROOT, Some("abc123"), "src/app.py", b"print(0)\n");
        f.code.add_file(ROOT, None, "logo.png", png);
        f.code.add_file(ROOT, None, "data.bin", b"\0\x01\x02");
        f.code.add_file(ROOT, None, ".env", b"TOKEN=x");
        f.code.add_file(ROOT, None, ".env.real", b"TOKEN=x");
        f.code.add_link(ROOT, "config.txt", ".env.real");
        let big = vec![b'a'; MAX_TEXT_BYTES as usize + 1];
        f.code.add_file(ROOT, None, "big.csv", &big);

        let read = |path: &str, rev: Option<&str>| f.hedy.read_code_file(&folder(), path, rev);
        assert_eq!(
            read("./src/app.py", None).unwrap(),
            FileContent::Text {
                text: "print(1)\n".into(),
                size: 9,
                version: content_version(b"print(1)\n"),
            }
        );
        assert!(matches!(
            read("src/app.py", Some("base")).unwrap(),
            FileContent::Text { text, .. } if text == "print(0)\n"
        ));
        assert_eq!(
            read("logo.png", None).unwrap(),
            FileContent::Image {
                media_type: ImageType::Png,
                bytes: png.to_vec(),
                size: png.len() as u64
            }
        );
        assert_eq!(
            read("data.bin", None).unwrap(),
            FileContent::Binary { size: 3 }
        );
        assert_eq!(
            read("big.csv", None).unwrap(),
            FileContent::TooLarge {
                size: MAX_TEXT_BYTES + 1
            }
        );
        assert_eq!(read("nada.py", None).unwrap(), FileContent::Missing);
        assert_eq!(read(".env", None).unwrap(), FileContent::Locked);
        assert_eq!(read("config.txt", None).unwrap(), FileContent::Locked);
        assert!(
            !f.code.calls().iter().any(|c| c.ends_with(" .env")),
            "a protected file is never even opened"
        );
        assert_eq!(
            read("../otro/app.py", None),
            Err(AppError::InvalidCodePath("../otro/app.py".into()))
        );
        assert_eq!(
            read("src/app.py", Some("--output=/tmp/x")),
            Err(AppError::InvalidRevision("--output=/tmp/x".into()))
        );
    }

    fn hunk() -> Hunk {
        Hunk {
            header: "@@ -1 +1 @@".into(),
            lines: vec![
                DiffLine {
                    kind: LineKind::Removed,
                    old_line: Some(1),
                    new_line: None,
                    text: "a".into(),
                },
                DiffLine {
                    kind: LineKind::Added,
                    old_line: None,
                    new_line: Some(1),
                    text: "b".into(),
                },
            ],
        }
    }

    #[test]
    fn diffs_against_the_base_with_renames_and_states() {
        let f = Fixture::new();
        let (task, wt) = project_task(&f);
        let copy = CodeCopy::Task(task.id.clone());
        f.code.set_changes(
            &wt,
            vec![PathChange {
                path: "src/nuevo.py".into(),
                old_path: Some("viejo.py".into()),
                kind: ChangeKind::Renamed,
            }],
        );
        f.code.add_file(&wt, None, "src/nuevo.py", b"b\n");
        f.code.add_file(&wt, Some("abc123"), "viejo.py", b"a\n");
        f.code.set_diff(Some(FileDiff {
            path: "src/nuevo.py".into(),
            old_path: Some("viejo.py".into()),
            status: FileStatus::Renamed,
            additions: 1,
            deletions: 1,
            binary: false,
            hunks: vec![hunk()],
        }));
        let diff = f
            .hedy
            .code_diff(&copy, "src/nuevo.py", None, u32::MAX)
            .unwrap();
        assert_eq!(diff.old_path.as_deref(), Some("viejo.py"));
        assert_eq!(diff.status, Some(ChangeKind::Renamed));
        assert_eq!(diff.against, "abc123");
        assert_eq!(
            diff.body,
            DiffBody::Text {
                additions: 1,
                deletions: 1,
                hunks: vec![hunk()]
            }
        );
        assert!(f.code.calls().contains(&format!(
            "diff {wt} abc123..work viejo.py → src/nuevo.py -U{MAX_DIFF_CONTEXT}"
        )));

        // Against another commit: no rename lookup; unchanged and binary.
        f.code.set_diff(None);
        let same = f
            .hedy
            .code_diff(&copy, "src/nuevo.py", Some("7bd20aa"), 3)
            .unwrap();
        assert_eq!((same.status, same.old_path), (None, None));
        assert!(matches!(same.body, DiffBody::Text { ref hunks, .. } if hunks.is_empty()));
        f.code.set_diff(Some(FileDiff {
            path: "logo.png".into(),
            old_path: None,
            status: FileStatus::Added,
            additions: 0,
            deletions: 0,
            binary: true,
            hunks: vec![],
        }));
        let binary = f.hedy.code_diff(&copy, "logo.png", None, 3).unwrap();
        assert_eq!(
            (binary.status, binary.body),
            (Some(ChangeKind::Added), DiffBody::Binary)
        );

        let locked = f.hedy.code_diff(&copy, "config/.env", None, 3).unwrap();
        assert_eq!(locked.body, DiffBody::Locked);
        let big = vec![b'x'; MAX_TEXT_BYTES as usize + 5];
        f.code.add_file(&wt, Some("abc123"), "datos.csv", &big);
        let large = f.hedy.code_diff(&copy, "datos.csv", None, 3).unwrap();
        assert_eq!(
            large.body,
            DiffBody::TooLarge {
                size: MAX_TEXT_BYTES + 5
            }
        );
    }

    #[test]
    fn a_project_that_blocks_nothing_shows_its_env_like_any_file() {
        let f = Fixture::new();
        project(&f);
        f.hedy.set_project_blocked_files(&f.dir(ROOT), &[]).unwrap();
        f.code.add_file(ROOT, None, ".env", b"TOKEN=x\n");
        let results = f
            .hedy
            .search_code(
                &folder(),
                &TextQuery {
                    query: "TOKEN".into(),
                    ..TextQuery::default()
                },
            )
            .unwrap();
        assert_eq!(results.files[0].path, ".env");
        assert!(matches!(
            f.hedy.read_code_file(&folder(), ".env", None).unwrap(),
            FileContent::Text { .. }
        ));
    }

    #[test]
    fn search_never_returns_protected_files_and_skips_empty_queries() {
        let f = Fixture::new();
        project(&f);
        f.code
            .add_file(ROOT, None, "src/app.py", b"TOKEN = leer()\n");
        f.code.add_file(ROOT, None, ".env", b"TOKEN=x\n");
        f.code
            .add_file(ROOT, None, ".hedy/worktrees/t1/app.py", b"TOKEN\n");
        let query = |q: &str| TextQuery {
            query: q.into(),
            ..TextQuery::default()
        };
        let results = f.hedy.search_code(&folder(), &query("TOKEN")).unwrap();
        assert_eq!(results.total, 1);
        assert_eq!(results.files[0].path, "src/app.py");
        assert_eq!(results.files[0].matches[0].ranges[0].end, 5);
        let call = f.code.calls().pop().unwrap();
        assert!(
            call.contains(&format!(
                "excluding {} names",
                crate::domain::permissions::SUGGESTED_BLOCKED.len()
            )),
            "{call}"
        );
        assert!(call.contains("[\".hedy\"]"), "{call}");
        assert_eq!(
            f.hedy.search_code(&folder(), &query("  ")).unwrap(),
            SearchResults::default()
        );
    }

    fn commit(hash: &str, subject: &str) -> CommitInfo {
        CommitInfo {
            hash: hash.into(),
            short: hash.into(),
            author: "Ana".into(),
            date: 1_700_000_000,
            subject: subject.into(),
            path: "facturas/impuestos.py".into(),
        }
    }

    #[test]
    fn history_mixes_commits_with_the_tasks_that_touched_the_file() {
        let f = Fixture::new();
        let (accepted, _) = project_task(&f);
        finish(&f);
        f.hedy.accept_task(&accepted.id).unwrap();
        let (open, open_wt) = new_task(&f, "Otra mejora");
        f.code.set_changes(
            &open_wt,
            vec![change("facturas/impuestos.py", ChangeKind::Modified)],
        );
        f.code.set_log(vec![
            commit("tip", "Arregla los impuestos"),
            commit("a3f9c1e", "Inicio"),
        ]);

        let history = f
            .hedy
            .file_history(&folder(), "facturas/impuestos.py", None)
            .unwrap();
        assert_eq!(history.commits[0].task.as_ref(), Some(&accepted.id));
        assert_eq!(history.commits[1].task, None);
        let tasks: Vec<_> = history
            .tasks
            .iter()
            .map(|t| (t.task.clone(), t.open, t.commit.clone()))
            .collect();
        assert!(tasks.contains(&(accepted.id.clone(), false, Some("tip".into()))));
        assert!(tasks.contains(&(open.id.clone(), true, None)));

        f.hedy
            .file_history(&folder(), "facturas/impuestos.py", Some((9, 4)))
            .unwrap();
        assert!(
            f.code
                .calls()
                .iter()
                .any(|c| c.ends_with("facturas/impuestos.py Some((4, 9)) -n200")),
            "{:?}",
            f.code.calls()
        );
    }

    #[test]
    fn blame_and_symbols_respect_protected_files() {
        let f = Fixture::new();
        project(&f);
        let hash = "a".repeat(40);
        let mut blame = Blame {
            commits: vec![BlameCommit {
                hash: hash.clone(),
                short: "aaaaaaa".into(),
                author: "Ana".into(),
                date: 1,
                summary: "Inicio".into(),
                uncommitted: false,
            }],
            ranges: vec![],
        };
        blame.push_lines(&hash, 1, 3);
        f.code.set_blame(blame.clone());
        assert_eq!(
            f.hedy
                .blame_file(&folder(), "src/app.py", Some("base"), Some((2, 3)))
                .unwrap(),
            blame
        );
        assert_eq!(
            f.code.calls().pop().unwrap(),
            format!("blame {ROOT} abc123 src/app.py Some((2, 3))")
        );
        assert_eq!(
            f.hedy
                .blame_file(&folder(), "deploy/server.pem", None, None),
            Err(AppError::FileLocked("deploy/server.pem".into()))
        );

        f.code.add_file(
            ROOT,
            None,
            "facturas/impuestos.py",
            b"class Factura:\n    def total(self):\n        pass\n",
        );
        let symbols = f
            .hedy
            .file_symbols(&folder(), "facturas/impuestos.py")
            .unwrap();
        assert_eq!(
            symbols
                .iter()
                .map(|s| (s.name.as_str(), s.kind))
                .collect::<Vec<_>>(),
            [
                ("Factura", SymbolKind::Class),
                ("total", SymbolKind::Method)
            ]
        );
        f.code.add_file(ROOT, None, ".env.py", b"def x(): pass\n");
        assert!(
            f.hedy
                .file_symbols(&folder(), ".env.py")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn opens_the_editor_at_a_line_or_reveals_the_folder() {
        let f = Fixture::new();
        project(&f);
        f.hedy
            .open_code_in_editor(&folder(), "src/app.py", Some(12))
            .unwrap();
        let spec = f.runner.detached().pop().unwrap();
        assert_eq!(
            (spec.program.as_str(), spec.args),
            ("xdg-open", vec![format!("{ROOT}/src/app.py")])
        );
        f.runner.install("code");
        f.hedy
            .open_code_in_editor(&folder(), "src/app.py", Some(12))
            .unwrap();
        let spec = f.runner.detached().pop().unwrap();
        assert_eq!(
            (spec.program.as_str(), spec.args),
            (
                "code",
                vec!["-g".to_owned(), format!("{ROOT}/src/app.py:12")]
            )
        );
        f.hedy.reveal_code_file(&folder(), "src/app.py").unwrap();
        let spec = f.runner.detached().pop().unwrap();
        assert_eq!(
            (spec.program.as_str(), spec.args),
            ("xdg-open", vec![format!("{ROOT}/src")])
        );
        assert!(matches!(
            f.hedy.open_code_in_editor(&folder(), "/etc/passwd", None),
            Err(AppError::InvalidCodePath(_))
        ));
    }
}
