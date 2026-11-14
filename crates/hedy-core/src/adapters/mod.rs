//! Port implementations and the composition root.

mod acp;
mod api_agent;
mod claude;
mod codex;
mod fs;
mod git;
mod keyring;
mod paths;
mod process;
mod pty;
mod sqlite;

use std::sync::Arc;

pub use acp::AcpLauncher;
pub use api_agent::ApiAgentLauncher;
pub use claude::{ClaudeCodeLauncher, hook};
pub use codex::CodexLauncher;
pub use fs::LocalFileSystem;
pub use git::GitCli;
pub use keyring::KeyringSecretStore;
pub use paths::HedyPaths;
pub use process::SystemProcessRunner;
pub use process::find_program;
pub use pty::{PtyOutput, PtySession};
pub use sqlite::SqliteStore;

use crate::application::ports::{
    AgentBackend, AgentLauncher, AgentRequest, AgentSession, Clock, EventSink, PortResult,
};
use crate::application::{Hedy, MemoryServerCommand, Ports};

/// Sends each request to the adapter of its backend.
pub struct AgentRouter {
    pub claude: ClaudeCodeLauncher,
    pub codex: CodexLauncher,
    pub api: ApiAgentLauncher,
    pub acp: AcpLauncher,
}

impl AgentLauncher for AgentRouter {
    fn start(&self, request: AgentRequest, events: EventSink) -> PortResult<Box<dyn AgentSession>> {
        match request.backend {
            AgentBackend::ClaudeCode => self.claude.start(request, events),
            AgentBackend::Codex => self.codex.start(request, events),
            AgentBackend::OpenAiCompatible { .. } => self.api.start(request, events),
            AgentBackend::Acp { .. } => self.acp.start(request, events),
        }
    }
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }
}

/// Wires the real adapters together. Agents run without the `PreToolUse`
/// hook; use [`system_with_hook`] in binaries that can serve it.
pub fn system(paths: &HedyPaths) -> PortResult<Hedy> {
    build(paths, ClaudeCodeLauncher::default())
}

/// Like [`system`], with `program` (which runs [`hook::run`] when started with
/// [`hook::HOOK_ARGS`]) as Claude Code's `PreToolUse` hook.
pub fn system_with_hook(paths: &HedyPaths, program: &std::path::Path) -> PortResult<Hedy> {
    build(paths, ClaudeCodeLauncher::with_hook(program))
}

/// First argument that makes the app binary serve the memory MCP server.
pub const MEMORY_SERVER_ARG: &str = "memory-mcp";

/// The code-map MCP server's program (codebase-memory-mcp), looked up on `PATH`.
pub const CODE_MAP_PROGRAM: &str = "codebase-memory-mcp";

/// Like [`system_with_hook`], and `program` also serves the project memory
/// (`program memory-mcp --data-dir …`, see the `hedy-memory` crate).
pub fn system_for_app(paths: &HedyPaths, program: &std::path::Path) -> PortResult<Hedy> {
    Ok(
        system_with_hook(paths, program)?.with_memory_server(MemoryServerCommand {
            program: program.to_string_lossy().into_owned(),
            args: vec![
                MEMORY_SERVER_ARG.into(),
                "--data-dir".into(),
                paths.data_dir().to_string_lossy().into_owned(),
            ],
        }),
    )
}

fn build(paths: &HedyPaths, claude: ClaudeCodeLauncher) -> PortResult<Hedy> {
    let store = SqliteStore::open(&paths.database())?;
    let hedy = Hedy::new(Ports {
        store: Arc::new(store),
        secrets: Arc::new(KeyringSecretStore::default()),
        runner: Arc::new(SystemProcessRunner),
        fs: Arc::new(LocalFileSystem::new(paths.accounts_dir())),
        git: Arc::new(GitCli),
        agents: Arc::new(AgentRouter {
            claude,
            codex: CodexLauncher,
            api: ApiAgentLauncher,
            acp: AcpLauncher,
        }),
        clock: Arc::new(SystemClock),
    });
    // The setting is read from the store: a failure keeps the system language.
    if hedy.apply_language().is_err() {
        crate::i18n::init(None);
    }
    Ok(hedy)
}
