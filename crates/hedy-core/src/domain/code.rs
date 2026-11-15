//! Read-only code navigation and review: what changed in a copy of the code
//! (the user's folder or a task's worktree), file contents, text search,
//! history, blame and a light outline of a file. The rules here are pure:
//! which paths and revisions are acceptable, what counts as binary, and how
//! an outline is read from a file's text.

use serde::{Deserialize, Serialize};

use super::diff::Hunk;

/// Largest text file shown (and diffed), in bytes. Bigger ones are
/// `TooLarge`: open them in the editor.
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
/// Largest image previewed, in bytes.
pub const MAX_PREVIEW_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
/// Entries of a tree at most; a bigger repository is cut (in path order).
pub const MAX_TREE_ENTRIES: usize = 100_000;
/// Matching lines a search returns at most.
pub const MAX_SEARCH_MATCHES: usize = 2_000;
/// Characters of a matching line kept for the results.
pub const MAX_MATCH_LINE_CHARS: usize = 400;
/// Commits a file's history shows at most.
pub const MAX_HISTORY_COMMITS: usize = 200;
/// Symbols an outline lists at most.
pub const MAX_SYMBOLS: usize = 2_000;
/// How many bytes git looks at to decide a file is binary (a NUL byte in
/// them); Hedy uses the same rule.
const BINARY_SNIFF_BYTES: usize = 8_000;

/// How a file differs from the copy's base.
///
/// For a task, everything is measured against the commit the task started
/// from, so a file the task created is `Added` (never `Untracked`). For the
/// user's folder, the base is `HEAD`: a new file git does not know yet is
/// `Untracked`; one already staged is `Added`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathChange {
    pub path: String,
    /// The previous path of a rename.
    pub old_path: Option<String>,
    pub kind: ChangeKind,
}

/// A file (or, for ignored folders git collapses, a folder) of a copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntry {
    /// Relative to the copy's root, `/`-separated. Ignored folders end
    /// with `/` (their contents are not listed).
    pub path: String,
    /// `None`: unchanged since the base.
    pub status: Option<ChangeKind>,
    /// The previous path of a rename.
    pub old_path: Option<String>,
    /// A protected file (`.env`, keys...): listed, never read.
    pub locked: bool,
    /// Ignored by git (only listed when asked for).
    pub ignored: bool,
}

/// The first bytes of a file and its whole size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileBytes {
    /// At most the limit asked for.
    pub bytes: Vec<u8>,
    pub size: u64,
    /// The file actually read, relative to the copy's root, after following
    /// symbolic links (the same as the path asked for when it is not one).
    pub resolved: String,
}

/// Whether bytes are binary by git's rule: a NUL byte near the start.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
}

/// A path inside a copy as the UI sends it, checked and normalized: relative,
/// `/`-separated, no `.`/`..` parts, no `.git` folder, nothing empty.
/// `None` when it would leave the copy or is not a file path at all.
pub fn normalize_code_path(path: &str) -> Option<String> {
    if path.is_empty() || path.contains('\0') || path.len() > 4096 {
        return None;
    }
    let path = path.replace('\\', "/");
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if path.starts_with('/') || path.starts_with('~') || drive {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            p if p.eq_ignore_ascii_case(".git") => return None,
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The version of a file a missing file has: an editor that saves with it
/// expects the file not to exist (it was deleted meanwhile and is written again).
pub const MISSING_VERSION: &str = "missing";

/// A file's version for optimistic concurrency: a hash of its bytes and their
/// length. An editor saves with the version it loaded; a different one on
/// disk means someone else (an agent, another editor) changed the file.
/// FNV-1a, 64 bits: not cryptographic, only a fingerprint.
pub fn content_version(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}-{}", bytes.len())
}

const BOM: &str = "\u{feff}";

/// The bytes to write for `text` (as an editor holds it, lines joined by
/// `\n`) over a file that held `original`: its line endings (CRLF when most
/// of its lines used them), its byte-order mark and its final newline are
/// kept, so saving a one-line fix does not rewrite every line.
pub fn text_for_disk(original: Option<&[u8]>, text: &str) -> Vec<u8> {
    let text = text.strip_prefix(BOM).unwrap_or(text);
    let mut text = text.replace("\r\n", "\n");
    let original = original.unwrap_or_default();
    let crlf = original.windows(2).filter(|w| w == b"\r\n").count();
    let lf = original.iter().filter(|&&b| b == b'\n').count();
    if original.ends_with(b"\n") && !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if crlf > 0 && crlf * 2 >= lf {
        text = text.replace('\n', "\r\n");
    }
    let mut bytes = Vec::with_capacity(text.len() + 3);
    if original.starts_with(BOM.as_bytes()) {
        bytes.extend_from_slice(BOM.as_bytes());
    }
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

/// Whether `name` can be the last part of a new file or folder: not empty,
/// no separators or control characters, not `.`/`..`, no trailing dot or
/// space (Windows drops them) and not too long.
pub fn is_valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.starts_with(' ')
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

/// Whether `rev` can be handed to git as a revision: a commit hash, a
/// branch or tag name, `HEAD`, with `~`/`^` suffixes. Never an option
/// (`-x`), a range or anything with spaces.
pub fn is_safe_rev(rev: &str) -> bool {
    !rev.is_empty()
        && rev.len() <= 200
        && !rev.starts_with('-')
        && !rev.contains("..")
        && !rev.contains("@{")
        && rev
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-' | '~' | '^'))
}

/// A text search inside a copy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextQuery {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// `query` is a regular expression (POSIX extended, as `git grep -E`).
    pub regex: bool,
    /// Comma-separated globs the paths must match (`*.py, src/**`); a
    /// leading `!` excludes. A glob without `/` matches in any folder.
    pub glob: Option<String>,
}

/// Where a match is in a line: UTF-16 code units of the line's text (what
/// JavaScript strings index), end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchRange {
    pub start: u32,
    pub end: u32,
}

/// A matching line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    /// 1-based.
    pub line: u32,
    /// The line, cut to [`MAX_MATCH_LINE_CHARS`].
    pub text: String,
    /// Every match in `text`; empty when it could not be located exactly.
    pub ranges: Vec<MatchRange>,
}

/// The matches in one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatches {
    pub path: String,
    pub matches: Vec<LineMatch>,
}

/// What a search found, grouped by file in path order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub files: Vec<FileMatches>,
    /// Matching lines returned.
    pub total: usize,
    /// More lines matched than [`MAX_SEARCH_MATCHES`]; refine the query.
    pub truncated: bool,
}

impl SearchResults {
    /// Matching lines, in git's order (path, then line), grouped by file.
    pub fn from_lines(lines: Vec<(String, LineMatch)>, truncated: bool) -> Self {
        let mut files: Vec<FileMatches> = Vec::new();
        let total = lines.len();
        for (path, line) in lines {
            match files.last_mut() {
                Some(file) if file.path == path => file.matches.push(line),
                _ => files.push(FileMatches {
                    path,
                    matches: vec![line],
                }),
            }
        }
        Self {
            files,
            total,
            truncated,
        }
    }
}

/// A commit in a file's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub hash: String,
    pub short: String,
    pub author: String,
    /// Author date, Unix seconds.
    pub date: i64,
    pub subject: String,
    /// The file's path in that commit (it differs after a rename).
    pub path: String,
}

/// A commit that wrote some lines of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameCommit {
    pub hash: String,
    pub short: String,
    pub author: String,
    /// Author date, Unix seconds.
    pub date: i64,
    pub summary: String,
    /// Lines not committed yet (the copy's own changes).
    pub uncommitted: bool,
}

/// Consecutive lines written by the same commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameRange {
    /// 1-based first line.
    pub start: u32,
    pub count: u32,
    /// A hash in [`Blame::commits`].
    pub commit: String,
}

/// Who wrote each line of a file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blame {
    /// Each commit once, in order of first appearance.
    pub commits: Vec<BlameCommit>,
    /// Covering every line, in order.
    pub ranges: Vec<BlameRange>,
}

impl Blame {
    /// Adds `count` lines from `start` written by `commit`, joining them to
    /// the previous range when it is the same commit and they follow it.
    pub fn push_lines(&mut self, commit: &str, start: u32, count: u32) {
        if let Some(last) = self.ranges.last_mut()
            && last.commit == commit
            && last.start + last.count == start
        {
            last.count += count;
            return;
        }
        self.ranges.push(BlameRange {
            start,
            count,
            commit: commit.to_owned(),
        });
    }
}

/// The shape of a file's diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DiffBody {
    /// Line changes (no hunks: the file is the same on both sides).
    Text {
        additions: u32,
        deletions: u32,
        hunks: Vec<Hunk>,
    },
    Binary,
    /// One side is bigger than [`MAX_TEXT_BYTES`].
    TooLarge {
        size: u64,
    },
    /// A protected file: never read.
    Locked,
}

/// What a symbol is, roughly (the outline is read line by line, without a
/// parser).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Type,
    Module,
    Constant,
    Heading,
}

/// A function, type or heading of a file, for `@` in the palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// 1-based.
    pub line: u32,
    /// Nesting by indentation (0 = top level); for headings, `#` count − 1.
    pub depth: u32,
}

/// The languages [`outline`] knows, by file extension.
fn language(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    Some(match ext.as_str() {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => "js",
        "go" => "go",
        "rb" => "ruby",
        "java" | "kt" | "kts" | "cs" | "swift" | "scala" | "dart" | "php" => "c-like",
        "md" | "mdx" | "markdown" => "markdown",
        _ => return None,
    })
}

/// The identifier at the start of `s` (letters, digits, `_`, `$`).
fn ident(s: &str) -> Option<&str> {
    let end = s
        .char_indices()
        .find(|&(_, c)| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .map_or(s.len(), |(i, _)| i);
    let id = &s[..end];
    (!id.is_empty() && !id.starts_with(|c: char| c.is_ascii_digit())).then_some(id)
}

/// `rest` after any of `prefixes` (each followed by a space), repeatedly:
/// strips modifiers such as `pub`, `export`, `async`, `static`.
fn strip_words<'a>(mut rest: &'a str, words: &[&str]) -> &'a str {
    loop {
        let before = rest;
        for word in words {
            if let Some(after) = rest.strip_prefix(word)
                && let Some(after) = after.strip_prefix(' ')
            {
                rest = after.trim_start();
            }
        }
        // `pub(crate)`, `pub(super)`.
        if let Some(after) = rest.strip_prefix("pub(")
            && let Some((_, after)) = after.split_once(") ")
        {
            rest = after.trim_start();
        }
        if rest == before {
            return rest;
        }
    }
}

/// The first keyword of `rest` from `table` followed by a name: (kind, name).
fn keyword<'a>(rest: &'a str, table: &[(&str, SymbolKind)]) -> Option<(SymbolKind, &'a str)> {
    table.iter().find_map(|&(word, kind)| {
        let after = rest.strip_prefix(word)?.strip_prefix(' ')?.trim_start();
        Some((kind, ident(after)?))
    })
}

fn rust_symbol(line: &str) -> Option<(SymbolKind, String)> {
    let rest = strip_words(
        line,
        &[
            "pub", "async", "const", "unsafe", "extern", "default", "\"C\"",
        ],
    );
    if let Some(after) = rest
        .strip_prefix("impl")
        .filter(|a| a.starts_with([' ', '<']))
    {
        // `impl Trait for Type`, `impl<T> Type<T>`.
        let after = after.trim_start();
        let after = if after.starts_with('<') {
            after.split_once('>').map_or(after, |(_, a)| a).trim_start()
        } else {
            after
        };
        let name = after.split('{').next()?.trim();
        return (!name.is_empty()).then(|| (SymbolKind::Type, format!("impl {name}")));
    }
    if let Some((kind, name)) = keyword(
        rest,
        &[
            ("fn", SymbolKind::Function),
            ("struct", SymbolKind::Struct),
            ("enum", SymbolKind::Enum),
            ("trait", SymbolKind::Trait),
            ("type", SymbolKind::Type),
            ("mod", SymbolKind::Module),
            ("macro_rules!", SymbolKind::Function),
        ],
    ) {
        return Some((kind, name.to_owned()));
    }
    // `pub const MAX: usize`, `static X: ...` (the modifiers were stripped).
    let constant = line.trim_start_matches("pub ").trim_start();
    keyword(
        constant,
        &[
            ("const", SymbolKind::Constant),
            ("static", SymbolKind::Constant),
        ],
    )
    .filter(|(_, name)| {
        name.chars()
            .all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
    })
    .map(|(kind, name)| (kind, name.to_owned()))
}

fn python_symbol(line: &str) -> Option<(SymbolKind, String)> {
    let rest = strip_words(line, &["async"]);
    keyword(
        rest,
        &[("def", SymbolKind::Function), ("class", SymbolKind::Class)],
    )
    .map(|(kind, name)| (kind, name.to_owned()))
}

fn js_symbol(line: &str) -> Option<(SymbolKind, String)> {
    let rest = strip_words(line, &["export", "default", "declare", "abstract", "async"]);
    if let Some((kind, name)) = keyword(
        rest,
        &[
            ("function*", SymbolKind::Function),
            ("function", SymbolKind::Function),
            ("class", SymbolKind::Class),
            ("interface", SymbolKind::Interface),
            ("type", SymbolKind::Type),
            ("enum", SymbolKind::Enum),
            ("namespace", SymbolKind::Module),
        ],
    ) {
        return Some((kind, name.to_owned()));
    }
    // `const name = (…) =>`, `const name = async function`.
    let (_, name) = keyword(
        rest,
        &[
            ("const", SymbolKind::Function),
            ("let", SymbolKind::Function),
            ("var", SymbolKind::Function),
        ],
    )?;
    let value = rest.split_once('=')?.1.trim_start();
    let value = value.strip_prefix("async").unwrap_or(value).trim_start();
    let function = value.starts_with("function") || value.starts_with('(') && rest.contains("=>");
    let arrow_param = ident(value).is_some_and(|p| value[p.len()..].trim_start().starts_with("=>"));
    (function || arrow_param).then(|| (SymbolKind::Function, name.to_owned()))
}

fn go_symbol(line: &str) -> Option<(SymbolKind, String)> {
    if let Some(after) = line.strip_prefix("func ") {
        let after = after.trim_start();
        // `func (s *Server) Start(`.
        if let Some(receiver) = after.strip_prefix('(') {
            let name = ident(receiver.split_once(')')?.1.trim_start())?;
            return Some((SymbolKind::Method, name.to_owned()));
        }
        return ident(after).map(|n| (SymbolKind::Function, n.to_owned()));
    }
    let after = line.strip_prefix("type ")?.trim_start();
    let name = ident(after)?;
    let kind = match after[name.len()..].trim_start() {
        r if r.starts_with("struct") => SymbolKind::Struct,
        r if r.starts_with("interface") => SymbolKind::Interface,
        _ => SymbolKind::Type,
    };
    Some((kind, name.to_owned()))
}

fn ruby_symbol(line: &str) -> Option<(SymbolKind, String)> {
    if let Some(after) = line.strip_prefix("def ") {
        let after = after.trim_start();
        let after = after.strip_prefix("self.").unwrap_or(after);
        let end = after
            .find(|c: char| c == '(' || c.is_whitespace())
            .unwrap_or(after.len());
        let name = &after[..end];
        return (!name.is_empty()).then(|| (SymbolKind::Method, name.to_owned()));
    }
    keyword(
        line,
        &[("class", SymbolKind::Class), ("module", SymbolKind::Module)],
    )
    .map(|(kind, name)| (kind, name.to_owned()))
}

fn c_like_symbol(line: &str) -> Option<(SymbolKind, String)> {
    let rest = strip_words(
        line,
        &[
            "public",
            "private",
            "protected",
            "internal",
            "static",
            "final",
            "abstract",
            "sealed",
            "open",
            "override",
            "data",
            "partial",
            "async",
            "export",
            "suspend",
            "inline",
            "readonly",
        ],
    );
    keyword(
        rest,
        &[
            ("class", SymbolKind::Class),
            ("interface", SymbolKind::Interface),
            ("enum", SymbolKind::Enum),
            ("struct", SymbolKind::Struct),
            ("record", SymbolKind::Class),
            ("object", SymbolKind::Class),
            ("trait", SymbolKind::Trait),
            ("protocol", SymbolKind::Interface),
            ("extension", SymbolKind::Type),
            ("fun", SymbolKind::Function),
            ("func", SymbolKind::Function),
            ("function", SymbolKind::Function),
            ("def", SymbolKind::Function),
        ],
    )
    .map(|(kind, name)| (kind, name.to_owned()))
}

/// The functions, types and headings of a file, read line by line from its
/// text by language (Rust, Python, JavaScript/TypeScript, Go, Ruby, the
/// C-like family through keywords, Markdown headings). It is a light
/// outline, not a parser: multi-line signatures, methods without a keyword
/// (Java, C#) and anything inside strings or comments may be missed or
/// misread. Unknown languages give an empty outline.
pub fn outline(path: &str, text: &str) -> Vec<Symbol> {
    let Some(language) = language(path) else {
        return Vec::new();
    };
    let mut symbols = Vec::new();
    let mut in_fence = false;
    for (index, raw) in text.lines().enumerate() {
        let line_no = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if language == "markdown" {
            if line.starts_with("```") || line.starts_with("~~~") {
                in_fence = !in_fence;
                continue;
            }
            let level = line.chars().take_while(|&c| c == '#').count();
            if !in_fence && (1..=6).contains(&level) && line[level..].starts_with(' ') {
                let name = line[level..].trim().trim_end_matches('#').trim();
                if !name.is_empty() {
                    symbols.push(Symbol {
                        name: name.to_owned(),
                        kind: SymbolKind::Heading,
                        line: line_no,
                        depth: u32::try_from(level - 1).unwrap_or(0),
                    });
                }
            }
        } else if !(line.starts_with("//") || line.starts_with('#') || line.starts_with('*')) {
            let found = match language {
                "rust" => rust_symbol(line),
                "python" => python_symbol(line),
                "js" => js_symbol(line),
                "go" => go_symbol(line),
                "ruby" => ruby_symbol(line),
                _ => c_like_symbol(line),
            };
            if let Some((mut kind, name)) = found {
                let depth = u32::try_from(indent / 2).unwrap_or(u32::MAX).min(8);
                if kind == SymbolKind::Function && depth > 0 && language != "go" {
                    kind = SymbolKind::Method;
                }
                symbols.push(Symbol {
                    name,
                    kind,
                    line: line_no,
                    depth,
                });
            }
        }
        if symbols.len() >= MAX_SYMBOLS {
            break;
        }
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_copy() {
        assert_eq!(
            normalize_code_path("src/app.py").as_deref(),
            Some("src/app.py")
        );
        assert_eq!(
            normalize_code_path("./src//app.py").as_deref(),
            Some("src/app.py")
        );
        assert_eq!(
            normalize_code_path("src\\win.rs").as_deref(),
            Some("src/win.rs")
        );
        for bad in [
            "",
            ".",
            "/etc/passwd",
            "../secret",
            "src/../../x",
            "~/.ssh/id_rsa",
            "C:/Windows",
            ".git/config",
            "sub/.GIT/HEAD",
            "a\0b",
        ] {
            assert_eq!(normalize_code_path(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn revisions_are_never_options_or_ranges() {
        for good in [
            "HEAD",
            "a3f9c1e",
            "main",
            "hedy/arregla-t1",
            "HEAD~2",
            "v1.0^",
        ] {
            assert!(is_safe_rev(good), "{good}");
        }
        for bad in [
            "",
            "--output=x",
            "-p",
            "a..b",
            "HEAD@{1}",
            "a b",
            "x;rm",
            "$(x)",
        ] {
            assert!(!is_safe_rev(bad), "{bad}");
        }
    }

    #[test]
    fn versions_change_with_any_byte() {
        let a = content_version(b"IVA = 0.19\n");
        assert_eq!(a, content_version(b"IVA = 0.19\n"));
        assert_ne!(a, content_version(b"IVA = 0.21\n"));
        assert!(a.ends_with("-11"));
        assert_ne!(content_version(b""), MISSING_VERSION);
    }

    #[test]
    fn saving_keeps_line_endings_bom_and_final_newline() {
        let crlf = b"a\r\nb\r\n";
        assert_eq!(text_for_disk(Some(crlf), "a\nB"), b"a\r\nB\r\n");
        assert_eq!(text_for_disk(Some(b"a\nb\n"), "a\nb\nc\n"), b"a\nb\nc\n");
        // No final newline before: none added.
        assert_eq!(text_for_disk(Some(b"a\nb"), "a\nc"), b"a\nc");
        // Mostly LF with a stray CRLF: LF.
        assert_eq!(text_for_disk(Some(b"a\nb\nc\r\nd\n"), "x\ny\n"), b"x\ny\n");
        let bom = "\u{feff}hola\n".as_bytes();
        assert_eq!(
            text_for_disk(Some(bom), "\u{feff}adiós"),
            "\u{feff}adiós\n".as_bytes()
        );
        assert_eq!(text_for_disk(None, "nuevo"), b"nuevo");
        assert_eq!(text_for_disk(Some(b"x\n"), ""), b"");
    }

    #[test]
    fn new_names_are_plain_file_names() {
        for good in ["app.py", ".gitignore", "Leeme ñ.md", "a b"] {
            assert!(is_valid_file_name(good), "{good}");
        }
        for bad in [
            "", ".", "..", "a/b", "a\\b", "x.", "x ", " x", "a:b", "a\nb", "q?",
        ] {
            assert!(!is_valid_file_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn binary_is_a_nul_byte_near_the_start() {
        assert!(!looks_binary(b"hola\nmundo"));
        assert!(looks_binary(b"\x89PNG\r\n\x1a\n\0\0"));
        let mut late = vec![b'a'; 9_000];
        late.push(0);
        assert!(!looks_binary(&late));
    }

    #[test]
    fn search_lines_are_grouped_by_file() {
        let line = |n: u32| LineMatch {
            line: n,
            text: "x".into(),
            ranges: vec![],
        };
        let results = SearchResults::from_lines(
            vec![
                ("a.py".into(), line(1)),
                ("a.py".into(), line(7)),
                ("b.py".into(), line(2)),
            ],
            true,
        );
        assert_eq!(results.total, 3);
        assert!(results.truncated);
        assert_eq!(results.files.len(), 2);
        assert_eq!(results.files[0].matches.len(), 2);
    }

    #[test]
    fn blame_joins_consecutive_lines_of_a_commit() {
        let mut blame = Blame::default();
        blame.push_lines("a", 1, 2);
        blame.push_lines("a", 3, 1);
        blame.push_lines("b", 4, 1);
        blame.push_lines("a", 5, 1);
        let ranges: Vec<_> = blame
            .ranges
            .iter()
            .map(|r| (r.start, r.count, r.commit.as_str()))
            .collect();
        assert_eq!(ranges, [(1, 3, "a"), (4, 1, "b"), (5, 1, "a")]);
    }

    fn names(path: &str, text: &str) -> Vec<(String, SymbolKind, u32, u32)> {
        outline(path, text)
            .into_iter()
            .map(|s| (s.name, s.kind, s.line, s.depth))
            .collect()
    }

    #[test]
    fn outlines_python_and_rust() {
        let py = "import os\n\nclass Factura:\n    def total(self):\n        pass\n\nasync def enviar(x):\n    # def no\n    pass\n";
        assert_eq!(
            names("facturas/impuestos.py", py),
            [
                ("Factura".into(), SymbolKind::Class, 3, 0),
                ("total".into(), SymbolKind::Method, 4, 2),
                ("enviar".into(), SymbolKind::Function, 7, 0),
            ]
        );
        let rs = "pub const MAX_X: usize = 1;\npub(crate) struct Hedy;\nimpl<T> Git for GitCli {\n    pub async fn diff(&self) {}\n}\n// fn nope()\nenum Kind {}\n";
        assert_eq!(
            names("src/lib.rs", rs),
            [
                ("MAX_X".into(), SymbolKind::Constant, 1, 0),
                ("Hedy".into(), SymbolKind::Struct, 2, 0),
                ("impl Git for GitCli".into(), SymbolKind::Type, 3, 0),
                ("diff".into(), SymbolKind::Method, 4, 2),
                ("Kind".into(), SymbolKind::Enum, 7, 0),
            ]
        );
    }

    #[test]
    fn outlines_typescript_go_and_markdown() {
        let ts = "export function total(a: number) {}\nexport const sumar = (a, b) => a + b;\nconst LIMIT = 3;\nexport default class Carrito {}\nexport interface Linea {}\ntype Id = string;\nconst doble = x => x * 2;\n";
        assert_eq!(
            names("src/carrito.ts", ts)
                .into_iter()
                .map(|(n, k, ..)| (n, k))
                .collect::<Vec<_>>(),
            [
                ("total".into(), SymbolKind::Function),
                ("sumar".into(), SymbolKind::Function),
                ("Carrito".into(), SymbolKind::Class),
                ("Linea".into(), SymbolKind::Interface),
                ("Id".into(), SymbolKind::Type),
                ("doble".into(), SymbolKind::Function),
            ]
        );
        let go = "func (s *Server) Start() error {\nfunc main() {\ntype Config struct {\n";
        assert_eq!(
            names("main.go", go)
                .into_iter()
                .map(|(n, k, ..)| (n, k))
                .collect::<Vec<_>>(),
            [
                ("Start".into(), SymbolKind::Method),
                ("main".into(), SymbolKind::Function),
                ("Config".into(), SymbolKind::Struct),
            ]
        );
        let md = "# Hedy\n\n## Instalar ##\n```sh\n# no es título\n```\n### Uso\n";
        assert_eq!(
            names("README.md", md),
            [
                ("Hedy".into(), SymbolKind::Heading, 1, 0),
                ("Instalar".into(), SymbolKind::Heading, 3, 1),
                ("Uso".into(), SymbolKind::Heading, 7, 2),
            ]
        );
        assert!(outline("datos.csv", "a,b\n").is_empty());
    }
}
