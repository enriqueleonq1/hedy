//! The sync repo's working copy over the `git` command (docs/decisions.md
//! §34). Any git server works, with the user's own credentials (SSH agent,
//! credential helper): Hedy never asks for or keeps a token.
//!
//! Commits use a neutral identity ("Hedy", `sync@hedy.invalid`), so the
//! user's email never lands in the sync repo, and never ask for a password
//! (`GIT_TERMINAL_PROMPT=0`, SSH in batch mode): a missing credential fails
//! fast instead of waiting on a prompt nobody sees.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::application::ports::{PortError, PortResult, SyncRemote};
use crate::tr;

/// Tries to push this many times when another PC pushed first.
const PUSH_ATTEMPTS: usize = 4;

#[derive(Default)]
pub struct GitSyncRemote;

fn git(dir: &Path, args: &[&str]) -> PortResult<Output> {
    Command::new("git")
        .args([
            "-c",
            "user.name=Hedy",
            "-c",
            "user.email=sync@hedy.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes")
        .output()
        .map_err(|e| {
            PortError::new(tr!(
                "no se pudo ejecutar git: {e}",
                "could not run git: {e}"
            ))
        })
}

/// Runs git and fails with its message.
fn run(dir: &Path, args: &[&str]) -> PortResult<String> {
    let out = git(dir, args)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        let message = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let command = args.first().copied().unwrap_or("");
        Err(PortError::new(tr!(
            "git {command} falló: {message}",
            "git {command} failed: {message}"
        )))
    }
}

fn succeeds(dir: &Path, args: &[&str]) -> bool {
    git(dir, args).is_ok_and(|out| out.status.success())
}

/// The branch the sync uses: the clone's own, or `main` in an empty repo.
fn branch(dir: &Path) -> PortResult<String> {
    run(dir, &["symbolic-ref", "--short", "HEAD"])
}

/// A path inside `dir`, refusing anything that would leave it.
fn inside(dir: &Path, path: &str) -> PortResult<PathBuf> {
    let safe = !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != ".." && part != "." && part != ".git");
    if safe {
        Ok(dir.join(path))
    } else {
        Err(PortError::new(tr!(
            "ruta de sincronización inválida: {path}",
            "invalid sync path: {path}"
        )))
    }
}

impl SyncRemote for GitSyncRemote {
    fn open(&self, url: &str, dir: &Path) -> PortResult<()> {
        if dir.join(".git").is_dir() {
            let current = run(dir, &["remote", "get-url", "origin"])?;
            if current != url {
                return Err(PortError::new(tr!(
                    "la carpeta de sincronización ya usa otro repo ({current})",
                    "the sync folder already uses another repo ({current})"
                )));
            }
            return self.pull(dir);
        }
        let parent = dir.parent().unwrap_or(dir);
        std::fs::create_dir_all(parent).map_err(|e| PortError::new(e.to_string()))?;
        let target = dir.to_string_lossy().into_owned();
        run(parent, &["clone", "--quiet", "--", url, &target])?;
        // An empty repo has no branch yet: start `main`.
        if !succeeds(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
            run(dir, &["symbolic-ref", "HEAD", "refs/heads/main"])?;
        }
        Ok(())
    }

    fn pull(&self, dir: &Path) -> PortResult<()> {
        run(dir, &["fetch", "--quiet", "origin"])?;
        let branch = branch(dir)?;
        let remote = format!("origin/{branch}");
        if !succeeds(dir, &["rev-parse", "--verify", "--quiet", &remote]) {
            return Ok(());
        }
        if succeeds(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
            // Each PC only adds its own files: a rebase never conflicts.
            run(dir, &["rebase", "--quiet", &remote]).map(drop)
        } else {
            run(dir, &["reset", "--quiet", "--hard", &remote]).map(drop)
        }
    }

    fn files(&self, dir: &Path, prefix: &str) -> PortResult<Vec<String>> {
        let mut found = Vec::new();
        let start = if prefix.is_empty() {
            dir.to_owned()
        } else {
            inside(dir, prefix.trim_end_matches('/'))?
        };
        let mut pending = vec![start];
        while let Some(folder) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if entry.file_name() == ".git" {
                    continue;
                }
                if path.is_dir() {
                    pending.push(path);
                } else if let Ok(relative) = path.strip_prefix(dir) {
                    found.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        found.sort();
        Ok(found)
    }

    fn read(&self, dir: &Path, path: &str) -> PortResult<Option<Vec<u8>>> {
        match std::fs::read(inside(dir, path)?) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(PortError::new(e.to_string())),
        }
    }

    fn write(&self, dir: &Path, path: &str, bytes: &[u8]) -> PortResult<()> {
        let file = inside(dir, path)?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PortError::new(e.to_string()))?;
        }
        std::fs::write(file, bytes).map_err(|e| PortError::new(e.to_string()))
    }

    fn remove(&self, dir: &Path, path: &str) -> PortResult<()> {
        match std::fs::remove_file(inside(dir, path)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(PortError::new(e.to_string())),
        }
    }

    fn push(&self, dir: &Path, message: &str) -> PortResult<()> {
        run(dir, &["add", "--all"])?;
        if !succeeds(dir, &["diff", "--cached", "--quiet"]) {
            run(dir, &["commit", "--quiet", "--message", message])?;
        }
        if !succeeds(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
            return Ok(());
        }
        let branch = branch(dir)?;
        let mut last = None;
        for _ in 0..PUSH_ATTEMPTS {
            match run(
                dir,
                &["push", "--quiet", "--set-upstream", "origin", &branch],
            ) {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last = Some(e);
                    // Another PC pushed first: take its files and try again.
                    self.pull(dir)?;
                }
            }
        }
        Err(last.expect("at least one attempt"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare "server" and a clone of it per PC.
    fn server() -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("server.git");
        std::fs::create_dir_all(&bare).unwrap();
        run(
            &bare,
            &["init", "--quiet", "--bare", "--initial-branch=main"],
        )
        .unwrap();
        let url = bare.to_string_lossy().into_owned();
        (tmp, url)
    }

    #[test]
    fn two_clones_exchange_files_through_an_empty_repo() {
        let (tmp, url) = server();
        let git = GitSyncRemote;
        let (home, office) = (tmp.path().join("home"), tmp.path().join("office"));

        git.open(&url, &home).unwrap();
        assert!(git.files(&home, "").unwrap().is_empty());
        git.write(&home, "vault/epochs/000001.json", b"{}").unwrap();
        git.write(&home, "devices/a/changes/0000000001.hedy", b"uno")
            .unwrap();
        git.push(&home, "hedy: home 1").unwrap();
        // Nothing new: no empty commit.
        git.push(&home, "hedy: nothing").unwrap();

        git.open(&url, &office).unwrap();
        assert_eq!(
            git.files(&office, "").unwrap(),
            [
                "devices/a/changes/0000000001.hedy",
                "vault/epochs/000001.json"
            ]
        );
        assert_eq!(
            git.read(&office, "devices/a/changes/0000000001.hedy")
                .unwrap()
                .unwrap(),
            b"uno"
        );
        assert_eq!(git.read(&office, "devices/b/snapshot.hedy").unwrap(), None);

        // Both write their own files; the second push pulls and retries.
        git.write(&office, "devices/b/changes/0000000001.hedy", b"dos")
            .unwrap();
        git.write(&home, "devices/a/changes/0000000002.hedy", b"tres")
            .unwrap();
        git.push(&home, "hedy: home 2").unwrap();
        git.push(&office, "hedy: office 1").unwrap();
        git.pull(&home).unwrap();
        assert_eq!(git.files(&home, "devices/").unwrap().len(), 3);

        // Removing a file reaches the other clone.
        git.remove(&home, "devices/a/changes/0000000001.hedy")
            .unwrap();
        git.push(&home, "hedy: compact").unwrap();
        git.pull(&office).unwrap();
        assert_eq!(
            git.files(&office, "devices/a").unwrap(),
            ["devices/a/changes/0000000002.hedy"]
        );

        // Commits never carry the user's identity.
        let author = run(&home, &["log", "-1", "--format=%an <%ae>"]).unwrap();
        assert_eq!(author, "Hedy <sync@hedy.invalid>");
    }

    #[test]
    fn refuses_paths_outside_the_repo_and_another_remote() {
        let (tmp, url) = server();
        let git = GitSyncRemote;
        let dir = tmp.path().join("pc");
        git.open(&url, &dir).unwrap();
        for bad in ["../x", "/etc/passwd", ".git/config", "a//b", ""] {
            assert!(git.write(&dir, bad, b"x").is_err(), "{bad}");
        }
        assert!(git.open("/elsewhere/repo.git", &dir).is_err());
        assert!(
            git.open("/no/such/repo.git", &tmp.path().join("other"))
                .is_err()
        );
    }
}
