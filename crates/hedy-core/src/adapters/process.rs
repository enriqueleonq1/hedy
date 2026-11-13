use std::io::ErrorKind;
use std::process::{Command, Stdio};

use crate::application::ports::{CapturedOutput, PortError, PortResult, ProcessRunner};
use crate::domain::LaunchSpec;
use crate::domain::env::is_blocked;

/// Starts official CLIs as child processes of the user's session.
pub struct SystemProcessRunner;

/// Builds the command with the inherited environment minus every blocked
/// variable, then the account's folder variable on top.
pub(crate) fn command(spec: &LaunchSpec) -> Command {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args);
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(is_blocked) {
            cmd.env_remove(&name);
        }
    }
    for (name, value) in &spec.env {
        cmd.env(name, value);
    }
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    cmd
}

fn launch_error(spec: &LaunchSpec, err: std::io::Error) -> PortError {
    if err.kind() == ErrorKind::NotFound {
        PortError::new(format!(
            "no se encontró «{}». Instálalo o indica su ruta con `hedy config set binary.<proveedor> <ruta>`",
            spec.program
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
        // Every blocked variable present in this process is explicitly removed.
        for (name, _) in std::env::vars_os() {
            if name.to_str().is_some_and(is_blocked) && name != "CLAUDE_CONFIG_DIR" {
                assert!(envs.contains(&(name.as_os_str(), None)), "{name:?}");
            }
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
