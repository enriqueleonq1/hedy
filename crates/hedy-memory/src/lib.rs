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
pub struct GetParams {
    /// The note's id, from memory_search.
    #[serde(default)]
    pub id: Option<i64>,
    /// Or its topic, such as arquitectura/pagos.
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProposeParams {
    /// The note, in one sentence.
    pub text: String,
    /// decision, convention, rule or preference.
    pub category: String,
    /// Optional stable topic, a lowercase path such as arquitectura/pagos.
    /// Under the topic of an existing note, the proposal is a change to it.
    #[serde(default)]
    pub topic: Option<String>,
}

/// Hedy's memory tools for one project and task, over MCP.
pub struct MemoryServer {
    tools: Arc<dyn MemoryTools>,
}

#[tool_router(server_handler)]
impl MemoryServer {
    #[tool(
        name = "memory_search",
        description = "Busca en la memoria del proyecto: decisiones, convenciones, reglas y preferencias aprobadas por el usuario. Devuelve un resumen por nota (id, categoría, tema y el comienzo del texto); memory_get devuelve una completa. Sin texto devuelve todas."
    )]
    fn memory_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        self.tools.search(&params.query)
    }

    #[tool(
        name = "memory_get",
        description = "Devuelve una nota completa de la memoria del proyecto, por id (de memory_search) o por tema."
    )]
    fn memory_get(&self, Parameters(params): Parameters<GetParams>) -> String {
        self.tools.get(params.id, params.topic.as_deref())
    }

    #[tool(
        name = "memory_topics",
        description = "Lista los temas de la memoria del proyecto (p. ej. arquitectura/pagos) con la nota de cada uno, para proponer cambios bajo el mismo tema."
    )]
    fn memory_topics(&self) -> String {
        self.tools.topics()
    }

    #[tool(
        name = "memory_propose",
        description = "Propone una nota para la memoria del proyecto (una decisión duradera o una convención). No se guarda hasta que el usuario la apruebe. Con el tema de una nota existente, propone un cambio de esa nota."
    )]
    fn memory_propose(&self, Parameters(params): Parameters<ProposeParams>) -> String {
        self.tools
            .propose(&params.text, &params.category, params.topic.as_deref())
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
    use std::sync::Mutex;

    use super::*;

    /// Records what the server asks the memory for.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<String>>);

    impl MemoryTools for Recorder {
        fn search(&self, query: &str) -> String {
            self.0.lock().unwrap().push(format!("search {query}"));
            "- #1 [decision · proyecto] pagos: Stripe".into()
        }
        fn get(&self, id: Option<i64>, topic: Option<&str>) -> String {
            self.0.lock().unwrap().push(format!("get {id:?} {topic:?}"));
            "#1 [decision · proyecto]\nStripe".into()
        }
        fn topics(&self) -> String {
            self.0.lock().unwrap().push("topics".into());
            "- pagos → #1: Stripe".into()
        }
        fn propose(&self, text: &str, category: &str, topic: Option<&str>) -> String {
            self.0
                .lock()
                .unwrap()
                .push(format!("propose {text} {category} {topic:?}"));
            "ok".into()
        }
    }

    fn parameters<T: serde::de::DeserializeOwned>(json: serde_json::Value) -> Parameters<T> {
        Parameters(serde_json::from_value(json).unwrap())
    }

    #[test]
    fn serves_the_memory_tools() {
        let names: Vec<String> = MemoryServer::tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        for name in [
            "memory_search",
            "memory_get",
            "memory_topics",
            "memory_propose",
        ] {
            assert!(names.iter().any(|n| n == name), "{name} missing: {names:?}");
        }
        let propose = MemoryServer::tool_router()
            .list_all()
            .into_iter()
            .find(|t| t.name == "memory_propose")
            .unwrap();
        let schema = serde_json::Value::Object((*propose.input_schema).clone());
        assert!(schema["properties"]["topic"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().all(|r| r != "topic"), "topic is optional");
    }

    #[test]
    fn tools_reach_the_memory() {
        let recorder = Arc::new(Recorder::default());
        let server = MemoryServer::new(recorder.clone());
        assert!(
            server
                .memory_search(parameters(serde_json::json!({})))
                .contains("#1")
        );
        server.memory_get(parameters(serde_json::json!({"id": 1})));
        server.memory_get(parameters(serde_json::json!({"topic": "pagos"})));
        assert!(server.memory_topics().contains("pagos"));
        // Older agents propose without a topic.
        server.memory_propose(parameters(
            serde_json::json!({"text": "Precios en USD", "category": "decision"}),
        ));
        server.memory_propose(parameters(serde_json::json!(
            {"text": "Pagos con Stripe", "category": "decision", "topic": "pagos"}
        )));
        assert_eq!(
            *recorder.0.lock().unwrap(),
            [
                "search ",
                "get Some(1) None",
                "get None Some(\"pagos\")",
                "topics",
                "propose Precios en USD decision None",
                "propose Pagos con Stripe decision Some(\"pagos\")",
            ]
        );
    }

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
