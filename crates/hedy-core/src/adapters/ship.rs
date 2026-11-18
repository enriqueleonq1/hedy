//! Shipping through the system's own tools: git (with the user's credential
//! helpers and SSH keys) and GitHub CLI (with its own login). Hedy stores
//! nothing, never reads a token and never forces a push.

use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::process::find_program;
use crate::application::ports::{
    CodeHost, PortError, PortResult, PullRequestSpec, SourceControl, Upstream,
};
use crate::domain::ship::{GitFolder, parse_status_v2};
use crate::tr;

/// Pushes and pulls talk to the network: they get time, but not forever.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(180);
const LOCAL_TIMEOUT: Duration = Duration::from_secs(30);
const GH_AUTH_TIMEOUT: Duration = Duration::from_secs(10);
/// `gh auth status` asks the host; its answer is remembered a while.
const GH_READY_TTL: Duration = Duration::from_secs(600);
const GH_LOGGED_OUT_TTL: Duration = Duration::from_secs(60);

struct Ran {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Runs `command`, feeding `input` on stdin, and kills it after `timeout`.
fn run_timed(mut command: Command, input: Option<&str>, timeout: Duration) -> PortResult<Ran> {
    let name = command.get_program().to_string_lossy().into_owned();
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| PortError::new(format!("no se pudo ejecutar {name}: {e}")))?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
    }
    let read = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = String::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_string(&mut out);
            }
            out
        })
    };
    let stdout = read(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = read(child.stderr.take().map(|p| Box::new(p) as _));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(15));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let secs = timeout.as_secs();
                return Err(PortError::new(tr!(
                    "{name} no respondió en {secs} s",
                    "{name} did not answer within {secs} s"
                )));
            }
            Err(e) => return Err(PortError::new(format!("{name}: {e}"))),
        }
    };
    Ok(Ran {
        ok: status.success(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

fn git_command(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    // The project's own repo, whatever the environment says (a hook's
    // `GIT_DIR`): its remotes decide where its memory lives.
    super::sync_git::clear_git_redirects(&mut command);
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        // No prompt for a username: a missing credential fails at once.
        .env("GIT_TERMINAL_PROMPT", "0")
        // A status refresh never takes the index lock from the user's git.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C");
    command
}

fn git_timed(dir: &Path, args: &[&str], timeout: Duration) -> PortResult<String> {
    let ran = run_timed(git_command(dir, args), None, timeout)?;
    if ran.ok {
        Ok(ran.stdout)
    } else {
        let mut message = ran.stderr.trim().to_owned();
        if message.is_empty() {
            message = ran.stdout.trim().to_owned();
        }
        Err(PortError::new(format!(
            "git {}: {message}",
            args.first().copied().unwrap_or("")
        )))
    }
}

fn git(dir: &Path, args: &[&str]) -> PortResult<String> {
    git_timed(dir, args, LOCAL_TIMEOUT)
}

/// The system's git, for the folder's state and its remote.
pub struct GitSourceControl;

impl SourceControl for GitSourceControl {
    fn folder(&self, repo: &Path) -> PortResult<GitFolder> {
        let out = git(repo, &["status", "--porcelain=v2", "--branch"])?;
        Ok(parse_status_v2(&out))
    }

    fn upstream(&self, repo: &Path, branch: &str) -> PortResult<Option<Upstream>> {
        let reference = format!("refs/heads/{branch}");
        let out = git(
            repo,
            &[
                "for-each-ref",
                "--format=%(upstream:remotename)%00%(upstream:remoteref)",
                &reference,
            ],
        )?;
        let mut parts = out.trim_end().split('\0');
        let (Some(remote), Some(remote_ref)) = (parts.next(), parts.next()) else {
            return Ok(None);
        };
        if remote.is_empty() || remote_ref.is_empty() {
            return Ok(None);
        }
        let range = format!("{branch}...{branch}@{{upstream}}");
        // A remote branch that is gone: the next push recreates it.
        let Ok(counts) = git(repo, &["rev-list", "--left-right", "--count", &range]) else {
            return Ok(None);
        };
        let mut counts = counts.split_whitespace().map(|n| n.parse().unwrap_or(0));
        Ok(Some(Upstream {
            remote: remote.to_owned(),
            branch: remote_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(remote_ref)
                .to_owned(),
            ahead: counts.next().unwrap_or(0),
            behind: counts.next().unwrap_or(0),
        }))
    }

    fn remotes(&self, repo: &Path) -> PortResult<Vec<String>> {
        Ok(git(repo, &["remote"])?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }

    fn remote_url(&self, repo: &Path, remote: &str) -> PortResult<Option<String>> {
        Ok(git(repo, &["remote", "get-url", remote])
            .ok()
            .map(|u| u.trim().to_owned())
            .filter(|u| !u.is_empty()))
    }

    fn default_branch(&self, repo: &Path, remote: &str) -> Option<String> {
        let head = format!("refs/remotes/{remote}/HEAD");
        if let Ok(out) = git(repo, &["symbolic-ref", "--quiet", "--short", &head]) {
            let prefix = format!("{remote}/");
            if let Some(branch) = out.trim().strip_prefix(&prefix) {
                return Some(branch.to_owned());
            }
        }
        ["main", "master"].into_iter().find_map(|name| {
            let reference = format!("refs/remotes/{remote}/{name}");
            git(repo, &["show-ref", "--verify", "--quiet", &reference])
                .ok()
                .map(|_| name.to_owned())
        })
    }

    fn push(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
        remote_branch: &str,
        set_upstream: bool,
    ) -> PortResult<()> {
        let refspec = format!("refs/heads/{branch}:refs/heads/{remote_branch}");
        let mut args = vec!["push"];
        if set_upstream {
            args.push("--set-upstream");
        }
        args.extend([remote, refspec.as_str()]);
        git_timed(repo, &args, NETWORK_TIMEOUT).map(drop)
    }

    fn pull_ff_only(&self, repo: &Path) -> PortResult<()> {
        git_timed(repo, &["pull", "--no-rebase", "--ff-only"], NETWORK_TIMEOUT).map(drop)
    }

    fn remove_worktree_keep_branch(&self, repo: &Path, path: &Path) -> PortResult<()> {
        if path.exists() {
            git(
                repo,
                &["worktree", "remove", "--force", &path.to_string_lossy()],
            )?;
        }
        let _ = git(repo, &["worktree", "prune"]);
        Ok(())
    }

    fn attach_worktree(&self, repo: &Path, path: &Path, branch: &str) -> PortResult<()> {
        git(repo, &["worktree", "add", &path.to_string_lossy(), branch]).map(drop)
    }
}

/// GitHub CLI. `gh auth status` only tells whether the login works (its
/// exit code); Hedy never asks for or reads the token.
#[derive(Default)]
pub struct GhCli {
    /// A fixed program (tests); otherwise `gh` on `PATH`.
    program: Option<PathBuf>,
    auth: Mutex<HashMap<String, (Instant, bool)>>,
}

impl GhCli {
    pub fn with_program(program: PathBuf) -> Self {
        Self {
            program: Some(program),
            auth: Mutex::default(),
        }
    }

    fn program(&self) -> Option<PathBuf> {
        self.program.clone().or_else(|| find_program("gh"))
    }

    fn command(&self, repo: &Path, args: &[&str]) -> Option<Command> {
        let mut command = Command::new(self.program()?);
        command
            .current_dir(repo)
            .args(args)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("NO_COLOR", "1");
        Some(command)
    }
}

impl CodeHost for GhCli {
    fn installed(&self) -> bool {
        self.program().is_some_and(|p| p.is_file())
    }

    fn logged_in(&self, repo: &Path, host: &str) -> bool {
        if let Some((at, ok)) = self.auth.lock().unwrap().get(host).copied() {
            let ttl = if ok { GH_READY_TTL } else { GH_LOGGED_OUT_TTL };
            if at.elapsed() < ttl {
                return ok;
            }
        }
        let ok = self
            .command(repo, &["auth", "status", "--hostname", host])
            .and_then(|c| run_timed(c, None, GH_AUTH_TIMEOUT).ok())
            .is_some_and(|ran| ran.ok);
        self.auth
            .lock()
            .unwrap()
            .insert(host.to_owned(), (Instant::now(), ok));
        ok
    }

    fn default_branch(&self, repo: &Path) -> Option<String> {
        let command = self.command(
            repo,
            &[
                "repo",
                "view",
                "--json",
                "defaultBranchRef",
                "--jq",
                ".defaultBranchRef.name",
            ],
        )?;
        let ran = run_timed(command, None, LOCAL_TIMEOUT).ok()?;
        let name = ran.stdout.trim();
        (ran.ok && !name.is_empty()).then(|| name.to_owned())
    }

    fn create_pull_request(&self, repo: &Path, spec: &PullRequestSpec) -> PortResult<String> {
        let mut args = vec![
            "pr",
            "create",
            "--title",
            spec.title.as_str(),
            "--body-file",
            "-",
            "--base",
            spec.base.as_str(),
            "--head",
            spec.head.as_str(),
        ];
        if spec.draft {
            args.push("--draft");
        }
        let command = self
            .command(repo, &args)
            .ok_or_else(|| PortError::new("gh no está instalado"))?;
        let ran = run_timed(command, Some(&spec.body), NETWORK_TIMEOUT)?;
        if ran.ok {
            Ok(ran.stdout)
        } else {
            Err(PortError::new(format!(
                "{}\n{}",
                ran.stderr.trim(),
                ran.stdout.trim()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ship::{PullProblem, PushProblem};
    use std::fs;

    fn sh(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap();
    }

    fn identity(dir: &Path) {
        for args in [
            &["config", "user.email", "test@hedy.local"][..],
            &["config", "user.name", "Test"],
            &["config", "commit.gpgsign", "false"],
        ] {
            sh(dir, args);
        }
    }

    /// A bare remote with one commit on `main`, and two clones of it.
    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("remoto.git");
        sh(
            tmp.path(),
            &["init", "-q", "--bare", "-b", "main", "remoto.git"],
        );
        let seed = tmp.path().join("seed");
        fs::create_dir(&seed).unwrap();
        sh(&seed, &["init", "-q", "-b", "main"]);
        identity(&seed);
        fs::write(seed.join("app.py"), "x = 1\n").unwrap();
        sh(&seed, &["add", "."]);
        sh(&seed, &["commit", "-qm", "Inicio"]);
        sh(&seed, &["remote", "add", "origin", &bare.to_string_lossy()]);
        sh(&seed, &["push", "-q", "-u", "origin", "main"]);
        let mine = tmp.path().join("mine");
        sh(
            tmp.path(),
            &["clone", "-q", &bare.to_string_lossy(), "mine"],
        );
        identity(&mine);
        (tmp, seed, mine)
    }

    fn commit(dir: &Path, file: &str, text: &str) {
        fs::write(dir.join(file), text).unwrap();
        sh(dir, &["add", "."]);
        sh(dir, &["commit", "-qm", file]);
    }

    #[test]
    fn status_counts_changes_and_ahead_behind() {
        let (_tmp, seed, mine) = setup();
        let g = GitSourceControl;
        let folder = g.folder(&mine).unwrap();
        assert_eq!(folder.branch.as_deref(), Some("main"));
        assert_eq!(folder.upstream.as_deref(), Some("origin/main"));
        assert_eq!((folder.ahead, folder.behind, folder.changed), (0, 0, 0));

        commit(&mine, "mio.py", "a\n");
        commit(&seed, "suyo.py", "b\n");
        sh(&seed, &["push", "-q"]);
        sh(&mine, &["fetch", "-q"]);
        fs::write(mine.join("app.py"), "x = 2\n").unwrap();
        fs::write(mine.join("nuevo.txt"), "n\n").unwrap();
        fs::write(mine.join("mio.py"), "a2\n").unwrap();
        sh(&mine, &["add", "mio.py"]);
        let folder = g.folder(&mine).unwrap();
        assert_eq!((folder.ahead, folder.behind), (1, 1));
        assert_eq!(
            (
                folder.staged,
                folder.unstaged,
                folder.untracked,
                folder.changed
            ),
            (1, 1, 1, 3)
        );
        let up = g.upstream(&mine, "main").unwrap().unwrap();
        assert_eq!(
            (up.name(), up.ahead, up.behind),
            ("origin/main".into(), 1, 1)
        );
        assert_eq!(g.remotes(&mine).unwrap(), ["origin"]);
        assert!(
            g.remote_url(&mine, "origin")
                .unwrap()
                .unwrap()
                .ends_with("remoto.git")
        );
        assert_eq!(g.default_branch(&mine, "origin").as_deref(), Some("main"));
    }

    #[test]
    fn pushes_set_an_upstream_and_rejections_are_not_forced() {
        let (_tmp, seed, mine) = setup();
        let g = GitSourceControl;
        sh(&mine, &["checkout", "-qb", "hedy/tarea"]);
        commit(&mine, "tarea.py", "t\n");
        assert_eq!(g.upstream(&mine, "hedy/tarea").unwrap(), None);
        g.push(&mine, "origin", "hedy/tarea", "hedy/tarea", true)
            .unwrap();
        let up = g.upstream(&mine, "hedy/tarea").unwrap().unwrap();
        assert_eq!((up.name(), up.ahead), ("origin/hedy/tarea".into(), 0));

        // Someone else pushed to main: the push is rejected, never forced.
        sh(&mine, &["checkout", "-q", "main"]);
        commit(&mine, "mio.py", "a\n");
        commit(&seed, "suyo.py", "b\n");
        sh(&seed, &["push", "-q"]);
        let error = g.push(&mine, "origin", "main", "main", false).unwrap_err();
        assert_eq!(
            PushProblem::from_git(&error.message),
            PushProblem::Rejected,
            "{}",
            error.message
        );
        // The remote still has the other commit.
        sh(&seed, &["fetch", "-q"]);
        assert!(
            git(&seed, &["log", "--format=%s", "origin/main"])
                .unwrap()
                .contains("suyo.py")
        );
    }

    #[test]
    fn pull_only_fast_forwards() {
        let (_tmp, seed, mine) = setup();
        let g = GitSourceControl;
        commit(&seed, "suyo.py", "b\n");
        sh(&seed, &["push", "-q"]);
        g.pull_ff_only(&mine).unwrap();
        assert!(mine.join("suyo.py").exists());

        commit(&seed, "otro.py", "c\n");
        sh(&seed, &["push", "-q"]);
        commit(&mine, "mio.py", "a\n");
        let before = git(&mine, &["rev-parse", "HEAD"]).unwrap();
        let error = g.pull_ff_only(&mine).unwrap_err();
        assert_eq!(
            PullProblem::from_git(&error.message),
            PullProblem::Diverged,
            "{}",
            error.message
        );
        assert_eq!(
            git(&mine, &["rev-parse", "HEAD"]).unwrap(),
            before,
            "nothing merged"
        );
    }

    #[test]
    fn a_kept_branch_survives_its_worktree_and_can_be_attached_again() {
        let (_tmp, _seed, mine) = setup();
        let g = GitSourceControl;
        let wt = mine.join(".hedy/worktrees/t1");
        sh(
            &mine,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "hedy/t1",
                &wt.to_string_lossy(),
            ],
        );
        commit(&wt, "tarea.py", "t\n");
        g.remove_worktree_keep_branch(&mine, &wt).unwrap();
        assert!(!wt.exists());
        assert!(
            git(&mine, &["log", "--format=%s", "hedy/t1"])
                .unwrap()
                .contains("tarea.py")
        );
        g.attach_worktree(&mine, &wt, "hedy/t1").unwrap();
        assert!(wt.join("tarea.py").exists());
    }

    #[cfg(unix)]
    #[test]
    fn gh_gets_the_body_on_stdin_and_its_login_is_only_an_exit_code() {
        use std::os::unix::fs::PermissionsExt as _;
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("log");
        let script = tmp.path().join("gh");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$@\" >> {log}\nif [ \"$1\" = auth ]; then [ \"$4\" = github.com ]; exit $?; fi\ncat >> {log}\necho https://github.com/a/b/pull/9\n",
                log = log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let gh = GhCli::with_program(script);
        assert!(gh.installed());
        assert!(gh.logged_in(tmp.path(), "github.com"));
        assert!(gh.logged_in(tmp.path(), "github.com"), "cached");
        assert!(!gh.logged_in(tmp.path(), "gitlab.com"));
        let out = gh
            .create_pull_request(
                tmp.path(),
                &PullRequestSpec {
                    title: "feat: x".into(),
                    body: "## Qué se pidió\n".into(),
                    base: "main".into(),
                    head: "hedy/x".into(),
                    draft: true,
                },
            )
            .unwrap();
        assert_eq!(out.trim(), "https://github.com/a/b/pull/9");
        let log = fs::read_to_string(&log).unwrap();
        assert_eq!(log.matches("auth status").count(), 2, "{log}");
        assert!(!log.contains("--show-token"));
        assert!(
            log.contains(
                "pr create --title feat: x --body-file - --base main --head hedy/x --draft"
            )
        );
        assert!(log.contains("## Qué se pidió"));
    }
}
