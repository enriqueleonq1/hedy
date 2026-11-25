//! Git through its CLI: worktrees, diffs, commits and merges are simpler and
//! more faithful this way than through a library.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::application::ports::{Git, MergeOutcome, PortError, PortResult};
use crate::domain::diff::{Commit, DiffLine, FileDiff, FileStatus, Hunk, LineKind, parse_unified};
use crate::domain::permissions::is_secret_path;
use crate::tr;

pub struct GitCli;

/// The task copy `dir` is in (`<project>/.hedy/worktrees/<id>`), if any.
fn task_copy(dir: &Path) -> Option<(&Path, &Path)> {
    dir.ancestors().find_map(|copy| {
        let worktrees = copy.parent()?;
        let hedy = worktrees.parent()?;
        (worktrees.file_name()? == "worktrees" && hedy.file_name()? == ".hedy")
            .then_some((copy, hedy.parent()?))
    })
}

/// The copy's own folder in the project's git data, found from the project
/// (which the copy cannot write) and never from the copy's `.git`.
fn copy_git_dir(copy: &Path, project: &Path) -> Option<PathBuf> {
    let common = plain(project, &["rev-parse", "--git-common-dir"]).ok()?;
    if !common.status.success() {
        return None;
    }
    let common = project.join(String::from_utf8_lossy(&common.stdout).trim());
    let copy = copy.canonicalize().ok()?;
    fs::read_dir(common.join("worktrees"))
        .ok()?
        .flatten()
        .find_map(|entry| {
            let recorded = fs::read_to_string(entry.path().join("gitdir")).ok()?;
            // Relative to the entry with `worktree.useRelativePaths` (git 2.48+).
            let recorded = entry.path().join(recorded.trim());
            let recorded = recorded.parent()?.canonicalize().ok()?;
            (recorded == copy).then(|| entry.path())
        })
}

/// Where git looks when a copy has no real git data: a path that cannot
/// exist (`/dev/null` is no folder), so git fails.
const NO_GIT_DIR: &str = "/dev/null/hedy-no-git";

/// Git in a task copy uses the copy's real git data whatever its `.git`
/// says: anything that writes in the copy (the agent, the trial's check) can
/// point it at a folder of its own, whose config git would obey (a
/// `core.fsmonitor` command runs on every `git diff`). With no real data,
/// git finds none and fails. A repository nested in the copy is never
/// entered (git would obey its config too), no fsmonitor runs, and what the
/// environment says about other git data (a hook's `GIT_INDEX_FILE`…) is
/// dropped.
pub(super) fn pin_task_copy(command: &mut Command, dir: &Path) {
    let Some((copy, project)) = task_copy(dir) else {
        return;
    };
    let git_dir = copy_git_dir(copy, project).unwrap_or_else(|| PathBuf::from(NO_GIT_DIR));
    super::sync_git::clear_repo_redirects(command);
    command
        .env("GIT_DIR", git_dir)
        .env("GIT_WORK_TREE", copy)
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "diff.ignoreSubmodules=all",
            "-c",
            "status.submoduleSummary=false",
            "-c",
            "submodule.recurse=false",
        ]);
}

/// `git -C dir`, held to a task copy's real git data (see [`pin_task_copy`]).
pub(super) fn git_in(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).env("GIT_TERMINAL_PROMPT", "0");
    pin_task_copy(&mut command, dir);
    command
}

/// Diff options that keep its format Hedy's whatever the user's config:
/// no external diff or textconv (a decrypting driver, a command of the
/// config), and the usual `a/` `b/` prefixes (`diff.noprefix`,
/// `diff.mnemonicPrefix`).
const DIFF_OPTIONS: &[&str] = &[
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "-M",
];
/// Most new, not yet added files read for the uncommitted work.
const UNTRACKED_MAX: usize = 200;
/// Larger new files are named only, as binary ones.
const UNTRACKED_BYTES_MAX: u64 = 512 * 1024;

/// The empty tree (what a repository with no commit, or a first commit, is
/// compared to), in the repository's own hash.
fn empty_tree(repo: &Path) -> PortResult<String> {
    Ok(git(repo, &["hash-object", "-t", "tree", "/dev/null"])?
        .trim()
        .to_owned())
}

/// A file with no other name (a hard link may be a protected file).
#[cfg(unix)]
fn single_link(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    meta.nlink() == 1
}

/// Elsewhere the links of a file are not known: it is only named.
#[cfg(not(unix))]
fn single_link(_: &fs::Metadata) -> bool {
    false
}

fn diff_args<'a>(before: &[&'a str], after: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["diff"];
    args.extend_from_slice(before);
    args.extend_from_slice(DIFF_OPTIONS);
    args.extend_from_slice(after);
    args
}

/// The new files `.gitattributes` keeps from showing: a filter (git-crypt
/// encrypts what is added; on disk it is plain) or no diff (`-diff`,
/// `binary`). Git would show them as binary; read from disk they would not
/// be. If git cannot say, every one is kept from showing.
fn attribute_hidden(repo: &Path, paths: &[&str]) -> std::collections::HashSet<String> {
    use std::process::Stdio;
    let all = || paths.iter().map(|p| (*p).to_owned()).collect();
    if paths.is_empty() {
        return std::collections::HashSet::new();
    }
    let child = git_in(repo)
        .args(["check-attr", "-z", "--stdin", "filter", "diff"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return all();
    };
    let input = paths.iter().map(|p| format!("{p}\0")).collect::<String>();
    // Written from another thread: a long list fills the pipes both ways.
    let stdin = child.stdin.take();
    let writer = std::thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(input.as_bytes());
        }
    });
    let out = child.wait_with_output();
    let _ = writer.join();
    let Some(out) = out.ok().filter(|o| o.status.success()) else {
        return all();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let fields: Vec<&str> = text.split('\0').collect();
    fields
        .chunks(3)
        .filter(|f| match f {
            [_, "filter", value] => !matches!(*value, "unspecified" | "unset"),
            [_, "diff", value] => *value == "unset",
            _ => false,
        })
        .map(|f| f[0].to_owned())
        .collect()
}

/// A new file nobody added yet, as a diff: every line added. A protected
/// one is never read; a binary or large one, or one `.gitattributes` keeps
/// from showing, only named.
fn untracked_file(repo: &Path, path: &str, hidden: bool) -> FileDiff {
    let mut file = FileDiff {
        path: path.to_owned(),
        old_path: None,
        status: FileStatus::Added,
        additions: 0,
        deletions: 0,
        binary: false,
        hunks: Vec::new(),
    };
    if is_secret_path(path) {
        return file;
    }
    if hidden {
        file.binary = true;
        return file;
    }
    let full = repo.join(path);
    // Never through a link: a symbolic one is not a file, and a hard one
    // may be a protected file under another name.
    let small = fs::symlink_metadata(&full)
        .is_ok_and(|m| m.is_file() && m.len() <= UNTRACKED_BYTES_MAX && single_link(&m));
    let text = small
        .then(|| fs::read(&full).ok())
        .flatten()
        .filter(|bytes| !bytes.contains(&0))
        .and_then(|bytes| String::from_utf8(bytes).ok());
    let Some(text) = text else {
        file.binary = true;
        return file;
    };
    let lines: Vec<DiffLine> = text
        .lines()
        .enumerate()
        .map(|(i, line)| DiffLine {
            kind: LineKind::Added,
            old_line: None,
            new_line: Some(i as u32 + 1),
            text: line.to_owned(),
        })
        .collect();
    file.additions = lines.len() as u32;
    if !lines.is_empty() {
        file.hunks.push(Hunk {
            header: format!("@@ -0,0 +1,{} @@", lines.len()),
            lines,
        });
    }
    file
}

pub(super) fn run(dir: &Path, args: &[&str]) -> PortResult<Output> {
    if task_copy(dir).is_none() {
        return plain(dir, args);
    }
    git_in(dir)
        .args(args)
        .output()
        .map_err(|e| PortError::new(format!("no se pudo ejecutar git: {e}")))
}

/// Git as the folder's `.git` says, for folders that are not task copies
/// (and for finding a copy's real git data).
fn plain(dir: &Path, args: &[&str]) -> PortResult<Output> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| PortError::new(format!("no se pudo ejecutar git: {e}")))
}

/// Stages every change in a task's copy as `git add --all` would, but never
/// goes into a repository nested in it: `add` runs a nested repository's own
/// status, and with it whatever its config says (a clean filter). Its
/// folder is staged as git sees it from outside, or not at all.
fn stage_all(worktree: &Path) -> PortResult<()> {
    use std::process::Stdio;
    let listed = git(
        worktree,
        &[
            "ls-files",
            "-z",
            "--others",
            "--modified",
            "--deleted",
            "--exclude-standard",
        ],
    )?;
    if listed.is_empty() {
        return Ok(());
    }
    let mut child = git_in(worktree)
        .args([
            "update-index",
            "--add",
            "--remove",
            "--ignore-submodules",
            "-z",
            "--stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PortError::new(format!("no se pudo ejecutar git: {e}")))?;
    // Written from another thread: a long list fills the pipes both ways.
    let stdin = child.stdin.take();
    let writer = std::thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(listed.as_bytes());
        }
    });
    let out = child
        .wait_with_output()
        .map_err(|e| PortError::new(format!("git update-index: {e}")))?;
    let _ = writer.join();
    if out.status.success() {
        Ok(())
    } else {
        Err(PortError::new(format!(
            "git update-index: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Runs git and returns stdout, or an error with git's own message.
pub(super) fn git(dir: &Path, args: &[&str]) -> PortResult<String> {
    let out = run(dir, args)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(PortError::new(format!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

impl Git for GitCli {
    fn current_branch(&self, repo: &Path) -> PortResult<String> {
        git(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])
            .map(|s| s.trim().to_owned())
            .map_err(|_| {
                PortError::new(tr!(
                    "el repositorio no está en una rama (HEAD separado)",
                    "the repository is not on a branch (detached HEAD)"
                ))
            })
    }

    fn head_commit(&self, repo: &Path) -> PortResult<String> {
        git(repo, &["rev-parse", "--verify", "HEAD"])
            .map(|s| s.trim().to_owned())
            .map_err(|_| {
                PortError::new(tr!(
                    "el repositorio no tiene commits todavía; haz el primero",
                    "the repository has no commits yet; make the first one"
                ))
            })
    }

    fn ensure_excluded(&self, repo: &Path, pattern: &str) -> PortResult<()> {
        let common = PathBuf::from(git(repo, &["rev-parse", "--git-common-dir"])?.trim());
        let common = if common.is_absolute() {
            common
        } else {
            repo.join(common)
        };
        let info = common.join("info");
        let exclude = info.join("exclude");
        let current = fs::read_to_string(&exclude).unwrap_or_default();
        if current.lines().any(|l| l.trim() == pattern) {
            return Ok(());
        }
        let io = |e: std::io::Error| {
            PortError::new(format!("no se pudo escribir {}: {e}", exclude.display()))
        };
        fs::create_dir_all(&info).map_err(io)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&exclude)
            .map_err(io)?;
        let sep = if current.is_empty() || current.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        writeln!(file, "{sep}# Hedy: carpetas de trabajo locales\n{pattern}").map_err(io)
    }

    fn add_worktree(&self, repo: &Path, path: &Path, branch: &str, base: &str) -> PortResult<()> {
        let path = path.to_string_lossy();
        git(repo, &["worktree", "add", "-b", branch, &path, base]).map(drop)
    }

    fn add_detached_worktree(&self, repo: &Path, path: &Path, base: &str) -> PortResult<()> {
        let path = path.to_string_lossy();
        git(repo, &["worktree", "add", "--detach", &path, base]).map(drop)
    }

    fn remove_detached_worktree(&self, repo: &Path, path: &Path) -> PortResult<()> {
        if path.exists() {
            let removed = git(
                repo,
                &["worktree", "remove", "--force", &path.to_string_lossy()],
            );
            // A copy whose `.git` no longer points back (something in it
            // rewrote it) goes anyway; `remove_dir_all` follows no link.
            if let Err(e) = removed {
                if task_copy(path).is_none_or(|(copy, _)| copy != path) {
                    return Err(e);
                }
                fs::remove_dir_all(path)
                    .map_err(|e| PortError::new(format!("{}: {e}", path.display())))?;
            }
        }
        let _ = git(repo, &["worktree", "prune"]);
        Ok(())
    }

    fn is_ignored(&self, repo: &Path, path: &str) -> bool {
        run(repo, &["check-ignore", "-q", path]).is_ok_and(|out| out.status.success())
    }

    fn diff(&self, worktree: &Path, base: &str) -> PortResult<Vec<FileDiff>> {
        // Staging inside the task's own worktree makes new files part of the diff.
        stage_all(worktree)?;
        let patch = git(worktree, &diff_args(&["--cached"], &[base]))?;
        Ok(parse_unified(&patch))
    }

    fn diff_base(
        &self,
        worktree: &Path,
        base_commit: &str,
        base_branch: &str,
    ) -> PortResult<String> {
        // The branch itself: a tag the AI makes with its name never stands in for it.
        let branch = format!("refs/heads/{base_branch}");
        let Ok(common) = git(worktree, &["merge-base", "HEAD", &branch]) else {
            return Ok(base_commit.to_owned());
        };
        let common = common.trim();
        // Only a newer point than where it started: the base branch brought in.
        let newer = common != base_commit
            && run(
                worktree,
                &["merge-base", "--is-ancestor", base_commit, common],
            )?
            .status
            .success();
        Ok(if newer { common } else { base_commit }.to_owned())
    }

    fn branch_off(&self, worktree: &Path, branch: &str) -> PortResult<Option<String>> {
        Ok(
            match git(worktree, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
                Ok(on) if on.trim() == branch => None,
                Ok(on) => Some(on.trim().to_owned()),
                Err(_) => Some("HEAD".to_owned()),
            },
        )
    }

    fn commit_all(&self, worktree: &Path, message: &str) -> PortResult<bool> {
        stage_all(worktree)?;
        let staged = run(worktree, &["diff", "--cached", "--quiet"])?;
        if staged.status.success() {
            return Ok(false);
        }
        git(worktree, &["commit", "--quiet", "-m", message])?;
        Ok(true)
    }

    fn merge(&self, repo: &Path, base_branch: &str, branch: &str) -> PortResult<MergeOutcome> {
        let current = self.current_branch(repo).unwrap_or_default();
        if current != base_branch {
            return Ok(MergeOutcome::NotOnBase { current });
        }
        let status = git(repo, &["status", "--porcelain", "--untracked-files=no"])?;
        if !status.trim().is_empty() {
            return Ok(MergeOutcome::Dirty);
        }
        let merged = run(repo, &["merge", "--no-edit", branch])?;
        if merged.status.success() {
            return Ok(MergeOutcome::Merged);
        }
        // Only unmerged paths are a conflict; anything else (e.g. untracked
        // files that would be overwritten) is git's own message.
        let conflicted = !git(repo, &["ls-files", "--unmerged"])?.trim().is_empty();
        if conflicted {
            let _ = run(repo, &["merge", "--abort"]);
            return Ok(MergeOutcome::Conflict);
        }
        Err(PortError::new(format!(
            "git merge: {}",
            String::from_utf8_lossy(&merged.stderr).trim()
        )))
    }

    fn remove_worktree(&self, repo: &Path, path: &Path, branch: &str) -> PortResult<()> {
        self.remove_detached_worktree(repo, path)?;
        git(repo, &["branch", "-D", branch]).map(drop)
    }

    fn uncommitted_changes(&self, repo: &Path) -> PortResult<usize> {
        Ok(git(repo, &["status", "--porcelain"])?.lines().count())
    }

    fn rev_parse(&self, repo: &Path, rev: &str) -> PortResult<String> {
        let spec = format!("{rev}^{{commit}}");
        git(repo, &["rev-parse", "--verify", "--quiet", &spec]).map(|s| s.trim().to_owned())
    }

    fn remote_branches_containing(&self, repo: &Path, commit: &str) -> PortResult<Vec<String>> {
        let out = git(
            repo,
            &[
                "branch",
                "--remotes",
                "--contains",
                commit,
                "--format=%(refname:short)",
            ],
        )?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }

    fn reset_keep(&self, repo: &Path, commit: &str) -> PortResult<()> {
        git(repo, &["reset", "--quiet", "--keep", commit]).map(drop)
    }

    fn init_repo(&self, dir: &Path, message: &str) -> PortResult<()> {
        git(dir, &["init", "--quiet"])?;
        let out = run(dir, &["commit", "--quiet", "--allow-empty", "-m", message])?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("user.email") || stderr.contains("user.name") {
            return Err(PortError::new(tr!(
                "git no sabe tu nombre ni tu correo; configúralos con `git config --global user.name \"Tu nombre\"` y `git config --global user.email tu@correo`",
                "git does not know your name and email; set them with `git config --global user.name \"Your Name\"` and `git config --global user.email you@example.com`"
            )));
        }
        Err(PortError::new(format!("git commit: {}", stderr.trim())))
    }

    fn ignored_env_files(&self, repo: &Path) -> PortResult<Vec<String>> {
        // `--directory`: an ignored folder (node_modules) is one entry, not
        // walked.
        let out = git(
            repo,
            &[
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--directory",
            ],
        )?;
        Ok(out
            .split('\0')
            .filter(|p| !p.is_empty() && !p.ends_with('/') && !p.starts_with(".hedy/"))
            .filter(|p| {
                let name = p.rsplit('/').next().unwrap_or(p);
                name == ".env" || name.starts_with(".env.")
            })
            .map(str::to_owned)
            .collect())
    }

    fn list_files(&self, repo: &Path) -> PortResult<Vec<String>> {
        // `-z`: names as they are, without quoting unusual characters.
        let out = git(
            repo,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?;
        Ok(out
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect())
    }

    fn recent_commits(&self, repo: &Path, limit: usize) -> PortResult<Vec<Commit>> {
        let out =
            git(repo, &["log", &format!("-n{limit}"), "--format=%h%x1f%s"]).unwrap_or_default();
        Ok(out
            .lines()
            .filter_map(|l| l.split_once('\u{1f}'))
            .map(|(hash, subject)| Commit {
                hash: hash.to_owned(),
                subject: subject.to_owned(),
            })
            .collect())
    }

    fn staged_diff(&self, repo: &Path) -> PortResult<Vec<FileDiff>> {
        let patch = git(repo, &diff_args(&["--cached"], &[]))?;
        Ok(parse_unified(&patch))
    }

    fn work_diff(&self, repo: &Path) -> PortResult<(Vec<FileDiff>, usize)> {
        let base = if run(repo, &["rev-parse", "--verify", "-q", "HEAD"])?
            .status
            .success()
        {
            "HEAD".to_owned()
        } else {
            empty_tree(repo)?
        };
        let patch = git(repo, &diff_args(&[], &[&base, "--"]))?;
        let mut files = parse_unified(&patch);
        let others = git(
            repo,
            &[
                "-c",
                "core.quotePath=false",
                "ls-files",
                "-z",
                "--others",
                "--exclude-standard",
            ],
        )?;
        let new: Vec<&str> = others.split('\0').filter(|p| !p.is_empty()).collect();
        let left = new.len().saturating_sub(UNTRACKED_MAX);
        let read = &new[..new.len() - left];
        let hidden = attribute_hidden(repo, read);
        files.extend(
            read.iter()
                .map(|path| untracked_file(repo, path, hidden.contains(*path))),
        );
        Ok((files, left))
    }

    fn revs_diff(&self, repo: &Path, revs: &str) -> PortResult<Vec<FileDiff>> {
        // Never read as an option (`--output=…` writes a file), nor as a
        // path (`--` after it), nor as a file's contents (`HEAD:.env`).
        if revs.starts_with('-') || revs.contains(':') {
            return Err(PortError::new(format!(
                "git diff: «{revs}» no es una revisión"
            )));
        }
        // git's own message would be in the system's language, with a hint
        // about paths that does not apply here.
        let unknown = |_: PortError| {
            PortError::new(tr!(
                "«{revs}» no es un commit ni un rango de commits de este repositorio",
                "«{revs}» is not a commit or a range of commits of this repository"
            ))
        };
        if revs.contains("..") {
            // Both ends commits: two blobs (`<hash of .env>..<hash>`) would
            // compare a file's contents under no name.
            let (from, to) = revs
                .split_once("...")
                .or_else(|| revs.split_once(".."))
                .unwrap_or((revs, ""));
            for end in [from, to].into_iter().filter(|e| !e.is_empty()) {
                let commit = format!("{end}^{{commit}}");
                let found = run(
                    repo,
                    &["rev-parse", "--verify", "-q", "--end-of-options", &commit],
                )?;
                if !found.status.success() {
                    return Err(unknown(PortError::new(String::new())));
                }
            }
            let patch = git(repo, &diff_args(&[], &[revs, "--"])).map_err(unknown)?;
            return Ok(parse_unified(&patch));
        }
        // One commit: its own change, against its first parent (a merge's
        // too), or against nothing for a first commit.
        let commit = revs.strip_suffix("^!").unwrap_or(revs);
        let line = git(
            repo,
            &[
                "rev-list",
                "--parents",
                "-n",
                "1",
                "--end-of-options",
                commit,
            ],
        )
        .map_err(unknown)?;
        let mut ids = line.split_whitespace();
        let id = ids
            .next()
            .ok_or_else(|| unknown(PortError::new(String::new())))?
            .to_owned();
        let parent = match ids.next() {
            Some(parent) => parent.to_owned(),
            None => empty_tree(repo)?,
        };
        let patch = git(repo, &diff_args(&[], &[&parent, &id, "--"]))?;
        Ok(parse_unified(&patch))
    }

    fn main_root(&self, repo: &Path) -> PortResult<PathBuf> {
        let dirs = git(
            repo,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-dir",
                "--git-common-dir",
            ],
        )?;
        let mut dirs = dirs.lines().map(|d| PathBuf::from(d.trim()));
        let (Some(own), Some(common)) = (dirs.next(), dirs.next()) else {
            return Err(PortError::new("git rev-parse: sin carpeta de git"));
        };
        // Not a worktree added from another checkout: it is the main one.
        if own == common {
            return Ok(repo.to_owned());
        }
        if common.file_name().is_some_and(|name| name == ".git")
            && let Some(main) = common.parent()
        {
            return Ok(main.to_owned());
        }
        // A submodule's or a separate git folder's: where its config says,
        // else refused, never guessed (the projects inside depend on it).
        let configured = run(
            repo,
            &[
                "--git-dir",
                &common.to_string_lossy(),
                "config",
                "--path",
                "core.worktree",
            ],
        )?;
        let main = String::from_utf8_lossy(&configured.stdout)
            .trim()
            .to_owned();
        if configured.status.success() && !main.is_empty() {
            let main = common.join(main);
            return fs::canonicalize(&main)
                .map_err(|e| PortError::new(format!("{}: {e}", main.display())));
        }
        Err(PortError::new(tr!(
            "no se encontró la carpeta principal de este worktree; revísalo desde ella",
            "the main folder of this worktree was not found; check it from there"
        )))
    }

    fn commit_staged(&self, repo: &Path, message: &str, edit: bool) -> PortResult<()> {
        // In the repository's own git folder, so nothing shows up as a change.
        let file = git(repo, &["rev-parse", "--git-path", "HEDY_COMMIT_MSG"])?;
        let file = repo.join(file.trim());
        fs::write(&file, message)
            .map_err(|e| PortError::new(format!("{}: {e}", file.display())))?;
        let file_arg = file.to_string_lossy().into_owned();
        let mut args = vec!["commit", "-F", file_arg.as_str()];
        if edit {
            args.push("--edit");
        }
        // The terminal stays git's: hooks print, the editor and a signing
        // passphrase prompt can ask.
        let status = git_in(repo)
            .env_remove("GIT_TERMINAL_PROMPT")
            .args(&args)
            .status();
        let _ = fs::remove_file(&file);
        let status = status.map_err(|e| PortError::new(format!("no se pudo ejecutar git: {e}")))?;
        if status.success() {
            return Ok(());
        }
        let code = status.code().map_or("?".into(), |c| c.to_string());
        Err(PortError::new(tr!(
            "git commit terminó con código {code}; no se hizo el commit",
            "git commit ended with code {code}; nothing was committed"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::FileStatus;

    fn repo() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("agenda");
        fs::create_dir_all(&repo).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "test@hedy.local"],
            &["config", "user.name", "Test"],
            &["config", "commit.gpgsign", "false"],
        ] {
            git(&repo, args).unwrap();
        }
        fs::write(repo.join("app.py"), "def crear():\n    pass\n").unwrap();
        git(&repo, &["add", "."]).unwrap();
        git(&repo, &["commit", "-qm", "Inicio"]).unwrap();
        (tmp, repo)
    }

    #[test]
    fn a_task_copy_whose_git_file_points_elsewhere_runs_nothing_of_it() {
        use crate::application::ports::CodeReader;
        let (tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let copy = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &copy, "hedy/t1", &base).unwrap();
        fs::write(copy.join("app.py"), "def crear():\n    return 1\n").unwrap();
        // What the run (or its check) can write: a git folder of its own,
        // sharing the real objects, whose config runs a command on `diff`.
        let ran = tmp.path().join("ran");
        let evil = copy.join("evil");
        fs::create_dir_all(evil.join("objects/info")).unwrap();
        fs::create_dir_all(evil.join("refs/heads")).unwrap();
        fs::write(evil.join("HEAD"), "ref: refs/heads/hedy/t1\n").unwrap();
        fs::create_dir_all(evil.join("refs/heads/hedy")).unwrap();
        fs::write(evil.join("refs/heads/hedy/t1"), format!("{base}\n")).unwrap();
        let objects = repo.join(".git/objects");
        fs::write(
            evil.join("objects/info/alternates"),
            format!("{}\n", objects.display()),
        )
        .unwrap();
        let fsmonitor = format!("touch {}", ran.display());
        fs::write(
            evil.join("config"),
            format!(
                "[core]\n\trepositoryformatversion = 0\n\tworktree = {}\n\tfsmonitor = \"{fsmonitor}\"\n",
                copy.display()
            ),
        )
        .unwrap();
        fs::write(copy.join(".git"), format!("gitdir: {}\n", evil.display())).unwrap();

        let changes = g.changes(&copy, &base, None).unwrap();
        assert!(changes.iter().any(|c| c.path == "app.py"), "{changes:?}");
        let diff = g.diff(&copy, &base).unwrap();
        assert!(diff.iter().any(|f| f.path == "app.py"));
        assert!(!ran.exists(), "the copy's own git config ran a command");
        // And it goes, although git no longer knows it as its worktree.
        g.remove_worktree(&repo, &copy, "hedy/t1").unwrap();
        assert!(!copy.exists());
        assert!(!ran.exists());
    }

    /// A git folder that runs `touch <ran>` from `core.fsmonitor`, sharing
    /// `repo`'s objects, at `dir` (for the work tree `work`).
    fn plant_git_dir(dir: &Path, work: &Path, repo: &Path, ran: &Path) {
        let base = git(repo, &["rev-parse", "HEAD"]).unwrap();
        fs::create_dir_all(dir.join("objects/info")).unwrap();
        fs::create_dir_all(dir.join("refs/heads")).unwrap();
        fs::write(dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(dir.join("refs/heads/main"), base).unwrap();
        let objects = repo.join(".git/objects");
        fs::write(
            dir.join("objects/info/alternates"),
            format!("{}\n", objects.display()),
        )
        .unwrap();
        let config = format!(
            "[core]\n\trepositoryformatversion = 0\n\tworktree = {}\n\tfsmonitor = \"touch {}\"\n",
            work.display(),
            ran.display()
        );
        fs::write(dir.join("config"), config).unwrap();
    }

    #[test]
    fn ignored_env_files_are_found_outside_ignored_folders() {
        let (_tmp, repo) = repo();
        fs::write(repo.join(".gitignore"), ".env*\nnode_modules/\n.hedy/\n").unwrap();
        fs::write(repo.join(".env.example"), "KEY=\n").unwrap();
        git(&repo, &["add", "-f", ".gitignore", ".env.example"]).unwrap();
        git(&repo, &["commit", "-qm", "ignorar"]).unwrap();
        for file in [
            ".env",
            "apps/web/.env.local",
            "node_modules/x/.env",
            ".hedy/worktrees/t1/.env",
        ] {
            let path = repo.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "KEY=1\n").unwrap();
        }
        assert_eq!(
            GitCli.ignored_env_files(&repo).unwrap(),
            vec![".env".to_owned(), "apps/web/.env.local".to_owned()],
            "tracked, in an ignored folder or in Hedy's copies: not listed"
        );
    }

    #[test]
    fn a_detached_copy_adds_and_removes_no_branch() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let branches = || git(&repo, &["branch", "--list"]).unwrap();
        let before = branches();
        let copy = repo.join(".hedy/worktrees/t1");
        g.add_detached_worktree(&repo, &copy, &base).unwrap();
        assert!(copy.join(".git").exists());
        assert_eq!(g.head_commit(&copy).unwrap(), base);
        fs::write(copy.join("nota.md"), "borrador\n").unwrap();
        g.remove_detached_worktree(&repo, &copy).unwrap();
        assert!(!copy.exists());
        assert_eq!(branches(), before, "no branch made or left");
        assert!(!git(&repo, &["worktree", "list"]).unwrap().contains("t1"));
        // Already gone: nothing to do.
        g.remove_detached_worktree(&repo, &copy).unwrap();
    }

    #[test]
    fn a_task_copy_recorded_with_relative_paths_keeps_its_git() {
        use crate::application::ports::CodeReader;
        let (_tmp, repo) = repo();
        if git(&repo, &["config", "worktree.useRelativePaths", "true"]).is_err() {
            return;
        }
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let copy = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &copy, "hedy/t1", &base).unwrap();
        fs::write(copy.join("app.py"), "def crear():\n    return 1\n").unwrap();
        let changes = g.changes(&copy, &base, None).unwrap();
        assert!(changes.iter().any(|c| c.path == "app.py"), "{changes:?}");
        assert!(
            g.diff(&copy, &base)
                .unwrap()
                .iter()
                .any(|f| f.path == "app.py")
        );
    }

    #[test]
    fn a_repository_nested_in_a_task_copy_is_never_entered() {
        use crate::application::ports::CodeReader;
        let (tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let copy = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &copy, "hedy/t1", &base).unwrap();
        let ran = tmp.path().join("ran");
        let nested = copy.join("lib");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("a.py"), "x = 1\n").unwrap();
        plant_git_dir(&nested.join(".git"), &nested, &repo, &ran);
        fs::write(copy.join("app.py"), "def crear():\n    return 1\n").unwrap();
        // A real repository of its own, with a committed file it changes
        // after being staged and a clean filter in its config.
        fs::remove_dir_all(nested.join(".git")).unwrap();
        let nested_git = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(&nested)
                .args([
                    "-c",
                    "user.name=T",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
        };
        nested_git(&["init", "-q"]);
        nested_git(&["add", "a.py"]);
        nested_git(&["commit", "-qm", "nested"]);
        let mut config = fs::read_to_string(nested.join(".git/config")).unwrap();
        config.push_str(&format!(
            "[filter \"evil\"]\n\tclean = \"touch {}\"\n",
            ran.display()
        ));
        fs::write(nested.join(".git/config"), config).unwrap();
        fs::write(nested.join(".gitattributes"), "* filter=evil\n").unwrap();

        let diff = g.diff(&copy, &base).unwrap();
        assert!(diff.iter().any(|f| f.path == "app.py"));
        fs::write(nested.join("a.py"), "x = 2\n").unwrap();
        g.diff(&copy, &base).unwrap();
        g.changes(&copy, &base, None).unwrap();
        g.uncommitted_changes(&copy).unwrap();
        fs::write(nested.join("a.py"), "x = 3\n").unwrap();
        g.commit_all(&copy, "Cambios").unwrap();
        g.diff(&copy, &base).unwrap();
        assert!(!ran.exists(), "a nested repository's config ran a command");
    }

    #[test]
    fn a_task_copy_git_no_longer_knows_runs_no_git_folder_of_its_own() {
        use crate::application::ports::CodeReader;
        let (tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let copy = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &copy, "hedy/t1", &base).unwrap();
        // Its `.git` gone, a prune forgets it; whatever it planted is not used.
        fs::remove_file(copy.join(".git")).unwrap();
        git(&repo, &["worktree", "prune"]).unwrap();
        let ran = tmp.path().join("ran");
        for planted in [".hedy-no-git", ".git"] {
            plant_git_dir(&copy.join(planted), &copy, &repo, &ran);
        }
        assert!(g.changes(&copy, &base, None).is_err() || !ran.exists());
        let _ = g.diff(&copy, &base);
        assert!(!ran.exists(), "a git folder the copy made ran a command");
    }

    #[test]
    fn a_copy_that_brought_in_main_shows_only_its_own_changes_and_a_switch_is_seen() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let wt = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &wt, "hedy/t1", &base).unwrap();
        assert_eq!(g.diff_base(&wt, &base, "main").unwrap(), base);
        assert_eq!(g.branch_off(&wt, "hedy/t1").unwrap(), None);
        // Someone else's work lands on main; the AI writes its own change.
        fs::write(repo.join("otro.py"), "x = 1\n").unwrap();
        git(&repo, &["add", "."]).unwrap();
        git(&repo, &["commit", "-qm", "De otra persona"]).unwrap();
        fs::write(wt.join("app.py"), "def crear():\n    return 1\n").unwrap();
        git(&wt, &["add", "."]).unwrap();
        git(&wt, &["commit", "-qm", "Tarea"]).unwrap();
        // Main moved on alone: the task is still measured from where it started.
        assert_eq!(g.diff_base(&wt, &base, "main").unwrap(), base);
        // The AI brings main in: its work only, never the other person's.
        git(&wt, &["merge", "-q", "--no-edit", "main"]).unwrap();
        let measured = g.diff_base(&wt, &base, "main").unwrap();
        assert_eq!(measured, g.head_commit(&repo).unwrap());
        let paths: Vec<_> = g
            .diff(&wt, &measured)
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(paths, ["app.py"]);
        // The AI switches the copy to another branch: seen.
        git(&wt, &["checkout", "-q", "-b", "otra"]).unwrap();
        assert_eq!(
            g.branch_off(&wt, "hedy/t1").unwrap().as_deref(),
            Some("otra")
        );
        git(&wt, &["checkout", "-q", "--detach"]).unwrap();
        assert_eq!(
            g.branch_off(&wt, "hedy/t1").unwrap().as_deref(),
            Some("HEAD")
        );
    }

    #[test]
    fn a_tag_named_like_the_base_branch_hides_no_commit() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let wt = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &wt, "hedy/t1", &base).unwrap();
        // The AI commits a change, tags it `main` and adds a harmless one on top.
        fs::write(wt.join("oculto.py"), "x = 1\n").unwrap();
        git(&wt, &["add", "."]).unwrap();
        git(&wt, &["commit", "-qm", "Oculto"]).unwrap();
        git(&wt, &["tag", "main"]).unwrap();
        fs::write(wt.join("app.py"), "y = 2\n").unwrap();
        git(&wt, &["add", "."]).unwrap();
        git(&wt, &["commit", "-qm", "Visible"]).unwrap();
        // Measured from where it started: both changes in sight.
        assert_eq!(g.diff_base(&wt, &base, "main").unwrap(), base);
    }

    #[test]
    fn full_task_lifecycle_in_a_worktree() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        assert_eq!(g.current_branch(&repo).unwrap(), "main");
        let base = g.head_commit(&repo).unwrap();

        g.ensure_excluded(&repo, "/.hedy/").unwrap();
        g.ensure_excluded(&repo, "/.hedy/").unwrap();
        let exclude = fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches("/.hedy/").count(), 1);

        let wt = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &wt, "hedy/recordatorios-t1", &base)
            .unwrap();
        fs::write(wt.join("app.py"), "def crear():\n    programar()\n").unwrap();
        fs::write(wt.join("test_app.py"), "def test_x():\n    assert True\n").unwrap();

        let diff = g.diff(&wt, &base).unwrap();
        let paths: Vec<_> = diff.iter().map(|f| (f.path.as_str(), f.status)).collect();
        assert_eq!(
            paths,
            vec![
                ("app.py", FileStatus::Modified),
                ("test_app.py", FileStatus::Added)
            ]
        );
        // The main checkout does not see the task's files.
        assert!(
            git(&repo, &["status", "--porcelain"])
                .unwrap()
                .trim()
                .is_empty()
        );

        assert!(g.commit_all(&wt, "Recordatorios").unwrap());
        assert!(!g.commit_all(&wt, "Nada").unwrap());
        assert_eq!(
            g.merge(&repo, "main", "hedy/recordatorios-t1").unwrap(),
            MergeOutcome::Merged
        );
        g.remove_worktree(&repo, &wt, "hedy/recordatorios-t1")
            .unwrap();

        assert!(!wt.exists());
        assert!(
            fs::read_to_string(repo.join("app.py"))
                .unwrap()
                .contains("programar")
        );
        let commits = g.recent_commits(&repo, 5).unwrap();
        assert_eq!(commits[0].subject, "Recordatorios");
        assert!(
            git(&repo, &["branch", "--list", "hedy/*"])
                .unwrap()
                .trim()
                .is_empty()
        );
    }

    #[test]
    fn lists_tracked_and_untracked_files_but_not_ignored_ones() {
        let (_tmp, repo) = repo();
        fs::write(repo.join(".gitignore"), "build/\n").unwrap();
        fs::create_dir_all(repo.join("build")).unwrap();
        fs::write(repo.join("build/out.js"), "x").unwrap();
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/nuevo archivo.py"), "x").unwrap();
        let g = GitCli;
        g.ensure_excluded(&repo, "/.hedy/").unwrap();
        fs::create_dir_all(repo.join(".hedy/worktrees/t1")).unwrap();
        fs::write(repo.join(".hedy/worktrees/t1/app.py"), "x").unwrap();
        let mut files = g.list_files(&repo).unwrap();
        files.sort();
        assert_eq!(files, [".gitignore", "app.py", "src/nuevo archivo.py"]);
    }

    #[test]
    fn a_new_repository_has_one_empty_commit_to_fork_tasks_from() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("tienda");
        fs::create_dir(&dir).unwrap();
        // The test's own identity; `init_repo` keeps it (re-initializing is safe).
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "test@hedy.local"],
            &["config", "user.name", "Test"],
            &["config", "commit.gpgsign", "false"],
        ] {
            git(&dir, args).unwrap();
        }
        let g = GitCli;
        g.init_repo(&dir, "Initial commit").unwrap();

        let base = g.head_commit(&dir).unwrap();
        assert!(!g.current_branch(&dir).unwrap().is_empty());
        assert_eq!(g.recent_commits(&dir, 5).unwrap().len(), 1);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "only .git");
        let wt = dir.join(".hedy/worktrees/t1");
        g.add_worktree(&dir, &wt, "hedy/primera-t1", &base).unwrap();
        assert!(wt.is_dir());
    }

    #[test]
    fn shared_dependency_symlinks_stay_out_of_the_diff() {
        use crate::application::ports::FileSystem;
        let (_tmp, repo) = repo();
        let g = GitCli;
        fs::write(repo.join(".gitignore"), "node_modules/\n").unwrap();
        git(&repo, &["add", "."]).unwrap();
        git(&repo, &["commit", "-qm", "ignore"]).unwrap();
        fs::create_dir_all(repo.join("node_modules/pkg")).unwrap();
        assert!(g.is_ignored(&repo, "node_modules"));
        assert!(!g.is_ignored(&repo, "app.py"));

        let base = g.head_commit(&repo).unwrap();
        let wt = repo.join(".hedy/worktrees/t3");
        g.add_worktree(&repo, &wt, "hedy/t3", &base).unwrap();
        g.ensure_excluded(&repo, "/node_modules").unwrap();
        crate::adapters::LocalFileSystem::new(repo.clone(), repo.join("chats"))
            .symlink_dir(&repo.join("node_modules"), &wt.join("node_modules"))
            .unwrap();
        assert!(wt.join("node_modules/pkg").is_dir());
        assert!(
            g.diff(&wt, &base).unwrap().is_empty(),
            "the symlink is not a change"
        );
        g.remove_worktree(&repo, &wt, "hedy/t3").unwrap();
        assert!(
            repo.join("node_modules/pkg").is_dir(),
            "the shared folder survives"
        );
    }

    #[test]
    fn merge_refuses_a_dirty_or_moved_checkout() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let wt = repo.join(".hedy/worktrees/t2");
        g.add_worktree(&repo, &wt, "hedy/t2", &base).unwrap();
        fs::write(wt.join("nuevo.py"), "x = 1\n").unwrap();
        g.commit_all(&wt, "Nuevo").unwrap();

        fs::write(repo.join("app.py"), "cambio local\n").unwrap();
        assert_eq!(
            g.merge(&repo, "main", "hedy/t2").unwrap(),
            MergeOutcome::Dirty
        );
        git(&repo, &["checkout", "-q", "--", "app.py"]).unwrap();

        git(&repo, &["checkout", "-qb", "otra"]).unwrap();
        assert_eq!(
            g.merge(&repo, "main", "hedy/t2").unwrap(),
            MergeOutcome::NotOnBase {
                current: "otra".into()
            }
        );
    }

    #[test]
    fn a_merge_blocked_by_untracked_files_is_not_a_conflict() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let wt = repo.join(".hedy/worktrees/t3");
        g.add_worktree(&repo, &wt, "hedy/t3", &base).unwrap();
        fs::write(wt.join("nuevo.py"), "x = 1\n").unwrap();
        g.commit_all(&wt, "Nuevo").unwrap();
        // The same file exists untracked in the main checkout.
        fs::write(repo.join("nuevo.py"), "mío\n").unwrap();
        let error = g.merge(&repo, "main", "hedy/t3").unwrap_err();
        assert!(error.message.contains("nuevo.py"), "{}", error.message);
        assert_eq!(fs::read_to_string(repo.join("nuevo.py")).unwrap(), "mío\n");
    }

    /// A task branch with one commit, merged into `main` like "Aceptar" does.
    /// Returns (before, after, tip).
    fn accept(repo: &Path, id: &str, file: &str) -> (String, String, String) {
        let g = GitCli;
        let base = g.head_commit(repo).unwrap();
        let wt = repo.join(format!(".hedy/worktrees/{id}"));
        let branch = format!("hedy/{id}");
        g.add_worktree(repo, &wt, &branch, &base).unwrap();
        fs::write(wt.join(file), "nuevo\n").unwrap();
        fs::write(wt.join("app.py"), format!("def crear():\n    {id}()\n")).unwrap();
        g.commit_all(&wt, "Tarea\n\nCon cuerpo.").unwrap();
        let tip = g.rev_parse(&wt, "HEAD").unwrap();
        let before = g.head_commit(repo).unwrap();
        assert_eq!(
            g.merge(repo, "main", &branch).unwrap(),
            MergeOutcome::Merged
        );
        let after = g.head_commit(repo).unwrap();
        g.remove_worktree(repo, &wt, &branch).unwrap();
        (before, after, tip)
    }

    #[test]
    fn undo_moves_main_back_and_the_branch_returns_from_its_tip() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        g.ensure_excluded(&repo, "/.hedy/").unwrap();
        // Unrelated local work: an untracked file and a change to a file the
        // merge does not touch.
        fs::write(repo.join("README.md"), "hola\n").unwrap();
        git(&repo, &["add", "README.md"]).unwrap();
        git(&repo, &["commit", "-qm", "readme"]).unwrap();
        let (before, after, tip) = accept(&repo, "t1", "nuevo.py");
        assert_eq!(
            git(&repo, &["log", "-1", "--format=%B"]).unwrap().trim(),
            "Tarea\n\nCon cuerpo."
        );
        fs::write(repo.join("notas.txt"), "mías\n").unwrap();
        fs::write(repo.join("README.md"), "hola, cambio local\n").unwrap();

        assert!(
            g.remote_branches_containing(&repo, &after)
                .unwrap()
                .is_empty()
        );
        let wt = repo.join(".hedy/worktrees/t1");
        g.add_worktree(&repo, &wt, "hedy/t1", &tip).unwrap();
        g.reset_keep(&repo, &before).unwrap();

        assert_eq!(g.head_commit(&repo).unwrap(), before);
        assert!(!repo.join("nuevo.py").exists());
        assert!(
            !fs::read_to_string(repo.join("app.py"))
                .unwrap()
                .contains("t1")
        );
        // The unrelated work is untouched.
        assert_eq!(
            fs::read_to_string(repo.join("notas.txt")).unwrap(),
            "mías\n"
        );
        assert_eq!(
            fs::read_to_string(repo.join("README.md")).unwrap(),
            "hola, cambio local\n"
        );
        // The task's work is back on its branch, and can be merged again.
        assert_eq!(fs::read_to_string(wt.join("nuevo.py")).unwrap(), "nuevo\n");
        assert_eq!(g.diff(&wt, &before).unwrap().len(), 2);
        git(&repo, &["checkout", "-q", "--", "README.md"]).unwrap();
        assert_eq!(
            g.merge(&repo, "main", "hedy/t1").unwrap(),
            MergeOutcome::Merged
        );
        assert!(repo.join("nuevo.py").exists());
    }

    #[test]
    fn a_true_merge_commit_is_undone_too() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let base = g.head_commit(&repo).unwrap();
        let wt = repo.join(".hedy/worktrees/t2");
        g.add_worktree(&repo, &wt, "hedy/t2", &base).unwrap();
        fs::write(wt.join("tarea.py"), "x = 1\n").unwrap();
        g.commit_all(&wt, "Tarea").unwrap();
        // main moved meanwhile: the merge makes a merge commit.
        fs::write(repo.join("otro.py"), "y = 2\n").unwrap();
        git(&repo, &["add", "otro.py"]).unwrap();
        git(&repo, &["commit", "-qm", "otro"]).unwrap();
        let before = g.head_commit(&repo).unwrap();
        assert_eq!(
            g.merge(&repo, "main", "hedy/t2").unwrap(),
            MergeOutcome::Merged
        );
        let after = g.head_commit(&repo).unwrap();
        assert_eq!(g.rev_parse(&repo, &format!("{after}^1")).unwrap(), before);
        g.reset_keep(&repo, &before).unwrap();
        assert!(!repo.join("tarea.py").exists());
        assert!(repo.join("otro.py").exists());
    }

    #[test]
    fn reset_keep_refuses_to_overwrite_local_changes_and_changes_nothing() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        let (before, after, _) = accept(&repo, "t3", "nuevo.py");
        fs::write(repo.join("app.py"), "a mano\n").unwrap();
        let error = g.reset_keep(&repo, &before).unwrap_err();
        assert!(error.message.contains("app.py"), "{}", error.message);
        assert_eq!(g.head_commit(&repo).unwrap(), after);
        assert_eq!(fs::read_to_string(repo.join("app.py")).unwrap(), "a mano\n");
        assert!(repo.join("nuevo.py").exists());
    }

    #[test]
    fn pushed_merges_are_reported_by_their_remote_branch() {
        let (tmp, repo) = repo();
        let g = GitCli;
        let remote = tmp.path().join("remoto.git");
        git(tmp.path(), &["init", "-q", "--bare", "remoto.git"]).unwrap();
        git(
            &repo,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        )
        .unwrap();
        git(&repo, &["push", "-q", "origin", "main"]).unwrap();
        let (_, after, _) = accept(&repo, "t4", "nuevo.py");
        assert!(
            g.remote_branches_containing(&repo, &after)
                .unwrap()
                .is_empty()
        );
        git(&repo, &["push", "-q", "origin", "main"]).unwrap();
        assert_eq!(
            g.remote_branches_containing(&repo, &after).unwrap(),
            vec!["origin/main".to_owned()]
        );
        assert!(g.rev_parse(&repo, "no-existe").is_err());
    }

    #[test]
    fn helpful_errors() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "-q", "-b", "main"]).unwrap();
        let err = GitCli.head_commit(tmp.path()).unwrap_err();
        assert!(err.message.contains("no tiene commits"));
    }

    #[test]
    fn diffs_uncommitted_work_and_ranges_without_reading_options() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        assert!(g.work_diff(&repo).unwrap().0.is_empty());
        fs::write(repo.join("app.py"), "def crear():\n    avisar()\n").unwrap();
        let (work, left) = g.work_diff(&repo).unwrap();
        assert_eq!((work.len(), left), (1, 0));
        assert_eq!(work[0].additions, 1);
        git(&repo, &["checkout", "-qb", "feat"]).unwrap();
        git(&repo, &["commit", "-qam", "Avisa"]).unwrap();
        fs::write(repo.join("nuevo.py"), "x = 1\n").unwrap();
        git(&repo, &["add", "nuevo.py"]).unwrap();
        git(&repo, &["commit", "-qm", "Nuevo"]).unwrap();
        let range = g.revs_diff(&repo, "main..feat").unwrap();
        assert_eq!(
            range.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["app.py", "nuevo.py"]
        );
        // Uncommitted work: changed files and new ones not added yet; a new
        // protected one is only named.
        fs::write(repo.join("app.py"), "def crear():\n    avisar(1)\n").unwrap();
        fs::write(repo.join("otro.py"), "y = 2\nz = 3\n").unwrap();
        fs::write(repo.join(".env"), "KEY=secret\n").unwrap();
        // A hard link to it is never read either.
        fs::hard_link(repo.join(".env"), repo.join("zz.txt")).unwrap();
        // Nor one `.gitattributes` encrypts or keeps from diffs.
        fs::write(
            repo.join(".gitattributes"),
            "secreto.yaml filter=git-crypt diff=git-crypt\nprivado.txt -diff\n",
        )
        .unwrap();
        fs::write(repo.join("secreto.yaml"), "password: hunter2\n").unwrap();
        fs::write(repo.join("privado.txt"), "PRIVADO\n").unwrap();
        let (work, _) = g.work_diff(&repo).unwrap();
        let summary: Vec<(&str, u32, usize)> = work
            .iter()
            .map(|f| (f.path.as_str(), f.additions, f.hunks.len()))
            .collect();
        assert_eq!(
            summary,
            [
                ("app.py", 1, 1),
                (".env", 0, 0),
                (".gitattributes", 2, 1),
                ("otro.py", 2, 1),
                ("privado.txt", 0, 0),
                ("secreto.yaml", 0, 0),
                ("zz.txt", 0, 0)
            ]
        );

        let lines = work.iter().flat_map(|f| &f.hunks).flat_map(|h| &h.lines);
        assert!(
            lines
                .clone()
                .all(|l| !l.text.contains("hunter2") && !l.text.contains("PRIVADO"))
        );
        for name in [".gitattributes", "secreto.yaml", "privado.txt"] {
            fs::remove_file(repo.join(name)).unwrap();
        }
        assert!(lines.clone().all(|l| !l.text.contains("KEY=secret")));
        fs::remove_file(repo.join("zz.txt")).unwrap();
        assert!(g.revs_diff(&repo, "HEAD:.env..HEAD:app.py").is_err());
        // Two blobs by their hashes are no range of commits either.
        let blob = git(&repo, &["rev-parse", "HEAD:app.py"]).unwrap();
        let blobs = format!("{0}..{0}", blob.trim());
        assert!(g.revs_diff(&repo, &blobs).is_err());
        assert!(g.revs_diff(&repo, "main...feat").is_ok());
        // A worktree's main checkout is the repository's.
        let wt = repo.join(".hedy/worktrees/t1");
        git(
            &repo,
            &["worktree", "add", "-q", wt.to_str().unwrap(), "main"],
        )
        .unwrap();
        let main = repo.canonicalize().unwrap();
        assert_eq!(g.main_root(&wt).unwrap().canonicalize().unwrap(), main);
        assert_eq!(g.main_root(&repo).unwrap().canonicalize().unwrap(), main);
        // A separate git folder: a worktree of it is refused, never guessed.
        let tmp = tempfile::tempdir().unwrap();
        let apart = tmp.path().join("apart");
        let store = tmp.path().join("apart.git");
        git(
            tmp.path(),
            &[
                "init",
                "-q",
                "-b",
                "main",
                "--separate-git-dir",
                store.to_str().unwrap(),
                apart.to_str().unwrap(),
            ],
        )
        .unwrap();
        for args in [
            &["config", "user.email", "t@l"][..],
            &["config", "user.name", "T"],
            &["config", "commit.gpgsign", "false"],
            &["commit", "-q", "--allow-empty", "-m", "Inicio"],
        ] {
            git(&apart, args).unwrap();
        }
        assert_eq!(g.main_root(&apart).unwrap(), apart);
        let wt = apart.join("wt");
        git(&apart, &["worktree", "add", "-q", wt.to_str().unwrap()]).unwrap();
        assert!(g.main_root(&wt).is_err());
        // Never read as an option.
        let out = repo.join("written");
        let option = format!("--output={}", out.display());
        assert!(g.revs_diff(&repo, &option).is_err());
        assert!(!out.exists());
    }

    #[test]
    fn one_commit_is_its_own_change_first_and_merge_commits_too() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        // The first commit: against nothing, not the working tree.
        fs::write(repo.join("app.py"), "cambiado\n").unwrap();
        let first = g.revs_diff(&repo, "HEAD").unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!((first[0].additions, first[0].deletions), (2, 0));
        git(&repo, &["commit", "-qam", "Cambio"]).unwrap();
        let one = g.revs_diff(&repo, "HEAD").unwrap();
        assert_eq!((one[0].additions, one[0].deletions), (1, 2));
        assert_eq!(g.revs_diff(&repo, "HEAD^!").unwrap(), one);
        // A merge: what it brings over its first parent.
        git(&repo, &["checkout", "-qb", "rama", "HEAD~1"]).unwrap();
        fs::write(repo.join("otro.py"), "x = 1\n").unwrap();
        git(&repo, &["add", "otro.py"]).unwrap();
        git(&repo, &["commit", "-qm", "Otro"]).unwrap();
        git(&repo, &["checkout", "-q", "main"]).unwrap();
        git(&repo, &["merge", "-q", "--no-edit", "--no-ff", "rama"]).unwrap();
        let merge = g.revs_diff(&repo, "HEAD").unwrap();
        assert_eq!(
            merge.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["otro.py"]
        );
        assert!(g.revs_diff(&repo, "no-such-thing").is_err());
    }

    #[test]
    fn commits_only_what_is_staged_and_runs_the_users_hooks() {
        let (_tmp, repo) = repo();
        let g = GitCli;
        assert!(g.staged_diff(&repo).unwrap().is_empty());
        fs::write(repo.join("app.py"), "def crear():\n    avisar()\n").unwrap();
        fs::write(repo.join("notas.txt"), "sin preparar\n").unwrap();
        git(&repo, &["add", "app.py"]).unwrap();
        let staged = g.staged_diff(&repo).unwrap();
        assert_eq!(
            staged.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["app.py"]
        );

        // A hook that leaves a mark: git runs it as for any commit.
        let hook = repo.join(".git/hooks/commit-msg");
        fs::write(
            &hook,
            "#!/bin/sh\necho hook >> \"$(dirname \"$0\")/../hook-ran\"\n",
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        g.commit_staged(&repo, "feat: avisa al crear\n", false)
            .unwrap();
        assert!(repo.join(".git/hook-ran").exists());
        assert!(!repo.join(".git/HEDY_COMMIT_MSG").exists());
        assert_eq!(
            g.recent_commits(&repo, 1).unwrap()[0].subject,
            "feat: avisa al crear"
        );
        // The rest stays as it was, unstaged.
        let status = git(&repo, &["status", "--porcelain"]).unwrap();
        assert_eq!(status.trim(), "?? notas.txt");
        // Nothing staged: git refuses and says nothing was committed.
        assert!(g.commit_staged(&repo, "otra", false).is_err());
    }
}
