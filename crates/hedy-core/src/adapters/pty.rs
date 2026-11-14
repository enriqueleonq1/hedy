//! Terminal sessions for driving adapters that embed a terminal (the app).
//! Same environment rules as [`super::process`]: blocked variables never reach
//! the child.

use std::io::{Read, Write};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use super::process::allowed_env;
use crate::application::ports::{PortError, PortResult};
use crate::domain::LaunchSpec;

/// Bytes produced by the session; read it on its own thread until it ends.
pub type PtyOutput = Box<dyn Read + Send>;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

fn pty_error(action: &str, err: impl std::fmt::Display) -> PortError {
    PortError::new(format!("terminal: no se pudo {action}: {err}"))
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

impl PtySession {
    pub fn spawn(spec: &LaunchSpec, cols: u16, rows: u16) -> PortResult<(Self, PtyOutput)> {
        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(|e| pty_error("abrir", e))?;

        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.env_clear();
        for (name, value) in allowed_env() {
            cmd.env(name, value);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (name, value) in &spec.env {
            cmd.env(name, value);
        }
        // portable-pty would start in $HOME otherwise; make it explicit.
        if let Some(cwd) = spec
            .cwd
            .clone()
            .or_else(|| std::env::var_os("HOME").map(Into::into))
        {
            cmd.cwd(cwd);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| pty_error(&format!("lanzar «{}»", spec.program), e))?;
        drop(pair.slave);
        let output = pair
            .master
            .try_clone_reader()
            .map_err(|e| pty_error("leer", e))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| pty_error("escribir", e))?;
        Ok((
            Self {
                master: pair.master,
                writer,
                child,
            },
            output,
        ))
    }

    pub fn write(&mut self, data: &[u8]) -> PortResult<()> {
        self.writer
            .write_all(data)
            .and_then(|()| self.writer.flush())
            .map_err(|e| pty_error("escribir", e))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> PortResult<()> {
        self.master
            .resize(size(cols, rows))
            .map_err(|e| pty_error("cambiar el tamaño", e))
    }

    /// Exit code once the process has ended.
    pub fn exit_code(&mut self) -> Option<u32> {
        self.child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.exit_code())
    }

    /// Exit code, waiting up to `timeout`: output can end slightly before the
    /// process does.
    pub fn wait_exit(&mut self, timeout: std::time::Duration) -> Option<u32> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(code) = self.exit_code() {
                return Some(code);
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if self.exit_code().is_none() {
            self.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_with_account_folder_and_without_blocked_vars() {
        let spec = LaunchSpec {
            program: "sh".into(),
            args: vec![
                "-c".into(),
                "printf '%s|%s|%s' \"$CODEX_HOME\" \"${OPENAI_API_KEY:-none}\" \"$PWD\"".into(),
            ],
            env: vec![("CODEX_HOME".into(), "/acc/codex".into())],
            cwd: Some("/tmp".into()),
        };
        let (mut session, mut output) = PtySession::spawn(&spec, 80, 24).unwrap();
        let mut text = String::new();
        let _ = output.read_to_string(&mut text);
        assert!(text.contains("/acc/codex|none|/tmp"), "{text:?}");
        assert_eq!(
            session.wait_exit(std::time::Duration::from_secs(5)),
            Some(0)
        );
    }

    #[test]
    fn accepts_input_and_resize() {
        let spec = LaunchSpec::program("sh", vec!["-c".into(), "read x; echo got:$x".into()]);
        let (mut session, mut output) = PtySession::spawn(&spec, 80, 24).unwrap();
        session.resize(100, 30).unwrap();
        session.write(b"hola\n").unwrap();
        let mut text = String::new();
        let _ = output.read_to_string(&mut text);
        assert!(text.contains("got:hola"), "{text:?}");
    }
}
