use std::ffi::OsString;
use std::io::{ErrorKind, Read};
use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::application::ports::{CapturedOutput, PortError, PortResult, ProcessRunner};
use crate::domain::LaunchSpec;
use crate::domain::env::is_blocked;

/// Starts official CLIs as child processes of the user's session.
pub struct SystemProcessRunner;

/// Process groups of the checks running now, by whose they are (see
/// [`ProcessRunner::run_check`]).
static CHECKS: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

/// Whether bubblewrap runs here (installed, and user namespaces allowed).
fn bubblewrap() -> bool {
    static WORKS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WORKS.get_or_init(|| {
        cfg!(target_os = "linux")
            && Command::new("bwrap")
                .args(["--ro-bind", "/", "/", "--unshare-net", "true"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
    })
}

/// What a check may read in the home folder: the tools and their caches.
const HOME_TOOLS: &[&str] = &[
    ".cargo",
    ".rustup",
    ".npm",
    ".nvm",
    ".cache",
    ".local/bin",
    ".local/lib",
    ".pyenv",
    ".bun",
    ".deno",
    "go",
    ".gradle",
    ".m2",
    ".yarn",
    ".pnpm-store",
];
/// Files inside those that hold credentials: shown empty.
const HOME_SECRETS: &[&str] = &[".cargo/credentials", ".cargo/credentials.toml"];

/// The check inside bubblewrap: no network, no IPC, its own processes
/// (all gone when it ends), the system read-only, `/run` (the session bus,
/// the keyring's way in) and `/tmp` empty, the home folder empty but for
/// the tools ([`HOME_TOOLS`]), `reads` read-only and only the copy
/// writable. Killed with Hedy.
fn isolated(spec: &LaunchSpec, copy: &std::path::Path, reads: &[std::path::PathBuf]) -> Command {
    let mut cmd = Command::new("bwrap");
    cmd.args([
        "--die-with-parent",
        "--new-session",
        "--unshare-net",
        "--unshare-ipc",
        "--unshare-pid",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/run",
    ]);
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        cmd.arg("--tmpfs").arg(&home);
        for tool in HOME_TOOLS {
            let path = home.join(tool);
            cmd.arg("--ro-bind-try").arg(&path).arg(&path);
        }
        for secret in HOME_SECRETS {
            let path = home.join(secret);
            if path.is_file() {
                cmd.arg("--ro-bind").arg("/dev/null").arg(&path);
            }
        }
    }
    for path in reads {
        cmd.arg("--ro-bind-try").arg(path).arg(path);
    }
    cmd.arg("--bind").arg(copy).arg(copy);
    cmd.arg("--chdir").arg(copy);
    cmd.arg("--").arg(&spec.program).args(&spec.args);
    // Offline on purpose: say so to the tools, so they fail fast.
    cmd.env("CARGO_NET_OFFLINE", "true")
        .env("npm_config_offline", "true");
    cmd
}

/// The only variables a check gets: enough to find the tools and their
/// caches, nothing that identifies or bills anyone.
const CHECK_ENV: &[&str] = &["PATH", "HOME", "LANG", "LC_ALL", "TERM", "TMPDIR", "USER"];

/// Kills a check's whole process group (its `cargo`, `npm`…).
fn kill_check_group(pid: u32) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: `kill` takes no pointers; a group that is gone only makes
        // it fail.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// The inherited environment minus every blocked variable.
pub(crate) fn allowed_env() -> impl Iterator<Item = (OsString, OsString)> {
    std::env::vars_os().filter(|(name, _)| !name.to_str().is_some_and(is_blocked))
}

/// Builds the command with the inherited environment minus every blocked
/// variable, then the account's folder variable on top.
pub(crate) fn command(spec: &LaunchSpec) -> Command {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args);
    cmd.env_clear().envs(allowed_env());
    cmd.env(crate::i18n::ENV_VAR, crate::i18n::language().code());
    cmd.env(crate::domain::env::AGENT_SESSION, "1");
    // So the CLI's hooks run with Hedy's experiments (docs/decisions.md §33).
    let (name, value) = crate::experiments::child_env();
    cmd.env(name, value);
    for (name, value) in &spec.env {
        cmd.env(name, value);
    }
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    cmd
}

/// Like [`command`], for an agent driven over pipes (no terminal): it gets its
/// own process group, so a Ctrl+C in the terminal that runs Hedy (`hedy ask`)
/// reaches Hedy only, and Hedy stops the session cleanly.
pub(crate) fn headless_command(spec: &LaunchSpec) -> Command {
    let mut cmd = command(spec);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd
}

pub(crate) fn launch_error(spec: &LaunchSpec, err: std::io::Error) -> PortError {
    if err.kind() == ErrorKind::NotFound {
        let program = &spec.program;
        PortError::new(crate::tr!(
            "no se encontró «{program}». Instálalo o indica su ruta con `hedy config set binary.<proveedor> <ruta>`",
            "«{program}» was not found. Install it or set its path with `hedy config set binary.<provider> <path>`"
        ))
    } else {
        PortError::new(format!("no se pudo lanzar «{}»: {err}", spec.program))
    }
}

impl ProcessRunner for SystemProcessRunner {
    fn run_interactive(&self, spec: &LaunchSpec) -> PortResult<Option<i32>> {
        let status = command(spec).status().map_err(|e| launch_error(spec, e))?;
        Ok(status.code())
    }

    fn run_captured(&self, spec: &LaunchSpec) -> PortResult<CapturedOutput> {
        let output = command(spec)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| launch_error(spec, e))?;
        Ok(CapturedOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        })
    }

    fn run_captured_within(
        &self,
        spec: &LaunchSpec,
        timeout: Duration,
    ) -> PortResult<CapturedOutput> {
        let mut child = command(spec)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| launch_error(spec, e))?;
        // Read on another thread: a full pipe would block the child, and a
        // grandchild holding the pipe open must not block Hedy.
        let mut stdout = child.stdout.take().expect("stdout is piped");
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let (program, secs) = (&spec.program, timeout.as_secs_f32());
                    return Err(PortError::new(crate::tr!(
                        "«{program}» no respondió en {secs} s",
                        "«{program}» did not answer within {secs} s"
                    )));
                }
                Err(e) => return Err(launch_error(spec, e)),
            }
        };
        let bytes = output
            .recv_timeout(Duration::from_millis(500))
            .unwrap_or_default();
        Ok(CapturedOutput {
            exit_code: status.code(),
            stdout: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }

    fn run_check(
        &self,
        spec: &LaunchSpec,
        timeout: Duration,
        group: &str,
        reads: &[std::path::PathBuf],
    ) -> PortResult<CapturedOutput> {
        let mut cmd = match &spec.cwd {
            Some(copy) if bubblewrap() => isolated(spec, copy, reads),
            _ => {
                let mut cmd = Command::new(&spec.program);
                cmd.args(&spec.args);
                cmd
            }
        };
        let offline: Vec<_> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(|v| v.to_owned())))
            .collect();
        cmd.env_clear();
        for name in CHECK_ENV {
            if let Some(value) = std::env::var_os(name) {
                cmd.env(name, value);
            }
        }
        for (name, value) in offline.into_iter().filter_map(|(k, v)| Some((k, v?))) {
            cmd.env(name, value);
        }
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| launch_error(spec, e))?;
        let pid = child.id();
        CHECKS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((group.to_owned(), pid));
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(None) => {
                    kill_check_group(pid);
                    let _ = child.wait();
                    let (program, secs) = (&spec.program, timeout.as_secs_f32());
                    break Err(PortError::new(crate::tr!(
                        "«{program}» no terminó en {secs} s",
                        "«{program}» did not end within {secs} s"
                    )));
                }
                Err(e) => break Err(launch_error(spec, e)),
            }
        };
        CHECKS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(_, p)| *p != pid);
        // What it left behind in its group goes too.
        kill_check_group(pid);
        Ok(CapturedOutput {
            exit_code: status?.code(),
            stdout: String::new(),
        })
    }

    fn stop_checks(&self, group: &str) {
        CHECKS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(of, pid)| {
                let stop = of == group;
                if stop {
                    kill_check_group(*pid);
                }
                !stop
            });
    }

    fn isolates_checks(&self) -> bool {
        bubblewrap()
    }
    fn blocked_env_vars(&self) -> Vec<String> {
        let mut names: Vec<String> = std::env::vars_os()
            .filter_map(|(name, _)| name.into_string().ok())
            .filter(|name| is_blocked(name))
            .collect();
        names.sort();
        names
    }

    fn is_installed(&self, program: &str) -> bool {
        if program.contains('/') {
            return std::path::Path::new(program).is_file();
        }
        find_program(program).is_some()
    }

    fn spawn_detached(&self, spec: &LaunchSpec) -> PortResult<()> {
        let mut child = command(spec)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| launch_error(spec, e))?;
        // Reap it so it does not linger as a zombie in a long-running app.
        std::thread::spawn(move || child.wait());
        Ok(())
    }

    #[cfg(unix)]
    fn exec(&self, spec: &LaunchSpec) -> PortError {
        use std::os::unix::process::CommandExt;
        launch_error(spec, command(spec).exec())
    }

    #[cfg(not(unix))]
    fn exec(&self, spec: &LaunchSpec) -> PortError {
        match self.run_interactive(spec) {
            Ok(code) => std::process::exit(code.unwrap_or(1)),
            Err(e) => e,
        }
    }
}

/// Finds an installed program on `PATH` or in `~/.local/bin` (where many
/// installers put it before the shell picks up the new `PATH`).
pub fn find_program(name: &str) -> Option<std::path::PathBuf> {
    let local = std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/bin"));
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(local)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// Waits for `child` to exit without holding its lock in between, so another
/// thread can still kill it (a `wait()` under the lock would block `stop()`).
pub fn wait_unlocked(child: &std::sync::Mutex<std::process::Child>) -> Option<i32> {
    loop {
        match child.lock().unwrap().try_wait() {
            Ok(Some(status)) => return status.code(),
            Ok(None) => {}
            Err(_) => return None,
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn check(dir: &std::path::Path, script: &str, group: &str) -> Option<i32> {
        check_reading(dir, script, group, &[])
    }

    #[cfg(unix)]
    fn check_reading(
        dir: &std::path::Path,
        script: &str,
        group: &str,
        reads: &[std::path::PathBuf],
    ) -> Option<i32> {
        let spec = LaunchSpec {
            program: "sh".into(),
            args: vec!["-c".into(), script.into()],
            env: vec![],
            cwd: Some(dir.to_path_buf()),
        };
        SystemProcessRunner
            .run_check(&spec, Duration::from_secs(20), group, reads)
            .unwrap()
            .exit_code
    }

    #[cfg(unix)]
    #[test]
    fn a_check_gets_no_secrets_and_leaves_nothing_running() {
        let dir = tempfile::tempdir().unwrap();
        // Cargo sets this for the test process: a stand-in for any variable.
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
        assert_eq!(check(dir.path(), "env > env.txt", "t1"), Some(0));
        let env = std::fs::read_to_string(dir.path().join("env.txt")).unwrap();
        assert!(env.contains("PATH=") && !env.contains("CARGO_MANIFEST_DIR"));

        assert_eq!(
            check(dir.path(), "(sleep 2; touch late) & exit 0", "t1"),
            Some(0)
        );
        std::thread::sleep(Duration::from_secs(3));
        assert!(
            !dir.path().join("late").exists(),
            "its background child went too"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_isolated_check_writes_only_in_its_copy_and_has_no_network() {
        if !SystemProcessRunner.isolates_checks() {
            return; // no bubblewrap here
        }
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let probe = outside.path().join("probe");
        let script = format!("touch '{}'", probe.display());
        assert_ne!(check(dir.path(), &script, "t2"), Some(0));
        assert!(!probe.exists());
        assert_eq!(check(dir.path(), "touch inside", "t2"), Some(0));
        assert!(dir.path().join("inside").exists());
        assert_eq!(check(dir.path(), "test ! -e /run/user", "t2"), Some(0));
        let net = "exec 3<>/dev/tcp/1.1.1.1/80 2>/dev/null && echo up || echo down";
        assert_eq!(
            check(dir.path(), &format!("bash -c '{net}' > net.txt"), "t2"),
            Some(0)
        );
        let net = std::fs::read_to_string(dir.path().join("net.txt")).unwrap();
        assert_eq!(net.trim(), "down");

        // The home folder is empty but for the tools: no keys, no sessions.
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        for private in [".ssh", ".claude", ".gnupg", ".config"] {
            if home.join(private).exists() {
                let script = format!("test ! -e \"$HOME/{private}\"");
                assert_eq!(check(dir.path(), &script, "t2"), Some(0), "{private}");
            }
        }

        // A copy inside its project (under /tmp too) reads what it links to
        // there, read-only.
        let project = tempfile::tempdir().unwrap();
        let copy = project.path().join(".hedy/worktrees/t1");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::create_dir_all(project.path().join("node_modules")).unwrap();
        std::fs::write(project.path().join("node_modules/dep.js"), "ok").unwrap();
        std::os::unix::fs::symlink(
            project.path().join("node_modules"),
            copy.join("node_modules"),
        )
        .unwrap();
        let reads = [project.path().to_path_buf()];
        assert_eq!(
            check_reading(&copy, "grep -q ok node_modules/dep.js", "t2", &reads),
            Some(0)
        );
        assert_ne!(
            check_reading(&copy, "touch node_modules/new.js", "t2", &reads),
            Some(0)
        );
        assert!(!project.path().join("node_modules/new.js").exists());
    }

    #[test]
    fn removes_blocked_vars_and_sets_the_folder() {
        let spec = LaunchSpec {
            program: "claude".into(),
            args: vec![],
            env: vec![("CLAUDE_CONFIG_DIR".into(), "/acc".into())],
            cwd: None,
        };
        let cmd = command(&spec);
        let envs: Vec<_> = cmd.get_envs().collect();
        // No blocked variable is passed, except the folder Hedy sets itself.
        for (name, value) in &envs {
            let name = name.to_str().unwrap();
            assert!(
                value.is_none() || !is_blocked(name) || name == "CLAUDE_CONFIG_DIR",
                "{name}"
            );
        }
        assert!(envs.contains(&("CLAUDE_CONFIG_DIR".as_ref(), Some("/acc".as_ref()))));
        // Marked as Hedy's, so `hedy` refuses to approve from inside it.
        assert!(envs.contains(&("HEDY_AGENT_SESSION".as_ref(), Some("1".as_ref()))));
    }

    #[test]
    fn captures_output_and_exit_code() {
        let spec = LaunchSpec {
            program: "sh".into(),
            args: vec!["-c".into(), "echo \"$CODEX_HOME\"; exit 3".into()],
            env: vec![("CODEX_HOME".into(), "/acc/codex".into())],
            cwd: None,
        };
        let out = SystemProcessRunner.run_captured(&spec).unwrap();
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(out.stdout.trim(), "/acc/codex");
    }

    #[test]
    fn a_slow_process_is_killed_after_the_timeout() {
        let spec = LaunchSpec::program("sh", vec!["-c".into(), "echo 1.2.3".into()]);
        let out = SystemProcessRunner
            .run_captured_within(&spec, Duration::from_secs(5))
            .unwrap();
        assert_eq!((out.exit_code, out.stdout.trim()), (Some(0), "1.2.3"));

        let slow = LaunchSpec::program("sh", vec!["-c".into(), "sleep 10".into()]);
        let started = Instant::now();
        let err = SystemProcessRunner
            .run_captured_within(&slow, Duration::from_millis(200))
            .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(err.message.contains("sh"), "{err}");
    }

    #[test]
    fn missing_binary_has_a_helpful_error() {
        let spec = LaunchSpec {
            program: "hedy-no-such-binary".into(),
            args: vec![],
            env: vec![],
            cwd: None,
        };
        let err = SystemProcessRunner.run_captured(&spec).unwrap_err();
        assert!(err.message.contains("binary.<proveedor>"));
    }
}
