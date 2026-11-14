//! Terminal sessions shown with xterm.js. Each session is a PTY whose output is
//! streamed to the UI through a Tauri channel.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use hedy_core::adapters::PtySession;
use hedy_core::application::{AppResult, Hedy};
use hedy_core::domain::LaunchSpec;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

/// What to run in a new terminal.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalRequest {
    /// The official login flow of a subscription account.
    Login { account: String },
    /// The project's CLI with its account (like `hedy run`).
    Run { path: PathBuf },
    /// The user's shell, exporting the project account folder.
    Shell { path: Option<PathBuf> },
    /// Installs the code-map server with its official script, which checks the
    /// download's checksum. `--skip-config`: Hedy registers it per run instead
    /// of letting it edit every agent's config.
    InstallCodeMap,
}

pub const CODE_MAP_INSTALL: &str = "curl -fsSL https://raw.githubusercontent.com/DeusData/codebase-memory-mcp/main/install.sh | bash -s -- --skip-config";

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum TerminalEvent {
    /// Base64, because output chunks can split multi-byte characters.
    Data {
        data: String,
    },
    Exit {
        code: Option<u32>,
    },
}

impl TerminalRequest {
    pub fn spec(&self, hedy: &Hedy) -> AppResult<LaunchSpec> {
        match self {
            Self::Login { account } => hedy.login_spec(account),
            Self::Run { path } => Ok(hedy.resolve_run(path, None, vec![])?.spec),
            Self::Shell { path } => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
                let dir = path
                    .clone()
                    .or_else(|| std::env::var_os("HOME").map(Into::into))
                    .unwrap_or_else(|| "/".into());
                hedy.shell_spec(&dir, &shell)
            }
            Self::InstallCodeMap => Ok(LaunchSpec::program(
                "bash",
                vec!["-c".into(), CODE_MAP_INSTALL.into()],
            )),
        }
    }
}

type Session = Arc<Mutex<PtySession>>;

/// Open terminals. The map's lock is only held to look a session up, so a
/// terminal that blocks (a full PTY buffer) never stalls the others.
#[derive(Default)]
pub struct Terminals {
    next_id: AtomicU32,
    sessions: Arc<Mutex<HashMap<u32, Session>>>,
}

impl Terminals {
    pub fn open(
        &self,
        spec: &LaunchSpec,
        cols: u16,
        rows: u16,
        events: Channel<TerminalEvent>,
    ) -> Result<u32, String> {
        let (session, mut output) =
            PtySession::spawn(spec, cols, rows).map_err(|e| e.to_string())?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let session = Arc::new(Mutex::new(session));
        self.sessions
            .lock()
            .unwrap()
            .insert(id, Arc::clone(&session));

        let sessions = Arc::clone(&self.sessions);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match output.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let data = BASE64.encode(&buf[..n]);
                        if events.send(TerminalEvent::Data { data }).is_err() {
                            break;
                        }
                    }
                }
            }
            // The process ended (or was closed): forget it, then report how.
            sessions.lock().unwrap().remove(&id);
            let code = session
                .lock()
                .unwrap()
                .wait_exit(std::time::Duration::from_millis(500));
            let _ = events.send(TerminalEvent::Exit { code });
        });
        Ok(id)
    }

    fn with<T>(
        &self,
        id: u32,
        f: impl FnOnce(&mut PtySession) -> Result<T, String>,
    ) -> Result<T, String> {
        let session = self
            .sessions
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("la terminal {id} ya no existe"))?;
        let mut session = session.lock().unwrap();
        f(&mut session)
    }

    pub fn write(&self, id: u32, data: &str) -> Result<(), String> {
        self.with(id, |s| s.write(data.as_bytes()).map_err(|e| e.to_string()))
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        self.with(id, |s| s.resize(cols, rows).map_err(|e| e.to_string()))
    }

    /// Ends the process (if still running) and forgets the session. The
    /// reader thread then sees the end of the output and reports the exit.
    pub fn close(&self, id: u32) {
        let session = self.sessions.lock().unwrap().remove(&id);
        if let Some(session) = session {
            session.lock().unwrap().kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_from_ui_json() {
        let req: TerminalRequest =
            serde_json::from_str(r#"{"kind":"login","account":"claude-personal"}"#).unwrap();
        assert!(matches!(req, TerminalRequest::Login { account } if account == "claude-personal"));
        let req: TerminalRequest = serde_json::from_str(r#"{"kind":"shell","path":null}"#).unwrap();
        assert!(matches!(req, TerminalRequest::Shell { path: None }));
        let req: TerminalRequest = serde_json::from_str(r#"{"kind":"installCodeMap"}"#).unwrap();
        assert!(matches!(req, TerminalRequest::InstallCodeMap));
    }

    #[test]
    fn events_are_tagged() {
        let json = serde_json::to_value(TerminalEvent::Exit { code: Some(0) }).unwrap();
        assert_eq!(json["event"], "exit");
        assert_eq!(json["code"], 0);
    }
}
