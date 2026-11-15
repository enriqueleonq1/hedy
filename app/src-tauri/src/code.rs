//! Code navigation and review commands (read-only), with their DTOs: the
//! file tree of a copy, file contents, one file's diff, text search,
//! history, blame, a file's outline, and opening a file outside Hedy.
//! Mirrored by `app/src/api/codeTypes.ts`.

use std::path::PathBuf;

use base64::Engine as _;
use hedy_core::application::{
    CodeCopy, CodeDiff, CodeTree, CopyInfo, FileContent, FileHistory, MAX_DIFF_CONTEXT,
};
use hedy_core::domain::TaskId;
use hedy_core::domain::TaskStatus;
use hedy_core::domain::code::{
    Blame, ChangeKind, CommitInfo, DiffBody, SearchResults, Symbol, TextQuery, TreeEntry,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;
use crate::commands::{CommandResult, blocking};

/// Which copy of the code: `{ kind: "folder", project }` or
/// `{ kind: "task", task }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CodeCopyInput {
    Folder { project: PathBuf },
    Task { task: String },
}

impl From<CodeCopyInput> for CodeCopy {
    fn from(copy: CodeCopyInput) -> Self {
        match copy {
            CodeCopyInput::Folder { project } => CodeCopy::Folder(project),
            CodeCopyInput::Task { task } => CodeCopy::Task(TaskId::new(task)),
        }
    }
}

/// Lines `start..=end`, 1-based.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

fn lines(range: Option<LineRange>) -> Option<(u32, u32)> {
    range.map(|r| (r.start, r.end))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub glob: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiffOptions {
    /// A commit, or `"base"` (the default).
    pub against: Option<String>,
    /// Lines around each change (3 by default).
    pub context: Option<u32>,
    /// The whole file as context.
    pub full: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyInfoDto {
    /// `folder` or `task`.
    pub kind: &'static str,
    pub project: String,
    pub task: Option<String>,
    pub title: Option<String>,
    pub root: String,
    pub live: bool,
    pub snapshot: Option<String>,
    pub base: Option<String>,
    pub branch: Option<String>,
    pub base_branch: Option<String>,
    pub agent_working: bool,
}

impl From<CopyInfo> for CopyInfoDto {
    fn from(c: CopyInfo) -> Self {
        Self {
            kind: if c.task.is_some() { "task" } else { "folder" },
            project: c.project.to_string_lossy().into_owned(),
            task: c.task.map(|t| t.to_string()),
            title: c.title,
            root: c.root.to_string_lossy().into_owned(),
            live: c.live,
            snapshot: c.snapshot,
            base: c.base,
            branch: c.branch,
            base_branch: c.base_branch,
            agent_working: c.agent_working,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeTreeDto {
    pub copy: CopyInfoDto,
    pub entries: Vec<TreeEntry>,
    pub truncated: bool,
}

impl From<CodeTree> for CodeTreeDto {
    fn from(t: CodeTree) -> Self {
        Self {
            copy: t.copy.into(),
            entries: t.entries,
            truncated: t.truncated,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileContentDto {
    Text {
        text: String,
        size: u64,
    },
    #[serde(rename_all = "camelCase")]
    Image {
        mime: &'static str,
        size: u64,
        data_url: String,
    },
    Binary {
        size: u64,
    },
    TooLarge {
        size: u64,
    },
    Locked,
    Missing,
}

impl From<FileContent> for FileContentDto {
    fn from(c: FileContent) -> Self {
        match c {
            FileContent::Text { text, size } => Self::Text { text, size },
            FileContent::Image {
                media_type,
                bytes,
                size,
            } => Self::Image {
                mime: media_type.media_type(),
                size,
                data_url: format!(
                    "data:{};base64,{}",
                    media_type.media_type(),
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ),
            },
            FileContent::Binary { size } => Self::Binary { size },
            FileContent::TooLarge { size } => Self::TooLarge { size },
            FileContent::Locked => Self::Locked,
            FileContent::Missing => Self::Missing,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeDiffDto {
    pub path: String,
    pub old_path: Option<String>,
    pub status: Option<ChangeKind>,
    pub against: String,
    pub body: DiffBody,
}

impl From<CodeDiff> for CodeDiffDto {
    fn from(d: CodeDiff) -> Self {
        Self {
            path: d.path,
            old_path: d.old_path,
            status: d.status,
            against: d.against,
            body: d.body,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCommitDto {
    #[serde(flatten)]
    pub commit: CommitInfo,
    pub task: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTouchDto {
    pub task: String,
    pub title: String,
    pub status: TaskStatus,
    pub at: i64,
    pub commit: Option<String>,
    pub open: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryDto {
    pub commits: Vec<HistoryCommitDto>,
    pub tasks: Vec<TaskTouchDto>,
}

impl From<FileHistory> for FileHistoryDto {
    fn from(h: FileHistory) -> Self {
        Self {
            commits: h
                .commits
                .into_iter()
                .map(|c| HistoryCommitDto {
                    commit: c.commit,
                    task: c.task.map(|t| t.to_string()),
                })
                .collect(),
            tasks: h
                .tasks
                .into_iter()
                .map(|t| TaskTouchDto {
                    task: t.task.to_string(),
                    title: t.title,
                    status: t.status,
                    at: t.at,
                    commit: t.commit,
                    open: t.open,
                })
                .collect(),
        }
    }
}

/// What a copy is, for the header of a code view.
#[tauri::command]
pub async fn code_copy(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
) -> CommandResult<CopyInfoDto> {
    blocking(&state, move |h| Ok(h.code_copy(&copy.into())?.into())).await
}

#[tauri::command]
pub async fn code_tree(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    ignored: Option<bool>,
) -> CommandResult<CodeTreeDto> {
    blocking(&state, move |h| {
        Ok(h.code_tree(&copy.into(), ignored.unwrap_or(false))?.into())
    })
    .await
}

#[tauri::command]
pub async fn read_file(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    path: String,
    rev: Option<String>,
) -> CommandResult<FileContentDto> {
    blocking(&state, move |h| {
        Ok(h.read_code_file(&copy.into(), &path, rev.as_deref())?
            .into())
    })
    .await
}

#[tauri::command]
pub async fn file_diff(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    path: String,
    options: Option<DiffOptions>,
) -> CommandResult<CodeDiffDto> {
    let options = options.unwrap_or_default();
    let context = if options.full {
        MAX_DIFF_CONTEXT
    } else {
        options.context.unwrap_or(3)
    };
    blocking(&state, move |h| {
        Ok(
            h.code_diff(&copy.into(), &path, options.against.as_deref(), context)?
                .into(),
        )
    })
    .await
}

#[tauri::command]
pub async fn search_text(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    query: String,
    options: Option<SearchOptions>,
) -> CommandResult<SearchResults> {
    let options = options.unwrap_or_default();
    let query = TextQuery {
        query,
        case_sensitive: options.case_sensitive,
        whole_word: options.whole_word,
        regex: options.regex,
        glob: options.glob,
    };
    blocking(&state, move |h| h.search_code(&copy.into(), &query)).await
}

#[tauri::command]
pub async fn file_history(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    path: String,
    lines: Option<LineRange>,
) -> CommandResult<FileHistoryDto> {
    blocking(&state, move |h| {
        Ok(h.file_history(&copy.into(), &path, self::lines(lines))?
            .into())
    })
    .await
}

#[tauri::command]
pub async fn blame_file(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    path: String,
    rev: Option<String>,
    lines: Option<LineRange>,
) -> CommandResult<Blame> {
    blocking(&state, move |h| {
        h.blame_file(&copy.into(), &path, rev.as_deref(), self::lines(lines))
    })
    .await
}

#[tauri::command]
pub async fn file_symbols(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    path: String,
) -> CommandResult<Vec<Symbol>> {
    blocking(&state, move |h| h.file_symbols(&copy.into(), &path)).await
}

#[tauri::command]
pub async fn open_file_in_editor(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    path: String,
    line: Option<u32>,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.open_code_in_editor(&copy.into(), &path, line)
    })
    .await
}

#[tauri::command]
pub async fn reveal_in_file_manager(
    state: State<'_, AppState>,
    copy: CodeCopyInput,
    path: String,
) -> CommandResult<()> {
    blocking(&state, move |h| h.reveal_code_file(&copy.into(), &path)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use hedy_core::domain::attachment::ImageType;

    #[test]
    fn copies_come_tagged_by_kind() {
        let folder: CodeCopyInput = serde_json::from_value(
            serde_json::json!({ "kind": "folder", "project": "/code/hedy" }),
        )
        .unwrap();
        assert!(
            matches!(CodeCopy::from(folder), CodeCopy::Folder(p) if p == std::path::Path::new("/code/hedy"))
        );
        let task: CodeCopyInput =
            serde_json::from_value(serde_json::json!({ "kind": "task", "task": "s5" })).unwrap();
        assert!(matches!(CodeCopy::from(task), CodeCopy::Task(id) if id.as_str() == "s5"));
        let options: DiffOptions =
            serde_json::from_value(serde_json::json!({ "against": "base" })).unwrap();
        assert_eq!((options.context, options.full), (None, false));
    }

    #[test]
    fn contents_are_tagged_and_images_become_data_urls() {
        let image = FileContentDto::from(FileContent::Image {
            media_type: ImageType::Png,
            bytes: vec![1, 2, 3],
            size: 3,
        });
        let json = serde_json::to_value(&image).unwrap();
        assert_eq!(json["kind"], "image");
        assert_eq!(json["mime"], "image/png");
        assert_eq!(json["dataUrl"], "data:image/png;base64,AQID");
        let large = serde_json::to_value(FileContentDto::TooLarge { size: 9 }).unwrap();
        assert_eq!(large, serde_json::json!({ "kind": "tooLarge", "size": 9 }));
        assert_eq!(
            serde_json::to_value(FileContentDto::Locked).unwrap(),
            serde_json::json!({ "kind": "locked" })
        );
        let body = serde_json::to_value(DiffBody::TooLarge { size: 1 }).unwrap();
        assert_eq!(body, serde_json::json!({ "kind": "tooLarge", "size": 1 }));
    }
}
