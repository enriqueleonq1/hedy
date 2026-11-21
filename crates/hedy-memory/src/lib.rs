//! `hedy-memory`: the project's shared memory as a stdio MCP server, so every
//! CLI agent (Claude Code, Codex) reads the same notes.
//!
//! Hedy starts one server per agent run, scoped to a project and a task.
//! Agents only see approved notes; what they write is a proposal the user
//! approves in the app (spec rule 6). The same server offers the learned
//! skills (`skill_list`, `skill_get`, `skill_propose`), with the same rule.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use hedy_core::adapters::{self, BOARD_SERVER_ARG, HedyPaths, MEMORY_SERVER_ARG, hook};
use hedy_core::application::ports::{BoardTools, MemoryTools, SkillTools};
use hedy_core::domain::TaskId;
use hedy_core::domain::context;
use hedy_core::domain::recall::PageRequest;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::JsonObject;
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

/// A tool's description, from Hedy's catalog (`domain::context`): today's
/// text, or the trimmed one with the `trim-tools` experiment.
fn description(name: &str) -> String {
    context::tool_definition(name)["description"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// A tool's input schema, from the same catalog (with `limit` / `offset`
/// under the `search-8` experiment).
fn schema(name: &str) -> Arc<JsonObject> {
    Arc::new(rmcp::model::object(
        context::tool_definition(name)["input_schema"].clone(),
    ))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetParams {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProposeParams {
    pub text: String,
    pub category: String,
    #[serde(default)]
    pub topic: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ListParams {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SkillGetParams {
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SkillProposeParams {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// Hedy's memory and skill tools for one project and task, over MCP. The
/// descriptions and schemas come from `domain::context`, so they are measured
/// with the rest of Hedy's fixed context (docs/decisions.md §33).
pub struct MemoryServer {
    tools: Arc<dyn MemoryTools>,
    skills: Arc<dyn SkillTools>,
}

#[tool_router(server_handler)]
impl MemoryServer {
    #[tool(
        name = "memory_search",
        description = description("memory_search"),
        input_schema = schema("memory_search")
    )]
    fn memory_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let page = PageRequest {
            limit: params.limit,
            offset: params.offset,
        };
        self.tools.search_page(&params.query, page)
    }

    #[tool(
        name = "memory_get",
        description = description("memory_get"),
        input_schema = schema("memory_get")
    )]
    fn memory_get(&self, Parameters(params): Parameters<GetParams>) -> String {
        self.tools.get(params.id, params.topic.as_deref())
    }

    #[tool(
        name = "memory_topics",
        description = description("memory_topics"),
        input_schema = schema("memory_topics")
    )]
    fn memory_topics(&self) -> String {
        self.tools.topics()
    }

    #[tool(
        name = "memory_propose",
        description = description("memory_propose"),
        input_schema = schema("memory_propose")
    )]
    fn memory_propose(&self, Parameters(params): Parameters<ProposeParams>) -> String {
        self.tools
            .propose(&params.text, &params.category, params.topic.as_deref())
    }

    #[tool(
        name = "skill_list",
        description = description("skill_list"),
        input_schema = schema("skill_list")
    )]
    fn skill_list(&self, Parameters(params): Parameters<ListParams>) -> String {
        self.skills.list_page(PageRequest {
            limit: params.limit,
            offset: params.offset,
        })
    }

    #[tool(
        name = "skill_get",
        description = description("skill_get"),
        input_schema = schema("skill_get")
    )]
    fn skill_get(&self, Parameters(params): Parameters<SkillGetParams>) -> String {
        self.skills.get(&params.name)
    }

    #[tool(
        name = "skill_propose",
        description = description("skill_propose"),
        input_schema = schema("skill_propose")
    )]
    fn skill_propose(&self, Parameters(params): Parameters<SkillProposeParams>) -> String {
        self.skills
            .propose(&params.name, &params.description, &params.body)
    }
}

impl MemoryServer {
    pub fn new(tools: Arc<dyn MemoryTools>, skills: Arc<dyn SkillTools>) -> Self {
        Self { tools, skills }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadParams {
    #[serde(default)]
    pub section: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PostParams {
    pub section: String,
    pub text: String,
}

/// One of the board's tools, as `domain::flow::board_tools` defines it.
fn board_tool(name: &str) -> serde_json::Value {
    hedy_core::domain::flow::board_tools()
        .into_iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("unknown board tool {name}"))
}

fn board_description(name: &str) -> String {
    board_tool(name)["description"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn board_schema(name: &str) -> Arc<JsonObject> {
    Arc::new(rmcp::model::object(
        board_tool(name)["input_schema"].clone(),
    ))
}

/// A flow task's blackboard (docs/decisions.md §42) over MCP, for the role
/// at work: `context_read` and `context_post`.
pub struct BoardServer {
    board: Arc<dyn BoardTools>,
}

#[tool_router(server_handler)]
impl BoardServer {
    #[tool(
        name = "context_read",
        description = board_description("context_read"),
        input_schema = board_schema("context_read")
    )]
    fn context_read(&self, Parameters(params): Parameters<ReadParams>) -> String {
        self.board.read(params.section.as_deref())
    }

    #[tool(
        name = "context_post",
        description = board_description("context_post"),
        input_schema = board_schema("context_post")
    )]
    fn context_post(&self, Parameters(params): Parameters<PostParams>) -> String {
        self.board.post(&params.section, &params.text)
    }
}

impl BoardServer {
    pub fn new(board: Arc<dyn BoardTools>) -> Self {
        Self { board }
    }
}

/// `--data-dir D --task T`: the board server's command line.
fn board_options(args: impl IntoIterator<Item = String>) -> Result<(PathBuf, TaskId)> {
    let (mut data_dir, mut task) = (None, None);
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .with_context(|| format!("falta el valor de {flag}"))?;
        match flag.as_str() {
            "--data-dir" => data_dir = Some(PathBuf::from(value)),
            "--task" => task = Some(TaskId::new(value)),
            other => bail!("opción desconocida: {other}"),
        }
    }
    Ok((
        data_dir.context("falta --data-dir")?,
        task.context("falta --task")?,
    ))
}

/// Serves a task's board over stdin/stdout until the agent closes it.
fn serve_board(data_dir: PathBuf, task: TaskId) -> Result<()> {
    let hedy = adapters::system(&HedyPaths::at(data_dir))?;
    let server = BoardServer::new(hedy.board_tools(&task)?);
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

/// Serves the memory over stdin/stdout until the agent closes it.
pub fn serve(options: Options) -> Result<()> {
    let hedy = adapters::system(&HedyPaths::at(options.data_dir.clone()))?;
    let tools = hedy.memory_tools(&options.project, options.task.as_ref(), &options.agent)?;
    let skills = hedy.skill_tools(&options.project, options.task.as_ref(), &options.agent)?;
    let server = MemoryServer::new(tools, skills);
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
    if std::env::args().nth(1).as_deref() == Some(BOARD_SERVER_ARG) {
        return Some(
            match board_options(std::env::args().skip(2)).and_then(|(d, t)| serve_board(d, t)) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("hedy-board: {e:#}");
                    1
                }
            },
        );
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
        fn search_page(&self, query: &str, page: PageRequest) -> String {
            let (limit, offset) = (page.limit, page.offset);
            self.0
                .lock()
                .unwrap()
                .push(format!("search_page {query} {limit:?} {offset:?}"));
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

    impl SkillTools for Recorder {
        fn list(&self) -> String {
            self.0.lock().unwrap().push("skill_list".into());
            "- run-tests [proyecto]: Antes de entregar".into()
        }
        fn list_page(&self, page: PageRequest) -> String {
            let (limit, offset) = (page.limit, page.offset);
            self.0
                .lock()
                .unwrap()
                .push(format!("skill_list {limit:?} {offset:?}"));
            "- run-tests [proyecto]: Antes de entregar".into()
        }
        fn get(&self, name: &str) -> String {
            self.0.lock().unwrap().push(format!("skill_get {name}"));
            "# run-tests".into()
        }
        fn propose(&self, name: &str, description: &str, body: &str) -> String {
            self.0
                .lock()
                .unwrap()
                .push(format!("skill_propose {name} | {description} | {body}"));
            "ok".into()
        }
    }

    fn parameters<T: serde::de::DeserializeOwned>(json: serde_json::Value) -> Parameters<T> {
        Parameters(serde_json::from_value(json).unwrap())
    }

    fn served(server_tools: Vec<rmcp::model::Tool>) -> Vec<serde_json::Value> {
        let mut served: Vec<serde_json::Value> = server_tools
            .into_iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": serde_json::Value::Object((*t.input_schema).clone()),
                })
            })
            .collect();
        served.sort_by_key(|v| v["name"].as_str().unwrap_or_default().to_owned());
        served
    }

    /// The only test that changes the process's experiments: every check
    /// that depends on them lives here, so parallel tests never race.
    #[test]
    fn follows_the_experiments_end_to_end() {
        use hedy_core::application::ports::ProjectRepository;
        use hedy_core::domain::{Category, Project};
        use hedy_core::experiments::{self, Experiment, Experiments};

        // The catalog `domain::context` measures is what the server serves.
        for set in [
            Experiments::NONE,
            Experiments::NONE.with(Experiment::Search8),
            context::trimmed(),
            Experiments::all(),
        ] {
            experiments::set(set);
            let mut expected = context::tool_definitions(set);
            expected.sort_by_key(|v| v["name"].as_str().unwrap_or_default().to_owned());
            assert_eq!(
                served(MemoryServer::tool_router().list_all()),
                expected,
                "{set:?}"
            );
        }

        // A real memory: 12 approved notes about prices.
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("tienda");
        std::fs::create_dir_all(&project).unwrap();
        let project = project.canonicalize().unwrap();
        let paths = HedyPaths::at(dir.path().join("data"));
        std::fs::create_dir_all(paths.data_dir()).unwrap();
        hedy_core::adapters::SqliteStore::open(&paths.database())
            .unwrap()
            .save_project(&Project::new(project.clone()))
            .unwrap();
        let hedy = adapters::system(&paths).unwrap();
        for i in 1..=12 {
            hedy.add_note(
                Some(&project),
                &format!("Precio {i} en USD"),
                Category::Decision,
            )
            .unwrap();
        }
        let tools = hedy.memory_tools(&project, None, "Codex").unwrap();
        let skills = hedy.skill_tools(&project, None, "Codex").unwrap();
        let server = MemoryServer::new(tools, skills);
        let lines = |text: &str| text.lines().filter(|l| l.starts_with("- #")).count();

        // Today's behavior: up to 30 notes, no paging line.
        experiments::set(Experiments::NONE);
        let today = server.memory_search(parameters(
            serde_json::json!({"query": "precio", "limit": 2}),
        ));
        assert_eq!(lines(&today), 12, "{today}");
        assert!(!today.contains("offset"));

        // search-8: 8 notes, then how to ask for the rest.
        experiments::set(Experiments::NONE.with(Experiment::Search8));
        let first = server.memory_search(parameters(serde_json::json!({"query": "precio"})));
        assert_eq!(lines(&first), 8, "{first}");
        assert!(
            first.ends_with("… 4 más: llama a memory_search con offset 8"),
            "{first}"
        );
        let rest = server.memory_search(parameters(
            serde_json::json!({"query": "precio", "offset": 8}),
        ));
        assert_eq!(lines(&rest), 4, "{rest}");
        assert!(!rest.contains("más:"));
        let two = server.memory_search(parameters(serde_json::json!({"limit": 2, "offset": 3})));
        assert_eq!(lines(&two), 2);
        assert!(
            two.contains("… 7 más: llama a memory_search con offset 5"),
            "{two}"
        );
        experiments::set(Experiments::NONE);
    }

    /// Records what the board server asks the board for.
    #[derive(Default)]
    struct Board(Mutex<Vec<String>>);

    impl BoardTools for Board {
        fn read(&self, section: Option<&str>) -> String {
            self.0.lock().unwrap().push(format!("read {section:?}"));
            "plan".into()
        }
        fn post(&self, section: &str, text: &str) -> String {
            self.0
                .lock()
                .unwrap()
                .push(format!("post {section} {text}"));
            "ok".into()
        }
    }

    #[test]
    fn serves_the_board_tools_and_they_reach_the_board() {
        let names: Vec<String> = BoardServer::tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(
            names.contains(&"context_read".to_owned())
                && names.contains(&"context_post".to_owned())
        );
        let board = Arc::new(Board::default());
        let server = BoardServer::new(board.clone());
        assert_eq!(
            server.context_read(parameters(serde_json::json!({"section": "plan"}))),
            "plan"
        );
        server.context_post(parameters(
            serde_json::json!({"section": "riesgos", "text": "firma"}),
        ));
        assert_eq!(
            *board.0.lock().unwrap(),
            ["read Some(\"plan\")", "post riesgos firma"]
        );
        assert!(board_options(args(&["--data-dir", "/d", "--task", "t1"])).is_ok());
        assert!(
            board_options(args(&["--data-dir", "/d"])).is_err(),
            "a task is required"
        );
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
            "skill_list",
            "skill_get",
            "skill_propose",
        ] {
            assert!(names.iter().any(|n| n == name), "{name} missing: {names:?}");
        }
        let skill_propose = MemoryServer::tool_router()
            .list_all()
            .into_iter()
            .find(|t| t.name == "skill_propose")
            .unwrap();
        let schema = serde_json::Value::Object((*skill_propose.input_schema).clone());
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r.as_str())
            .collect();
        assert_eq!(required.len(), 3, "{required:?}");
        for field in ["name", "description", "body"] {
            assert!(required.contains(&field), "{field}");
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
        let server = MemoryServer::new(recorder.clone(), recorder.clone());
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
        server.memory_search(parameters(
            serde_json::json!({"query": "pagos", "limit": 3, "offset": 8}),
        ));
        assert_eq!(
            *recorder.0.lock().unwrap(),
            [
                "search_page  None None",
                "get Some(1) None",
                "get None Some(\"pagos\")",
                "topics",
                "propose Precios en USD decision None",
                "propose Pagos con Stripe decision Some(\"pagos\")",
                "search_page pagos Some(3) Some(8)",
            ]
        );
    }

    #[test]
    fn skill_tools_reach_the_skills() {
        let recorder = Arc::new(Recorder::default());
        let server = MemoryServer::new(recorder.clone(), recorder.clone());
        assert!(
            server
                .skill_list(parameters(serde_json::json!({})))
                .contains("run-tests")
        );
        server.skill_list(parameters(serde_json::json!({"offset": 20})));
        server.skill_get(parameters(serde_json::json!({"name": "run-tests"})));
        server.skill_propose(parameters(serde_json::json!(
            {"name": "lint", "description": "Antes de commit", "body": "npm run lint"}
        )));
        assert_eq!(
            *recorder.0.lock().unwrap(),
            [
                "skill_list None None",
                "skill_list None Some(20)",
                "skill_get run-tests",
                "skill_propose lint | Antes de commit | npm run lint",
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
