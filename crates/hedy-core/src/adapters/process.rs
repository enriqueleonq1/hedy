use std::ffi::OsString;
use std::io::{ErrorKind, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::application::ports::{CapturedOutput, PortError, PortResult, ProcessRunner};
use crate::domain::LaunchSpec;
use crate::domain::env::is_blocked;

/// Starts official CLIs as child processes of the user's session.
pub struct SystemProcessRunner;

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
