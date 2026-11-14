//! `hedy-memory`: the project's shared memory as a stdio MCP server, so every
//! CLI agent (Claude Code, Codex) reads the same notes.
//!
//! Hedy starts one server per agent run, scoped to a project and a task.
//! Agents only see approved notes; what they write is a proposal the user
//! approves in the app (spec rule 6).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use hedy_core::adapters::{self, HedyPaths, MEMORY_SERVER_ARG, hook};
use hedy_core::application::ports::MemoryTools;
use hedy_core::domain::TaskId;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ServiceExt, schemars, tool, tool_router};
use serde::Deserialize;

/// What the server is scoped to; parsed from its command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub data_dir: PathBuf,
    pub project: PathBuf,
    pub task: Option<TaskId>,
    /// Who proposes notes, e.g. "Claude Opus".
    pub agent: String,
}

impl Options {
    /// `--data-dir D --project P [--task T] [--agent A]`.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let (mut data_dir, mut project, mut task, mut agent) = (None, None, None, None);
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            let value = args
                .next()
                .with_context(|| format!("falta el valor de {flag}"))?;
            match flag.as_str() {
                "--data-dir" => data_dir = Some(PathBuf::from(value)),
                "--project" => project = Some(PathBuf::from(value)),
                "--task" => task = Some(TaskId::new(value)),
                "--agent" => agent = Some(value),
                other => bail!("opción desconocida: {other}"),
            }
        }
        Ok(Self {
            data_dir: data_dir.context("falta --data-dir")?,
            project: project.context("falta --project")?,
            task,
            agent: agent.unwrap_or_else(|| "IA".into()),
        })
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Words to look for. Empty: every note.
    #[serde(default)]
    pub query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProposeParams {
    /// The note, in one sentence.
    pub text: String,
    /// decision, convention, rule or preference.
    pub category: String,
}

/// Hedy's memory tools for one project and task, over MCP.
pub struct MemoryServer {
    tools: Arc<dyn MemoryTools>,
}

#[tool_router(server_handler)]
impl MemoryServer {
    #[tool(
        name = "memory_search",
        description = "Busca en la memoria del proyecto: decisiones, convenciones, reglas y preferencias aprobadas por el usuario. Sin texto devuelve todas."
    )]
    fn memory_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        self.tools.search(&params.query)
    }

    #[tool(
        name = "memory_propose",
        description = "Propone una nota para la memoria del proyecto (una decisión duradera o una convención). No se guarda hasta que el usuario la apruebe."
    )]
    fn memory_propose(&self, Parameters(params): Parameters<ProposeParams>) -> String {
        self.tools.propose(&params.text, &params.category)
    }
}

impl MemoryServer {
    pub fn new(tools: Arc<dyn MemoryTools>) -> Self {
        Self { tools }
    }
}

/// Serves the memory over stdin/stdout until the agent closes it.
pub fn serve(options: Options) -> Result<()> {
    let hedy = adapters::system(&HedyPaths::at(options.data_dir.clone()))?;
    let tools = hedy.memory_tools(&options.project, options.task.as_ref(), &options.agent)?;
    let server = MemoryServer::new(tools);
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            server
                .serve(rmcp::transport::stdio())
                .await?
                .waiting()
                .await?;
            Ok(())
        })
}

/// Serves what Claude Code and the CLI agents start Hedy's binaries for: the
/// `PreToolUse` hook ([`hook::HOOK_ARGS`]) and the memory server
/// ([`MEMORY_SERVER_ARG`]). Returns the exit code when the process was started
/// as one of them; `None` when it should run as usual.
pub fn serve_helper_mode() -> Option<i32> {
    if std::env::args().skip(1).eq(hook::HOOK_ARGS) {
        return Some(hook::run());
    }
    if std::env::args().nth(1).as_deref() != Some(MEMORY_SERVER_ARG) {
        return None;
    }
    match Options::parse(std::env::args().skip(2)).and_then(serve) {
        Ok(()) => Some(0),
        Err(e) => {
            eprintln!("hedy-memory: {e:#}");
            Some(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_options() {
        let options = Options::parse(args(&[
            "--data-dir",
            "/d",
            "--project",
            "/p",
            "--task",
            "t-1",
            "--agent",
            "Codex",
        ]))
        .unwrap();
        assert_eq!(options.project, PathBuf::from("/p"));
        assert_eq!(options.task, Some(TaskId::new("t-1")));
        assert!(Options::parse(args(&["--project", "/p"])).is_err());
        assert!(
            Options::parse(args(&["--data-dir", "/d", "--project", "/p", "--x", "1"])).is_err()
        );
    }
}
