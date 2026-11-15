//! Port implementations and the composition root.

mod acp;
mod api_agent;
mod claude;
mod codex;
mod fs;
mod git;
mod git_code;
mod images;
mod keyring;
mod paths;
mod process;
mod pty;
mod ship;
mod sqlite;
mod usage_probe;

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
pub use ship::{GhCli, GitSourceControl};
pub use sqlite::SqliteStore;
pub use usage_probe::SystemUsageProbe;

use crate::application::ports::{
    AgentBackend, AgentLauncher, AgentRequest, AgentSession, Clock, EventSink, PortResult,
};
use crate::application::{Hedy, MemoryServerCommand, Ports};
use crate::domain::ImageSupport;
use crate::domain::plan_usage::WallTime;

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

    /// Claude Code's stream-json input takes image content blocks, Codex's
    /// app-server `localImage` items and the chat-completions APIs (Kimi
    /// K3, Gemini) `image_url` parts. ACP agents say so in `initialize`.
    fn image_support(&self, backend: &AgentBackend) -> ImageSupport {
        match backend {
            AgentBackend::ClaudeCode | AgentBackend::Codex => ImageSupport::Yes,
            AgentBackend::OpenAiCompatible { .. } => ImageSupport::Yes,
            AgentBackend::Acp { .. } => ImageSupport::IfAgentAllows,
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

    fn resolve_wall_time(&self, wall: &WallTime, after: i64) -> Option<i64> {
        resolve_wall_time(wall, after)
    }
}

/// See [`Clock::resolve_wall_time`]. Uses the system's time-zone database.
fn resolve_wall_time(wall: &WallTime, after: i64) -> Option<i64> {
    use jiff::civil::Date;
    use jiff::tz::TimeZone;

    let tz = match &wall.zone {
        Some(zone) if zone.eq_ignore_ascii_case("utc") => TimeZone::UTC,
        Some(zone) => TimeZone::get(zone).ok()?,
        None => TimeZone::system(),
    };
    let now = jiff::Timestamp::from_second(after)
        .ok()?
        .to_zoned(tz.clone());
    let earliest = after - 3600;
    let dates: Vec<Date> = match (wall.month, wall.day) {
        (Some(month), Some(day)) => [now.year(), now.year() + 1]
            .into_iter()
            .filter_map(|year| Date::new(year, month as i8, day as i8).ok())
            .collect(),
        _ => [now.date(), now.date().tomorrow().ok()?].to_vec(),
    };
    dates.into_iter().find_map(|date| {
        let at = date
            .at(wall.hour as i8, wall.minute as i8, 0, 0)
            .to_zoned(tz.clone())
            .ok()?
            .timestamp()
            .as_second();
        (at >= earliest).then_some(at)
    })
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
        fs: Arc::new(LocalFileSystem::new(
            paths.accounts_dir(),
            paths.chats_dir(),
        )),
        git: Arc::new(GitCli),
        code: Arc::new(GitCli),
        agents: Arc::new(AgentRouter {
            claude,
            codex: CodexLauncher,
            api: ApiAgentLauncher,
            acp: AcpLauncher,
        }),
        clock: Arc::new(SystemClock),
        usage: Arc::new(SystemUsageProbe),
        source: Arc::new(GitSourceControl),
        host: Arc::new(GhCli::default()),
    });
    // The setting is read from the store: a failure keeps the system language.
    if hedy.apply_language().is_err() {
        crate::i18n::init(None);
    }
    Ok(hedy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(month: Option<u8>, day: Option<u8>, hour: u8, zone: &str) -> WallTime {
        WallTime {
            month,
            day,
            hour,
            minute: 0,
            zone: Some(zone.into()),
        }
    }

    #[test]
    fn wall_times_resolve_to_the_next_occurrence() {
        // 2026-11-14 12:00 UTC.
        let now = 1_794_657_600;
        // Nov 14, 7pm in Caracas (UTC-4) = Nov 14, 23:00 UTC.
        let caracas = resolve_wall_time(&wall(Some(11), Some(14), 19, "America/Caracas"), now);
        assert_eq!(caracas, Some(now + 11 * 3600));
        // A date already past this year is next year's.
        let jan = resolve_wall_time(&wall(Some(1), Some(2), 0, "UTC"), now).unwrap();
        assert!(jan > now + 40 * 86_400);
        // Only a time: today if still ahead, otherwise tomorrow.
        assert_eq!(
            resolve_wall_time(&wall(None, None, 13, "UTC"), now),
            Some(now + 3600)
        );
        assert_eq!(
            resolve_wall_time(&wall(None, None, 9, "UTC"), now),
            Some(now + 21 * 3600)
        );
        assert_eq!(
            resolve_wall_time(&wall(Some(2), Some(30), 9, "UTC"), now),
            None
        );
        assert_eq!(
            resolve_wall_time(&wall(Some(11), Some(14), 9, "Mars/Olympus"), now),
            None
        );
    }
}
