//! Read-only git for code navigation and review ([`CodeReader`]): file
//! lists, status against a base, file contents at a commit, one file's
//! diff, `git grep`, `git log --follow` and `git blame`. Nothing here
//! writes to the repository, not even the index.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use regex::RegexBuilder;

use super::git::{git, run};
use crate::application::ports::{CodeReader, PortError, PortResult};
use crate::domain::code::{
    Blame, BlameCommit, ChangeKind, CommitInfo, FileBytes, LineMatch, MAX_MATCH_LINE_CHARS,
    MatchRange, PathChange, TextQuery,
};
use crate::domain::diff::{FileDiff, parse_unified};
use crate::tr;

use super::GitCli;

/// Field and record separators in `git log --format`.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';
const LOG_FORMAT: &str = "--format=%x1e%H%x1f%h%x1f%an%x1f%at%x1f%s";

fn spawn(dir: &Path, args: &[&str]) -> PortResult<std::process::Child> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PortError::new(format!("no se pudo ejecutar git: {e}")))
}

/// The first `max` bytes git writes, then stops it.
fn capped(dir: &Path, args: &[&str], max: u64) -> PortResult<Vec<u8>> {
    let mut child = spawn(dir, args)?;
    let mut bytes = Vec::new();
    if let Some(out) = child.stdout.take() {
        out.take(max)
            .read_to_end(&mut bytes)
            .map_err(|e| PortError::new(format!("git: {e}")))?;
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(bytes)
}

fn nul_split(out: &str) -> impl Iterator<Item = &str> {
    out.split('\0').filter(|p| !p.is_empty())
}

/// `rev:path`, as git names a file in a commit.
fn object(rev: &str, path: &str) -> String {
    format!("{rev}:{path}")
}

/// Whether `rev` names a commit or tree git knows.
fn rev_exists(dir: &Path, rev: &str) -> bool {
    run(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{tree}}"),
        ],
    )
    .is_ok_and(|out| out.status.success())
}

fn unknown_rev(rev: &str) -> PortError {
    PortError::new(tr!(
        "git no conoce el commit «{rev}»",
        "git does not know commit «{rev}»"
    ))
}

/// The size of the blob `rev:path`; `None` when it is not a file there.
fn blob_size(dir: &Path, rev: &str, path: &str) -> PortResult<Option<u64>> {
    let spec = object(rev, path);
    let kind = run(dir, &["cat-file", "-t", &spec])?;
    if !kind.status.success() {
        return if rev_exists(dir, rev) {
            Ok(None)
        } else {
            Err(unknown_rev(rev))
        };
    }
    if String::from_utf8_lossy(&kind.stdout).trim() != "blob" {
        return Ok(None);
    }
    Ok(git(dir, &["cat-file", "-s", &spec])?.trim().parse().ok())
}

/// A working file, only when it stays inside `dir` after following links:
/// (canonical path, path relative to `dir`).
fn working_file(dir: &Path, path: &str) -> PortResult<Option<(std::path::PathBuf, String)>> {
    let file = dir.join(path);
    if fs::symlink_metadata(&file).is_err() {
        return Ok(None);
    }
    let io = |e: std::io::Error| PortError::new(format!("{path}: {e}"));
    let root = dir.canonicalize().map_err(io)?;
    let Ok(real) = file.canonicalize() else {
        // A link to nothing.
        return Ok(None);
    };
    let Ok(relative) = real.strip_prefix(&root) else {
        return Err(PortError::new(tr!(
            "{path} apunta fuera de la carpeta del proyecto",
            "{path} points outside the project folder"
        )));
    };
    if real.is_dir() {
        return Ok(None);
    }
    let relative = relative.to_string_lossy().replace('\\', "/");
    Ok(Some((real, relative)))
}

/// Parses `git diff --name-status -z`.
fn name_status(out: &str) -> Vec<PathChange> {
    let mut parts = nul_split(out);
    let mut changes = Vec::new();
    while let Some(status) = parts.next() {
        let letter = status.chars().next().unwrap_or('M');
        let (kind, old_path) = match letter {
            'R' | 'C' => {
                let old = parts.next().map(str::to_owned);
                (
                    if letter == 'R' {
                        ChangeKind::Renamed
                    } else {
                        ChangeKind::Added
                    },
                    old,
                )
            }
            'A' => (ChangeKind::Added, None),
            'D' => (ChangeKind::Deleted, None),
            _ => (ChangeKind::Modified, None),
        };
        let Some(path) = parts.next() else { break };
        changes.push(PathChange {
            path: path.to_owned(),
            old_path: old_path.filter(|_| kind == ChangeKind::Renamed),
            kind,
        });
    }
    changes
}

fn untracked(dir: &Path) -> PortResult<Vec<String>> {
    let out = git(dir, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    Ok(nul_split(&out).map(str::to_owned).collect())
}

fn is_tracked(dir: &Path, path: &str) -> PortResult<bool> {
    Ok(!git(dir, &["ls-files", "-z", "--", path])?.is_empty())
}

/// Byte offset → UTF-16 offset in `text`.
fn utf16_at(text: &str, byte: usize) -> u32 {
    u32::try_from(text[..byte].encode_utf16().count()).unwrap_or(u32::MAX)
}

/// The line cut to [`MAX_MATCH_LINE_CHARS`] characters.
fn cut_line(text: &str) -> &str {
    match text.char_indices().nth(MAX_MATCH_LINE_CHARS) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// The pathspecs of a search: the user's globs (a glob without `/` in any
/// folder, `!` to exclude) and what is never searched.
fn search_pathspecs(query: &TextQuery, exclude: &[&str], exclude_dirs: &[&str]) -> Vec<String> {
    let anywhere = |glob: &str| {
        if glob.contains('/') {
            glob.trim_start_matches('/').to_owned()
        } else {
            format!("**/{glob}")
        }
    };
    let mut specs = Vec::new();
    for glob in query
        .glob
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|g| !g.is_empty())
    {
        match glob.strip_prefix('!') {
            Some(glob) if !glob.trim().is_empty() => {
                specs.push(format!(":(exclude,glob){}", anywhere(glob.trim())));
            }
            Some(_) => {}
            None => specs.push(format!(":(glob){}", anywhere(glob))),
        }
    }
    specs.extend(
        exclude
            .iter()
            .map(|name| format!(":(exclude,glob)**/{name}")),
    );
    specs.extend(
        exclude_dirs
            .iter()
            .map(|dir| format!(":(exclude,literal){dir}")),
    );
    specs
}

/// Where the query matches in `text`, by Rust's regex engine (close to
/// git's for what people type); empty when it cannot be built.
fn match_ranges(matcher: Option<&regex::Regex>, text: &str) -> Vec<MatchRange> {
    let Some(matcher) = matcher else {
        return Vec::new();
    };
    matcher
        .find_iter(text)
        .filter(|m| !m.is_empty())
        .map(|m| MatchRange {
            start: utf16_at(text, m.start()),
            end: utf16_at(text, m.end()),
        })
        .collect()
}

fn matcher(query: &TextQuery) -> Option<regex::Regex> {
    let pattern = if query.regex {
        query.query.clone()
    } else {
        regex::escape(&query.query)
    };
    let pattern = if query.whole_word {
        format!(r"\b(?:{pattern})\b")
    } else {
        pattern
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!query.case_sensitive)
        .size_limit(1 << 20)
        .build()
        .ok()
}

/// Parses `git log` with [`LOG_FORMAT`] (and maybe `--name-only`).
fn parse_log(out: &str, path: &str) -> Vec<CommitInfo> {
    out.split(RECORD)
        .filter(|r| !r.trim().is_empty())
        .filter_map(|record| {
            let mut lines = record.lines();
            let header = lines.next()?;
            let fields: Vec<&str> = header.splitn(5, FIELD).collect();
            let [hash, short, author, date, subject] = fields[..] else {
                return None;
            };
            let named = lines.map(str::trim).rfind(|l| !l.is_empty());
            Some(CommitInfo {
                hash: hash.to_owned(),
                short: short.to_owned(),
                author: author.to_owned(),
                date: date.parse().unwrap_or(0),
                subject: subject.to_owned(),
                path: named.unwrap_or(path).to_owned(),
            })
        })
        .collect()
}

/// Parses `git blame --porcelain`.
fn parse_blame(out: &str) -> Blame {
    let mut blame = Blame::default();
    let mut current: Option<String> = None;
    for line in out.lines() {
        if line.starts_with('\t') {
            continue;
        }
        let mut words = line.split(' ');
        let first = words.next().unwrap_or("");
        if first.len() >= 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            let final_line = words.nth(1).and_then(|n| n.parse().ok()).unwrap_or(0);
            blame.push_lines(first, final_line, 1);
            if !blame.commits.iter().any(|c| c.hash == first) {
                blame.commits.push(BlameCommit {
                    hash: first.to_owned(),
                    short: first[..7].to_owned(),
                    author: String::new(),
                    date: 0,
                    summary: String::new(),
                    uncommitted: first.chars().all(|c| c == '0'),
                });
            }
            current = Some(first.to_owned());
            continue;
        }
        let Some(commit) = current
            .as_ref()
            .and_then(|hash| blame.commits.iter_mut().find(|c| &c.hash == hash))
        else {
            continue;
        };
        let rest = line.split_once(' ').map_or("", |(_, r)| r);
        match first {
            "author" => commit.author = rest.to_owned(),
            "author-time" => commit.date = rest.parse().unwrap_or(0),
            "summary" => commit.summary = rest.to_owned(),
            _ => {}
        }
    }
    blame
}

impl CodeReader for GitCli {
    fn files_at(&self, dir: &Path, rev: &str) -> PortResult<Vec<String>> {
        if !rev_exists(dir, rev) {
            return Err(unknown_rev(rev));
        }
        let out = git(dir, &["ls-tree", "-r", "-z", "--name-only", rev])?;
        Ok(nul_split(&out).map(str::to_owned).collect())
    }

    fn ignored_files(&self, dir: &Path) -> PortResult<Vec<String>> {
        let out = git(
            dir,
            &[
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--directory",
            ],
        )?;
        Ok(nul_split(&out).map(str::to_owned).collect())
    }

    fn changes(&self, dir: &Path, base: &str, to: Option<&str>) -> PortResult<Vec<PathChange>> {
        let mut args = vec![
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-color",
            "--no-ext-diff",
            base,
        ];
        args.extend(to);
        let mut changes = name_status(&git(dir, &args)?);
        if to.is_none() {
            changes.extend(untracked(dir)?.into_iter().map(|path| PathChange {
                path,
                old_path: None,
                kind: ChangeKind::Untracked,
            }));
        }
        Ok(changes)
    }

    fn read_file(
        &self,
        dir: &Path,
        rev: Option<&str>,
        path: &str,
        max: u64,
    ) -> PortResult<Option<FileBytes>> {
        let Some(rev) = rev else {
            let Some((real, resolved)) = working_file(dir, path)? else {
                return Ok(None);
            };
            let io = |e: std::io::Error| PortError::new(format!("{path}: {e}"));
            let file = fs::File::open(&real).map_err(io)?;
            let size = file.metadata().map_err(io)?.len();
            let mut bytes = Vec::new();
            file.take(max).read_to_end(&mut bytes).map_err(io)?;
            return Ok(Some(FileBytes {
                bytes,
                size,
                resolved,
            }));
        };
        let Some(size) = blob_size(dir, rev, path)? else {
            return Ok(None);
        };
        let bytes = capped(dir, &["cat-file", "blob", &object(rev, path)], max)?;
        Ok(Some(FileBytes {
            bytes,
            size,
            resolved: path.to_owned(),
        }))
    }

    fn file_size(&self, dir: &Path, rev: Option<&str>, path: &str) -> PortResult<Option<u64>> {
        match rev {
            Some(rev) => blob_size(dir, rev, path),
            None => Ok(working_file(dir, path)?
                .and_then(|(real, _)| fs::metadata(real).ok())
                .map(|m| m.len())),
        }
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
        let unified = format!("-U{context}");
        let patch = if to.is_none()
            && old_path.is_none()
            && working_file(dir, path)?.is_some()
            && !is_tracked(dir, path)?
        {
            // A new file git does not know yet: against nothing.
            let out = run(
                dir,
                &[
                    "diff",
                    "--no-index",
                    "--no-color",
                    "--no-ext-diff",
                    &unified,
                    "--",
                    "/dev/null",
                    path,
                ],
            )?;
            String::from_utf8_lossy(&out.stdout).into_owned()
        } else {
            let mut args = vec![
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "-M",
                &unified,
                base,
            ];
            args.extend(to);
            args.push("--");
            args.extend(old_path);
            args.push(path);
            git(dir, &args)?
        };
        let mut files = parse_unified(&patch);
        let index = files.iter().position(|f| f.path == path).unwrap_or(0);
        Ok((!files.is_empty()).then(|| files.swap_remove(index)))
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
        let mut args = vec!["grep", "-n", "-z", "-I", "--no-color"];
        if rev.is_none() {
            args.push("--untracked");
        }
        if !query.case_sensitive {
            args.push("-i");
        }
        if query.whole_word {
            args.push("-w");
        }
        args.push(if query.regex { "-E" } else { "-F" });
        args.extend(["-e", &query.query]);
        args.extend(rev);
        args.push("--");
        let specs = search_pathspecs(query, exclude, exclude_dirs);
        args.extend(specs.iter().map(String::as_str));

        let matcher = matcher(query);
        let prefix = rev.map(|r| format!("{r}:"));
        let mut child = spawn(dir, &args)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PortError::new("git grep: sin salida"))?;
        let mut reader = BufReader::new(stdout);
        let mut lines = Vec::new();
        let mut truncated = false;
        let field = |reader: &mut BufReader<_>, end: u8| -> Option<String> {
            let mut buf = Vec::new();
            match reader.read_until(end, &mut buf) {
                Ok(0) | Err(_) => None,
                Ok(_) => {
                    if buf.last() == Some(&end) {
                        buf.pop();
                    }
                    Some(String::from_utf8_lossy(&buf).into_owned())
                }
            }
        };
        while let Some(path) = field(&mut reader, 0) {
            let (Some(number), Some(text)) = (field(&mut reader, 0), field(&mut reader, b'\n'))
            else {
                break;
            };
            if lines.len() == max {
                truncated = true;
                break;
            }
            let path = match &prefix {
                Some(prefix) => path
                    .strip_prefix(prefix.as_str())
                    .unwrap_or(&path)
                    .to_owned(),
                None => path,
            };
            let text = cut_line(text.trim_end_matches('\r')).to_owned();
            lines.push((
                path,
                LineMatch {
                    line: number.parse().unwrap_or(0),
                    ranges: match_ranges(matcher.as_ref(), &text),
                    text,
                },
            ));
        }
        if truncated {
            let _ = child.kill();
        }
        let mut stderr = String::new();
        if let Some(mut err) = child.stderr.take() {
            let _ = err.read_to_string(&mut stderr);
        }
        let status = child
            .wait()
            .map_err(|e| PortError::new(format!("git grep: {e}")))?;
        // 0: found, 1: nothing found; anything else is an error (a bad
        // regular expression), unless Hedy stopped it.
        if !truncated && status.code().is_none_or(|c| c > 1) {
            return Err(PortError::new(format!("git grep: {}", stderr.trim())));
        }
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
        let limit = format!("-n{max}");
        let out = match lines {
            Some((first, last)) => {
                let range = format!("-L{first},{last}:{path}");
                let mut args = vec!["log", "-s", "--no-color", &limit, LOG_FORMAT, &range];
                args.extend(rev);
                git(dir, &args)
            }
            None => {
                let mut args = vec![
                    "log",
                    "--follow",
                    "--name-only",
                    "--no-color",
                    &limit,
                    LOG_FORMAT,
                ];
                args.extend(rev);
                args.extend(["--", path]);
                git(dir, &args)
            }
        };
        // A file that is not in that commit (new, or a repository without
        // commits) has no history yet.
        Ok(out.map(|out| parse_log(&out, path)).unwrap_or_default())
    }

    fn blame(
        &self,
        dir: &Path,
        rev: Option<&str>,
        path: &str,
        lines: Option<(u32, u32)>,
    ) -> PortResult<Blame> {
        let range = lines.map(|(first, last)| format!("-L{first},{last}"));
        let mut args = vec!["blame", "--porcelain"];
        args.extend(range.as_deref());
        args.extend(rev);
        args.extend(["--", path]);
        match git(dir, &args) {
            Ok(out) => Ok(parse_blame(&out)),
            Err(error) => {
                // A working file git does not know yet: all of it is the
                // copy's own, uncommitted.
                let Some((real, _)) = rev
                    .is_none()
                    .then(|| working_file(dir, path))
                    .transpose()?
                    .flatten()
                else {
                    return Err(error);
                };
                let text = fs::read(real).map_err(|e| PortError::new(format!("{path}: {e}")))?;
                let total = u32::try_from(text.split(|&b| b == b'\n').count()).unwrap_or(0);
                let total = if text.ends_with(b"\n") {
                    total - 1
                } else {
                    total
                };
                let (first, last) = lines.unwrap_or((1, total));
                let last = last.min(total);
                let mut blame = Blame::default();
                if total > 0 && first <= last {
                    let zero = "0".repeat(40);
                    blame.commits.push(BlameCommit {
                        hash: zero.clone(),
                        short: zero[..7].to_owned(),
                        author: String::new(),
                        date: 0,
                        summary: String::new(),
                        uncommitted: true,
                    });
                    blame.push_lines(&zero, first, last - first + 1);
                }
                Ok(blame)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::Git;
    use crate::domain::diff::{FileStatus, LineKind};

    fn sh(dir: &Path, args: &[&str]) -> String {
        git(dir, args).unwrap()
    }

    /// A repository with history: `src/app.py` written, then renamed from
    /// `app.py`; a big file, an image, a binary and an `.env`.
    fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("agenda");
        fs::create_dir_all(&repo).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "test@hedy.local"],
            &["config", "user.name", "Ana"],
            &["config", "commit.gpgsign", "false"],
        ] {
            sh(&repo, args);
        }
        fs::write(repo.join("app.py"), "def crear():\n    pass\n").unwrap();
        fs::write(repo.join(".gitignore"), "build/\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-qm", "Inicio"]);
        fs::create_dir_all(repo.join("src")).unwrap();
        sh(&repo, &["mv", "app.py", "src/app.py"]);
        sh(&repo, &["commit", "-qm", "Mover a src"]);
        fs::write(
            repo.join("src/app.py"),
            "def crear():\n    guardar()\n\ndef borrar():\n    pass\n",
        )
        .unwrap();
        fs::write(repo.join(".env"), "TOKEN=secreto\n").unwrap();
        fs::write(repo.join("logo.png"), b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
        fs::write(repo.join("data.bin"), [0u8, 1, 2, 3, 0, 9]).unwrap();
        sh(&repo, &["add", "src/app.py", "logo.png", "data.bin"]);
        sh(&repo, &["commit", "-qm", "Borrar citas"]);
        (tmp, repo)
    }

    #[test]
    fn files_status_and_ignored_entries() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let head = g.head_commit(&repo).unwrap();
        fs::write(repo.join("src/app.py"), "cambiado\n").unwrap();
        fs::write(repo.join("nuevo.py"), "x = 1\n").unwrap();
        fs::remove_file(repo.join("data.bin")).unwrap();
        fs::create_dir_all(repo.join("build")).unwrap();
        fs::write(repo.join("build/out.js"), "x").unwrap();

        let mut changes = g.changes(&repo, &head, None).unwrap();
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        let kinds: Vec<_> = changes.iter().map(|c| (c.path.as_str(), c.kind)).collect();
        assert_eq!(
            kinds,
            [
                (".env", ChangeKind::Untracked),
                ("data.bin", ChangeKind::Deleted),
                ("nuevo.py", ChangeKind::Untracked),
                ("src/app.py", ChangeKind::Modified),
            ]
        );
        // Reading the status did not stage anything.
        assert!(sh(&repo, &["diff", "--cached", "--name-only"]).is_empty());
        assert_eq!(g.ignored_files(&repo).unwrap(), ["build/"]);

        // Between two commits: the rename is found.
        let first = sh(&repo, &["rev-list", "--max-parents=0", "HEAD"]);
        let renamed = g.changes(&repo, first.trim(), Some("HEAD~1")).unwrap();
        assert_eq!(renamed[0].kind, ChangeKind::Renamed);
        assert_eq!(renamed[0].old_path.as_deref(), Some("app.py"));
        let mut at = g.files_at(&repo, "HEAD~1").unwrap();
        at.sort();
        assert_eq!(at, [".gitignore", "src/app.py"]);
        assert!(g.files_at(&repo, "no-existe").is_err());
    }

    #[test]
    fn reads_working_files_and_commits_with_a_cap() {
        let (tmp, repo) = repo();
        let g = GitCli;
        fs::write(repo.join("src/app.py"), "ahora\n").unwrap();
        let now = g
            .read_file(&repo, None, "src/app.py", 100)
            .unwrap()
            .unwrap();
        assert_eq!((now.bytes.as_slice(), now.size), (&b"ahora\n"[..], 6));
        let before = g
            .read_file(&repo, Some("HEAD~1"), "src/app.py", 100)
            .unwrap()
            .unwrap();
        assert_eq!(before.bytes, b"def crear():\n    pass\n");
        let cut = g
            .read_file(&repo, Some("HEAD"), "src/app.py", 4)
            .unwrap()
            .unwrap();
        assert_eq!((cut.bytes.as_slice(), cut.size), (&b"def "[..], 51));
        assert_eq!(
            g.file_size(&repo, Some("HEAD"), "src/app.py").unwrap(),
            Some(51)
        );
        assert_eq!(
            g.read_file(&repo, Some("HEAD"), "nada.py", 9).unwrap(),
            None
        );
        assert_eq!(g.read_file(&repo, Some("HEAD"), "src", 9).unwrap(), None);
        assert_eq!(g.read_file(&repo, None, "nada.py", 9).unwrap(), None);
        assert!(
            g.read_file(&repo, Some("no-existe"), "src/app.py", 9)
                .is_err()
        );

        // A link is followed inside the copy, never out of it.
        let outside = tmp.path().join("fuera.txt");
        fs::write(&outside, "secreto").unwrap();
        std::os::unix::fs::symlink(&outside, repo.join("fuera")).unwrap();
        assert!(g.read_file(&repo, None, "fuera", 99).is_err());
        std::os::unix::fs::symlink(".env", repo.join("config.txt")).unwrap();
        let linked = g.read_file(&repo, None, "config.txt", 99).unwrap().unwrap();
        assert_eq!(linked.resolved, ".env");
    }

    #[test]
    fn diffs_one_file_against_a_commit_new_or_renamed() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        fs::write(
            repo.join("src/app.py"),
            "def crear():\n    guardar()\n\ndef borrar():\n    eliminar()\n",
        )
        .unwrap();
        let diff = g
            .diff_file(&repo, "HEAD", None, "src/app.py", None, 1)
            .unwrap()
            .unwrap();
        assert_eq!((diff.additions, diff.deletions), (1, 1));
        let lines = &diff.hunks[0].lines;
        assert_eq!(lines.len(), 3, "one line of context");
        assert_eq!(
            (lines[2].kind, lines[2].new_line, lines[2].text.as_str()),
            (LineKind::Added, Some(5), "    eliminar()")
        );
        assert_eq!(
            g.diff_file(&repo, "HEAD", None, ".gitignore", None, 3)
                .unwrap(),
            None
        );

        fs::write(repo.join("nuevo.py"), "a\nb\n").unwrap();
        let new = g
            .diff_file(&repo, "HEAD", None, "nuevo.py", None, 3)
            .unwrap()
            .unwrap();
        assert_eq!((new.status, new.additions), (FileStatus::Added, 2));

        let first = sh(&repo, &["rev-list", "--max-parents=0", "HEAD"]);
        let renamed = g
            .diff_file(
                &repo,
                first.trim(),
                Some("HEAD~1"),
                "src/app.py",
                Some("app.py"),
                3,
            )
            .unwrap()
            .unwrap();
        assert_eq!(renamed.status, FileStatus::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("app.py"));
        assert!(
            g.diff_file(&repo, "HEAD", None, "logo.png", None, 3)
                .unwrap()
                .is_none()
        );
        fs::write(repo.join("logo.png"), b"\x89PNG\r\n\x1a\n\0\0\0\0").unwrap();
        assert!(
            g.diff_file(&repo, "HEAD", None, "logo.png", None, 3)
                .unwrap()
                .unwrap()
                .binary
        );
    }

    #[test]
    fn greps_tracked_and_untracked_files_but_never_protected_ones() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        fs::write(repo.join("notas.md"), "Hay que Guardar todo, guardar.\n").unwrap();
        fs::write(repo.join("credentials.json"), "guardar\n").unwrap();
        fs::write(repo.join(".env"), "GUARDAR=1\n").unwrap();
        let query = TextQuery {
            query: "guardar".into(),
            ..TextQuery::default()
        };
        let blocked = crate::domain::permissions::DEFAULT_BLOCKED;
        let (lines, truncated) = g
            .grep(&repo, None, &query, blocked, &[".hedy"], 10)
            .unwrap();
        assert!(!truncated);
        let found: Vec<_> = lines.iter().map(|(p, m)| (p.as_str(), m.line)).collect();
        assert_eq!(found, [("notas.md", 1), ("src/app.py", 2)]);
        let ranges = &lines[0].1.ranges;
        assert_eq!(
            ranges,
            &[
                MatchRange { start: 8, end: 15 },
                MatchRange { start: 22, end: 29 }
            ]
        );

        // Case, whole words, globs, a commit and the cap.
        let exact = TextQuery {
            query: "Guardar".into(),
            case_sensitive: true,
            ..TextQuery::default()
        };
        let (lines, _) = g.grep(&repo, None, &exact, blocked, &[], 10).unwrap();
        assert_eq!(lines.len(), 1);
        let only_py = TextQuery {
            glob: Some("*.py".into()),
            ..query.clone()
        };
        let (lines, _) = g.grep(&repo, None, &only_py, blocked, &[], 10).unwrap();
        assert_eq!(lines[0].0, "src/app.py");
        let not_md = TextQuery {
            glob: Some("!*.md".into()),
            ..query.clone()
        };
        let (lines, _) = g.grep(&repo, None, &not_md, blocked, &[], 10).unwrap();
        assert_eq!(lines.len(), 1);
        let (lines, _) = g
            .grep(&repo, Some("HEAD~1"), &query, blocked, &[], 10)
            .unwrap();
        assert!(lines.is_empty(), "nothing saved back then");
        let regex = TextQuery {
            query: "def (crear|borrar)".into(),
            regex: true,
            ..TextQuery::default()
        };
        let (lines, _) = g
            .grep(&repo, Some("HEAD"), &regex, blocked, &[], 10)
            .unwrap();
        assert_eq!(
            lines.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            ["src/app.py", "src/app.py"]
        );
        assert_eq!(lines[1].1.ranges, [MatchRange { start: 0, end: 10 }]);
        let (lines, truncated) = g.grep(&repo, None, &query, blocked, &[], 1).unwrap();
        assert_eq!((lines.len(), truncated), (1, true));
        let word = TextQuery {
            query: "guarda".into(),
            whole_word: true,
            ..TextQuery::default()
        };
        assert!(
            g.grep(&repo, None, &word, blocked, &[], 10)
                .unwrap()
                .0
                .is_empty()
        );
        let bad = TextQuery {
            query: "(".into(),
            regex: true,
            ..TextQuery::default()
        };
        assert!(g.grep(&repo, None, &bad, blocked, &[], 10).is_err());
    }

    #[test]
    fn history_follows_renames_and_line_ranges() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let log = g.file_log(&repo, None, "src/app.py", None, 50).unwrap();
        let subjects: Vec<_> = log.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["Borrar citas", "Mover a src", "Inicio"]);
        assert_eq!(log[2].path, "app.py");
        assert_eq!(log[0].author, "Ana");
        assert_eq!(log[0].short.len(), 7);
        assert!(log[0].date > 1_600_000_000);
        // Only the commit that wrote line 4 (`def borrar`).
        let range = g
            .file_log(&repo, None, "src/app.py", Some((4, 4)), 50)
            .unwrap();
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].subject, "Borrar citas");
        assert!(
            g.file_log(&repo, None, "nuevo.py", None, 50)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn blame_by_commit_with_uncommitted_lines() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        fs::write(
            repo.join("src/app.py"),
            "def crear():\n    guardar()\n\ndef borrar():\n    pass\nnueva = 1\n",
        )
        .unwrap();
        let blame = g.blame(&repo, None, "src/app.py", None).unwrap();
        let ranges: Vec<_> = blame.ranges.iter().map(|r| (r.start, r.count)).collect();
        assert_eq!(ranges, [(1, 1), (2, 3), (5, 1), (6, 1)]);
        let first = blame
            .commits
            .iter()
            .find(|c| c.hash == blame.ranges[0].commit)
            .unwrap();
        assert_eq!(
            (first.summary.as_str(), first.author.as_str()),
            ("Inicio", "Ana")
        );
        assert!(blame.commits.iter().any(|c| c.uncommitted));
        let at = g
            .blame(&repo, Some("HEAD"), "src/app.py", Some((2, 3)))
            .unwrap();
        assert_eq!(at.ranges.len(), 1);
        assert_eq!((at.ranges[0].start, at.ranges[0].count), (2, 2));

        fs::write(repo.join("nuevo.py"), "a\nb\n").unwrap();
        let new = g.blame(&repo, None, "nuevo.py", None).unwrap();
        assert_eq!((new.ranges[0].start, new.ranges[0].count), (1, 2));
        assert!(new.commits[0].uncommitted);
        assert!(g.blame(&repo, Some("HEAD"), "nuevo.py", None).is_err());
    }

    #[test]
    fn a_task_worktree_is_read_against_its_base() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let wt = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &wt, "hedy/t1", &base).unwrap();
        fs::write(wt.join("src/app.py"), "tarea\n").unwrap();
        fs::write(wt.join("test_app.py"), "def test():\n    pass\n").unwrap();
        let mut changes = g.changes(&wt, &base, None).unwrap();
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        let kinds: Vec<_> = changes.iter().map(|c| (c.path.as_str(), c.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("src/app.py", ChangeKind::Modified),
                ("test_app.py", ChangeKind::Untracked)
            ]
        );
        let query = TextQuery {
            query: "def test".into(),
            ..TextQuery::default()
        };
        let (lines, _) = g.grep(&wt, None, &query, &[], &[], 10).unwrap();
        assert_eq!(lines[0].0, "test_app.py");
        // The main checkout does not see the task's files.
        assert!(
            g.grep(&repo, None, &query, &[], &[".hedy"], 10)
                .unwrap()
                .0
                .is_empty()
        );
    }
}
