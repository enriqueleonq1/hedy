//! Git through its CLI: worktrees, diffs, commits and merges are simpler and
//! more faithful this way than through a library.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::application::ports::{Git, MergeOutcome, PortError, PortResult};
use crate::domain::diff::{Commit, FileDiff, parse_unified};
use crate::tr;

pub struct GitCli;

pub(super) fn run(dir: &Path, args: &[&str]) -> PortResult<Output> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| PortError::new(format!("no se pudo ejecutar git: {e}")))
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

    fn is_ignored(&self, repo: &Path, path: &str) -> bool {
        run(repo, &["check-ignore", "-q", path]).is_ok_and(|out| out.status.success())
    }

    fn diff(&self, worktree: &Path, base: &str) -> PortResult<Vec<FileDiff>> {
        // Staging inside the task's own worktree makes new files part of the diff.
        git(worktree, &["add", "--all"])?;
        let patch = git(
            worktree,
            &[
                "diff",
                "--cached",
                "--no-color",
                "--no-ext-diff",
                "-M",
                base,
            ],
        )?;
        Ok(parse_unified(&patch))
    }

    fn commit_all(&self, worktree: &Path, message: &str) -> PortResult<bool> {
        git(worktree, &["add", "--all"])?;
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
        if path.exists() {
            git(
                repo,
                &["worktree", "remove", "--force", &path.to_string_lossy()],
            )?;
        }
        let _ = git(repo, &["worktree", "prune"]);
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
        let patch = git(
            repo,
            &["diff", "--cached", "--no-color", "--no-ext-diff", "-M"],
        )?;
        Ok(parse_unified(&patch))
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
        let status = Command::new("git").arg("-C").arg(repo).args(&args).status();
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
