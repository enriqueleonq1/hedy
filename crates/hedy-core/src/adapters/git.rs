//! Git through its CLI: worktrees, diffs, commits and merges are simpler and
//! more faithful this way than through a library.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::application::ports::{Git, MergeOutcome, PortError, PortResult};
use crate::domain::diff::{Commit, FileDiff, parse_unified};

pub struct GitCli;

fn run(dir: &Path, args: &[&str]) -> PortResult<Output> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| PortError::new(format!("no se pudo ejecutar git: {e}")))
}

/// Runs git and returns stdout, or an error with git's own message.
fn git(dir: &Path, args: &[&str]) -> PortResult<String> {
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
            .map_err(|_| PortError::new("el repositorio no está en una rama (HEAD separado)"))
    }

    fn head_commit(&self, repo: &Path) -> PortResult<String> {
        git(repo, &["rev-parse", "--verify", "HEAD"])
            .map(|s| s.trim().to_owned())
            .map_err(|_| PortError::new("el repositorio no tiene commits todavía; haz el primero"))
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
            Ok(MergeOutcome::Merged)
        } else {
            let _ = run(repo, &["merge", "--abort"]);
            Ok(MergeOutcome::Conflict)
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::FileStatus;

    fn repo() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("moira");
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
        crate::adapters::LocalFileSystem::new(repo.clone())
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
    fn helpful_errors() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "-q", "-b", "main"]).unwrap();
        let err = GitCli.head_commit(tmp.path()).unwrap_err();
        assert!(err.message.contains("no tiene commits"));
    }
}
