//! Claude Code in headless mode: `claude -p` with stream-json both ways and
//! permission requests over stdio (docs/decisions.md §3).

pub mod hook;
mod translate;

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::process::{headless_command, launch_error, wait_unlocked};
use crate::application::ports::{
    AgentLauncher, AgentRequest, AgentSession, EventSink, PortError, PortResult,
};
use crate::domain::permissions::BlockedFiles;
use crate::domain::version::{CLAUDE_SHARED_CACHE_SINCE, parse_version, version_at_least};
use crate::domain::{AgentEvent, Phase, PromptImage, TaskMode, prompts};
use crate::experiments::{self, Experiment};
use crate::tr;
use translate::Translator;

/// `hook`: shell command for the `PreToolUse` hook (see [`hook`]); without it
/// only Claude Code's own deny rules protect files it reads without asking.
#[derive(Default)]
pub struct ClaudeCodeLauncher {
    hook: Option<String>,
    /// Each program's version, asked once (`claude --version`).
    versions: Mutex<HashMap<String, Option<String>>>,
}

impl ClaudeCodeLauncher {
    /// `program` must run [`hook::run`] when started with [`hook::HOOK_ARGS`].
    pub fn with_hook(program: &std::path::Path) -> Self {
        Self {
            hook: Some(hook::command(program)),
            ..Self::default()
        }
    }

    /// Whether `program` is a Claude Code at least `since`; `false` when its
    /// version cannot be read.
    fn at_least(&self, program: &str, since: &str) -> bool {
        let mut versions = self.versions.lock().unwrap();
        let version = versions.entry(program.to_owned()).or_insert_with(|| {
            let spec = crate::domain::LaunchSpec::program(program, vec!["--version".into()]);
            super::process::command(&spec)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| parse_version(&String::from_utf8_lossy(&out.stdout)))
        });
        version
            .as_deref()
            .is_some_and(|v| version_at_least(v, since))
    }
}

const READ_ONLY_TOOLS_DENIED: &str = "Edit,Write,MultiEdit,NotebookEdit";
/// Also denied when a request only reads (`no_shell`): the shell, and
/// subagents that would have one.
const SHELL_TOOLS_DENIED: &str = "Bash,Task";

/// With a folder of its own (`output`), a question may write there: Hedy
/// answers each write and refuses any outside it.
const OUTPUT_TOOLS_DENIED: &str = "NotebookEdit";

/// Where a question may write, when the hook is there to keep its writes in
/// that folder (Claude's own allow rules would skip Hedy's answer).
fn writable_output<'a>(
    request: &'a AgentRequest,
    hook: Option<&str>,
) -> Option<&'a std::path::PathBuf> {
    request.output.as_ref().filter(|_| hook.is_some())
}

/// The tools a read-only request may not use.
fn denied(request: &AgentRequest, hook: Option<&str>) -> String {
    let edits = if writable_output(request, hook).is_some() {
        OUTPUT_TOOLS_DENIED
    } else {
        READ_ONLY_TOOLS_DENIED
    };
    if request.no_shell {
        format!("{edits},{SHELL_TOOLS_DENIED}")
    } else {
        edits.to_owned()
    }
}

/// Where a question may write what it is asked for, said to the agent.
fn output_note(request: &AgentRequest, hook: Option<&str>) -> String {
    match writable_output(request, hook) {
        Some(folder) => {
            let folder = folder.display();
            tr!(
                " Si te piden un documento, una página u otro archivo, escríbelo en {folder} (fuera del proyecto); el usuario lo verá al terminar.",
                " If you are asked for a document, a page or another file, write it in {folder} (outside the project); the user will see it at the end."
            )
        }
        None => String::new(),
    }
}

/// The built-in tools a Hedy task uses (the `lean-tools` experiment), out of
/// the 22 Claude Code 2.1.306 loads in `-p` (cron, worktrees, workflows,
/// messaging, design sync…). `ToolSearch` loads MCP tools on demand; `Task`
/// runs subagents; `Skill` runs the account's skills.
const LEAN_TOOLS: &[&str] = &[
    "Task",
    "Bash",
    "Read",
    "Edit",
    "Write",
    "ToolSearch",
    "Skill",
];

/// Whether a session uses the 5-minute prompt cache: tasks run on without
/// pauses, so its cheaper writes pay off; a conversation keeps the hour, as
/// the user comes back after a while.
fn short_cache(request: &AgentRequest) -> bool {
    experiments::on(Experiment::Cache5m) && !request.conversation
}

/// Environment variables for this process's experiments (docs/decisions.md
/// §33), all read by Claude Code 2.1.306; older versions ignore them.
pub fn experiment_env(request: &AgentRequest) -> Vec<(&'static str, &'static str)> {
    let mut env = Vec::new();
    if experiments::on(Experiment::LeanPrompt) {
        env.extend([
            ("CLAUDE_CODE_SIMPLE_SYSTEM_PROMPT", "1"),
            ("CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS", "1"),
            ("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1"),
        ]);
    }
    if short_cache(request) {
        env.push(("CLAUDE_CODE_PROMPT_CACHE_TTL", "5m"));
    }
    if experiments::on(Experiment::ExploreHaiku) {
        env.extend([
            ("CLAUDE_CODE_SUBAGENT_MODEL", "haiku"),
            ("CLAUDE_CODE_SUBAGENT_MODEL_FORCE", "1"),
        ]);
    }
    env
}

/// `--tools` for the `lean-tools` experiment.
fn lean_tools(request: &AgentRequest, mode: TaskMode) -> String {
    let mut tools: Vec<&str> = LEAN_TOOLS.to_vec();
    if request.plan_first {
        tools.push("ExitPlanMode");
    }
    if mode == TaskMode::Research {
        tools.extend(["WebSearch", "WebFetch"]);
    }
    tools.join(",")
}

/// The project's blocked files as Claude Code's own `deny` rules. Claude runs
/// some tools without asking (Read, read-only shell commands, plan files), so
/// Hedy's permission rules alone would not see them; these the CLI enforces
/// itself. None when the project keeps nothing from the AI.
pub fn settings(hook: Option<&str>, blocked: &BlockedFiles) -> String {
    let rules: Vec<String> = blocked
        .patterns()
        .iter()
        .flat_map(|pattern| [format!("Read(**/{pattern})"), format!("Edit(**/{pattern})")])
        .collect();
    let mut settings = json!({"permissions": {"deny": rules}});
    if let Some(command) = hook {
        settings["hooks"] = json!({"PreToolUse": [{
            "matcher": hook::HOOK_MATCHER,
            "hooks": [{"type": "command", "command": command}],
        }]});
    }
    settings.to_string()
}

/// Command-line arguments for a request (after the program and its own args).
pub fn args(request: &AgentRequest, hook: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
        if request.plan_first {
            "plan"
        } else {
            "default"
        },
    ]
    .map(String::from)
    .to_vec();
    args.extend(["--settings".into(), settings(hook, &request.blocked)]);
    // Hedy's skills, for this session only: the user's own `claude` in the
    // same repository never sees them.
    for plugin in &request.plugins {
        args.extend(["--plugin-dir".into(), plugin.to_string_lossy().into_owned()]);
    }
    // Someone else's text and nothing to do but answer: no tool (not even
    // the web ones an account may allow), no MCP server and no skill.
    if request.no_tools {
        args.extend([
            "--tools".into(),
            String::new(),
            "--strict-mcp-config".into(),
            "--disable-slash-commands".into(),
        ]);
    }
    // A reviewer reads and comments, like a question.
    let mode = if request.phase == Phase::Review {
        TaskMode::Ask
    } else {
        request.mode
    };
    let mut allowed: Vec<String> = Vec::new();
    let mut system: Vec<String> = vec![prompts::reply_language()];
    // Its native plan mode asks for the plan; Hedy says how to write it.
    if request.plan_first {
        system.push(prompts::plan_format());
    }
    match mode {
        TaskMode::Project => system.push(prompts::copy_rules()),
        TaskMode::Ask => {
            args.extend(["--disallowedTools".into(), denied(request, hook)]);
            let note = output_note(request, hook);
            system.push(tr!(
                "Modo Pregunta de Hedy: responde y explica; no modifiques el proyecto.{note}",
                "Hedy's Ask mode: answer and explain; do not modify the project.{note}"
            ));
        }
        TaskMode::Research => {
            args.extend(["--disallowedTools".into(), denied(request, hook)]);
            allowed.extend(["WebSearch".into(), "WebFetch".into()]);
            let note = output_note(request, hook);
            system.push(tr!(
                "Modo Research de Hedy: investiga (también en la web), cita tus fuentes y no modifiques el proyecto.{note}",
                "Hedy's Research mode: investigate (on the web too), cite your sources and do not modify the project.{note}"
            ));
        }
    }
    if let Some(folder) = writable_output(request, hook) {
        args.extend(["--add-dir".into(), folder.to_string_lossy().into_owned()]);
    }
    let servers = request.mcp_servers();
    if !servers.is_empty() {
        let config: serde_json::Map<String, Value> = servers
            .iter()
            .map(|s| {
                let env: serde_json::Map<String, Value> =
                    s.env.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
                (
                    s.name.clone(),
                    json!({"type": "stdio", "command": s.command, "args": s.args, "env": env}),
                )
            })
            .collect();
        args.extend([
            "--mcp-config".into(),
            json!({ "mcpServers": config }).to_string(),
        ]);
        // Hedy's own servers only read the project (memory proposals wait for
        // the user), so their tools need no prompt.
        allowed.extend(servers.iter().map(|s| format!("mcp__{}", s.name)));
    }
    if let Some(memory) = &request.memory {
        system.push(prompts::memory_context(
            request.phase,
            request.mode,
            memory.team.as_deref(),
        ));
    }
    if !allowed.is_empty() {
        args.extend(["--allowedTools".into(), allowed.join(",")]);
    }
    if !system.is_empty() {
        args.extend(["--append-system-prompt".into(), system.join("\n\n")]);
    }
    if let Some(model) = &request.model {
        args.extend([
            "--model".into(),
            session_model(model, request.plan_first).to_owned(),
        ]);
    }
    if let Some(session) = &request.resume {
        args.extend(["--resume".into(), session.clone()]);
    }
    // A request with no tools keeps none, whatever the experiment.
    if experiments::on(Experiment::LeanTools) && !request.no_tools {
        args.extend(["--tools".into(), lean_tools(request, mode)]);
    }
    args
}

/// Claude Code runs another model in plan mode when the session's model is
/// the alias `haiku` (docs/decisions.md §59: haiku asked, Sonnet planned). A
/// task that plans first names that model by its id, which stays as asked.
const PLAN_MODE_IDS: [(&str, &str); 1] = [("haiku", "claude-haiku-5-5")];

/// The model a session is launched with.
fn session_model(model: &str, plan_first: bool) -> &str {
    PLAN_MODE_IDS
        .iter()
        .find(|(alias, _)| plan_first && model.eq_ignore_ascii_case(alias))
        .map_or(model, |(_, id)| id)
}

fn user_message(text: &str, images: &[PromptImage]) -> PortResult<Value> {
    let content = super::images::claude_content(text, images)?;
    Ok(
        json!({"type": "user", "session_id": "", "parent_tool_use_id": null,
           "message": {"role": "user", "content": content}}),
    )
}

/// State shared by the reader thread and the session handle.
#[derive(Default)]
struct Shared {
    /// Inputs of open requests: an "allow" must send them back.
    inputs: HashMap<String, Value>,
    /// request_id → tool_use_id, to hide the activity of denied tools.
    tool_use_of: HashMap<String, String>,
    denied: HashSet<String>,
}

struct Writer(Mutex<ChildStdin>);

impl Writer {
    fn send(&self, value: &Value) -> PortResult<()> {
        let mut line = value.to_string();
        line.push('\n');
        let mut stdin = self.0.lock().unwrap();
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| PortError::new(format!("Claude Code ya no responde: {e}")))
    }
}

pub struct ClaudeSession {
    writer: Arc<Writer>,
    child: Arc<Mutex<Child>>,
    shared: Arc<Mutex<Shared>>,
    stopped: Arc<AtomicBool>,
    turn_open: Arc<AtomicBool>,
}

impl AgentLauncher for ClaudeCodeLauncher {
    fn start(&self, request: AgentRequest, events: EventSink) -> PortResult<Box<dyn AgentSession>> {
        let mut spec = request.launch.clone();
        spec.args.extend(args(&request, self.hook.as_deref()));
        if short_cache(&request) && self.at_least(&spec.program, CLAUDE_SHARED_CACHE_SINCE) {
            spec.args
                .push("--exclude-dynamic-system-prompt-sections".into());
        }
        spec.env.extend(
            experiment_env(&request)
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.into())),
        );
        if let Some(folder) = writable_output(&request, self.hook.as_deref()) {
            spec.env.push((hook::OUTPUT_ENV.to_owned(), folder.clone()));
        }
        if !request.blocked.is_empty() {
            let list = serde_json::to_string(request.blocked.patterns()).unwrap_or_default();
            spec.env.push((hook::BLOCKED_ENV.to_owned(), list.into()));
        }
        let mut child = headless_command(&spec)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| launch_error(&spec, e))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        let session = ClaudeSession {
            writer: Arc::new(Writer(Mutex::new(stdin))),
            child: Arc::new(Mutex::new(child)),
            shared: Arc::default(),
            stopped: Arc::default(),
            turn_open: Arc::new(AtomicBool::new(true)),
        };

        let stderr_tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
        {
            let tail = Arc::clone(&stderr_tail);
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut tail = tail.lock().unwrap();
                    if tail.len() == 20 {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            });
        }
        {
            let cwd = spec.cwd.clone().unwrap_or_default();
            let (shared, stopped, turn_open, child) = (
                Arc::clone(&session.shared),
                Arc::clone(&session.stopped),
                Arc::clone(&session.turn_open),
                Arc::clone(&session.child),
            );
            std::thread::spawn(move || {
                read_events(stdout, cwd, &shared, &turn_open, &events);
                let code = wait_unlocked(&child);
                if !stopped.load(Ordering::SeqCst) && turn_open.load(Ordering::SeqCst) {
                    let detail = stderr_tail
                        .lock()
                        .unwrap()
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    let code = code.map_or("?".into(), |c| c.to_string());
                    let detail = detail.trim();
                    let message = tr!(
                        "Claude Code se detuvo (código {code}). {detail}",
                        "Claude Code stopped (code {code}). {detail}"
                    );
                    events(AgentEvent::Error {
                        message: message.clone(),
                    });
                    // The turn will not finish on its own: end the task's stage.
                    events(AgentEvent::Finished {
                        ok: false,
                        summary: message,
                        duration_ms: None,
                    });
                }
            });
        }

        session.writer.send(
            &json!({"type": "control_request", "request_id": translate::INIT_REQUEST_ID,
                    "request": {"subtype": "initialize"}}),
        )?;
        session
            .writer
            .send(&user_message(&request.prompt, &request.images)?)?;
        Ok(Box::new(session))
    }
}

fn read_events(
    stdout: impl Read,
    cwd: std::path::PathBuf,
    shared: &Mutex<Shared>,
    turn_open: &AtomicBool,
    events: &EventSink,
) {
    let mut translator = Translator::new(cwd);
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if msg["type"] == "control_request" && msg["request"]["subtype"] == "can_use_tool" {
            let id = msg["request_id"].as_str().unwrap_or("").to_owned();
            let mut shared = shared.lock().unwrap();
            shared
                .inputs
                .insert(id.clone(), msg["request"]["input"].clone());
            if let Some(tool_use) = msg["request"]["tool_use_id"].as_str() {
                shared.tool_use_of.insert(id, tool_use.to_owned());
            }
        }
        if msg["type"] == "result" {
            turn_open.store(false, Ordering::SeqCst);
        }
        let denied = shared.lock().unwrap().denied.clone();
        for event in translator.translate(&msg, &denied) {
            events(event);
        }
    }
}

impl AgentSession for ClaudeSession {
    fn send(&mut self, text: &str, images: &[PromptImage]) -> PortResult<()> {
        let message = user_message(text, images)?;
        self.turn_open.store(true, Ordering::SeqCst);
        self.writer.send(&message)
    }

    fn answer(&mut self, request_id: &str, allow: bool, message: Option<&str>) -> PortResult<()> {
        let response = {
            let mut shared = self.shared.lock().unwrap();
            let input = shared
                .inputs
                .remove(request_id)
                .unwrap_or_else(|| json!({}));
            if allow {
                json!({"behavior": "allow", "updatedInput": input})
            } else {
                if let Some(tool_use) = shared.tool_use_of.get(request_id).cloned() {
                    shared.denied.insert(tool_use);
                }
                json!({"behavior": "deny", "message": message.map_or_else(|| tr!("Rechazado en Hedy.", "Rejected in Hedy."), str::to_owned)})
            }
        };
        self.writer
            .send(&json!({"type": "control_response", "response": {
            "subtype": "success", "request_id": request_id, "response": response}}))
    }

    fn stop(&mut self) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            let mut child = self.child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ClaudeSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::application::ports::McpServer;
    use crate::domain::LaunchSpec;

    fn request(mode: TaskMode, plan_first: bool) -> AgentRequest {
        AgentRequest {
            images: Vec::new(),
            backend: crate::application::ports::AgentBackend::ClaudeCode,
            launch: LaunchSpec::program("claude", vec![]),
            mode,
            phase: if plan_first {
                Phase::Plan
            } else {
                Phase::Implement
            },
            plan_first,
            model: Some("opus".into()),
            prompt: "hola".into(),
            resume: Some("s-1".into()),
            memory: None,
            mcp: vec![],
            conversation: false,
            no_shell: false,
            no_tools: false,
            plugins: Vec::new(),
            output: None,
            blocked: BlockedFiles::suggested(),
            board: None,
        }
    }

    #[test]
    fn hedys_skills_come_as_a_plugin_for_the_session_only() {
        let mut request = request(TaskMode::Project, false);
        assert!(!args(&request, None).contains(&"--plugin-dir".to_owned()));
        request.plugins = vec![std::path::PathBuf::from("/data/skills-plugins/t-1")];
        let args = args(&request, None);
        let at = args.iter().position(|a| a == "--plugin-dir").unwrap();
        assert_eq!(args[at + 1], "/data/skills-plugins/t-1");
    }

    #[test]
    fn experiments_add_flags_and_variables_only_when_on() {
        use crate::experiments::{Experiments, with_experiments};
        let plain = args(&request(TaskMode::Project, false), None);
        assert!(!plain.contains(&"--tools".to_owned()));
        assert!(experiment_env(&request(TaskMode::Project, false)).is_empty());

        let set = Experiments::parse("lean-prompt,lean-tools,cache-5m,explore-haiku").0;
        let (lean, research, env) = with_experiments(set, || {
            (
                args(&request(TaskMode::Project, true), None),
                args(&request(TaskMode::Research, false), None),
                experiment_env(&request(TaskMode::Project, false)),
            )
        });
        let tools = |a: &[String]| a[a.iter().position(|x| x == "--tools").unwrap() + 1].clone();
        assert_eq!(
            tools(&lean),
            "Task,Bash,Read,Edit,Write,ToolSearch,Skill,ExitPlanMode"
        );
        assert!(tools(&research).ends_with("Skill,WebSearch,WebFetch"));
        for pair in [
            ("CLAUDE_CODE_SIMPLE_SYSTEM_PROMPT", "1"),
            ("CLAUDE_CODE_PROMPT_CACHE_TTL", "5m"),
            ("CLAUDE_CODE_SUBAGENT_MODEL", "haiku"),
            ("CLAUDE_CODE_SUBAGENT_MODEL_FORCE", "1"),
        ] {
            assert!(env.contains(&pair), "{pair:?}");
        }
        // A conversation keeps the hour-long cache.
        let chat = AgentRequest {
            conversation: true,
            ..request(TaskMode::Ask, false)
        };
        let env = with_experiments(set, || experiment_env(&chat));
        assert!(
            !env.iter()
                .any(|(name, _)| *name == "CLAUDE_CODE_PROMPT_CACHE_TTL")
        );
        assert!(env.contains(&("CLAUDE_CODE_SIMPLE_SYSTEM_PROMPT", "1")));
    }

    struct NoMemory;
    impl crate::application::ports::MemoryTools for NoMemory {
        fn search(&self, _: &str) -> String {
            String::new()
        }
        fn get(&self, _: Option<i64>, _: Option<&str>) -> String {
            String::new()
        }
        fn topics(&self) -> String {
            String::new()
        }
        fn propose(&self, _: &str, _: &str, _: Option<&str>) -> String {
            String::new()
        }
    }

    pub(crate) fn with_servers(mut request: AgentRequest) -> AgentRequest {
        let server = |name: &str, env: Vec<(String, String)>| McpServer {
            name: name.into(),
            command: "/opt/hedy".into(),
            args: vec!["memory-mcp".into(), "--project".into(), "/code/a b".into()],
            env,
        };
        request.memory = Some(crate::application::ports::MemoryLink {
            server: Some(server("hedy-memory", vec![])),
            tools: Arc::new(NoMemory),
            skills: None,
            team: None,
        });
        request.mcp = vec![server(
            "codebase-memory",
            vec![("CBM_ALLOWED_ROOT".into(), "/code/a b".into())],
        )];
        request
    }

    #[test]
    fn memory_and_code_map_are_registered_for_the_run() {
        let args = args(&with_servers(request(TaskMode::Research, false)), None);
        let value = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        let config: Value = serde_json::from_str(&value("--mcp-config")).unwrap();
        assert_eq!(config["mcpServers"]["hedy-memory"]["command"], "/opt/hedy");
        assert_eq!(
            config["mcpServers"]["codebase-memory"]["env"]["CBM_ALLOWED_ROOT"],
            "/code/a b"
        );
        assert_eq!(
            value("--allowedTools"),
            "WebSearch,WebFetch,mcp__hedy-memory,mcp__codebase-memory"
        );
        let system = value("--append-system-prompt");
        assert!(system.starts_with("Responde en español.") && system.contains("Modo Research"));
        assert!(system.contains("memory_propose"));
        assert_eq!(args.iter().filter(|a| *a == "--allowedTools").count(), 1);
    }

    #[test]
    fn a_task_that_plans_first_with_haiku_names_it_by_its_id() {
        let model = |model: &str, plan_first: bool| {
            let args = args(
                &AgentRequest {
                    model: Some(model.into()),
                    ..request(TaskMode::Project, plan_first)
                },
                None,
            );
            args[args.iter().position(|a| a == "--model").unwrap() + 1].clone()
        };
        assert_eq!(model("haiku", true), "claude-haiku-5-5");
        assert_eq!(model("Haiku", true), "claude-haiku-5-5");
        assert_eq!(model("haiku", false), "haiku");
        assert_eq!(model("sonnet", true), "sonnet");
        // Who picks opusplan wants Opus to plan.
        assert_eq!(model("opusplan", true), "opusplan");
    }

    #[test]
    fn arguments_per_mode() {
        let project = args(&request(TaskMode::Project, true), None).join(" ");
        assert!(project.contains("--permission-prompt-tool stdio"));
        assert!(project.contains(r#""Read(**/.env)""#), "{project}");
        assert!(project.contains(r#""Edit(**/*.pem)""#));
        assert!(project.contains("--permission-mode plan"));
        assert!(project.contains("Lo que supongo") || project.contains("What I assume"));
        assert!(
            !args(&request(TaskMode::Project, false), None)
                .join(" ")
                .contains("supongo")
        );
        assert!(project.ends_with("--model opus --resume s-1"));
        assert!(!project.contains("disallowedTools"));
        // A project that keeps nothing from the AI gives Claude no deny rule.
        let open = args(
            &AgentRequest {
                blocked: BlockedFiles::default(),
                ..request(TaskMode::Project, true)
            },
            None,
        )
        .join(" ");
        assert!(open.contains(r#"{"permissions":{"deny":[]}}"#), "{open}");

        let ask = args(&request(TaskMode::Ask, false), None).join(" ");
        assert!(ask.contains("--permission-mode default"));
        assert!(ask.contains("--disallowedTools Edit,Write,MultiEdit,NotebookEdit"));
        assert!(!ask.contains("Bash,Task"));
        assert!(!ask.contains("--add-dir"));
        // With a folder of its own, a question may write there only, and
        // only with the hook to keep it there.
        let with_output = AgentRequest {
            output: Some("/data/outputs/t1".into()),
            ..request(TaskMode::Ask, false)
        };
        let unguarded = args(&with_output, None).join(" ");
        assert!(unguarded.contains("--disallowedTools Edit,Write,MultiEdit,NotebookEdit"));
        assert!(!unguarded.contains("--add-dir"));
        let writing = args(&with_output, Some("'/bin/hedy' hook pre-tool-use"));
        let at = writing
            .iter()
            .position(|a| a == "--disallowedTools")
            .unwrap();
        assert_eq!(writing[at + 1], "NotebookEdit");
        let at = writing.iter().position(|a| a == "--add-dir").unwrap();
        assert_eq!(writing[at + 1], "/data/outputs/t1");
        assert!(writing.join(" ").contains("/data/outputs/t1 ("));
        let reading = args(
            &AgentRequest {
                no_shell: true,
                no_tools: false,
                ..request(TaskMode::Ask, false)
            },
            None,
        )
        .join(" ");
        assert!(reading.contains("--disallowedTools Edit,Write,MultiEdit,NotebookEdit,Bash,Task"));
        assert!(!reading.contains("--tools"));
        let bare = args(
            &AgentRequest {
                no_shell: true,
                no_tools: true,
                ..request(TaskMode::Ask, false)
            },
            None,
        );
        let at = bare.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(bare[at + 1], "", "no tool at all");
        assert!(bare.contains(&"--strict-mcp-config".to_owned()));
        assert_eq!(bare.iter().filter(|a| *a == "--tools").count(), 1);
        assert!(bare.contains(&"--disable-slash-commands".to_owned()));
        let review = args(
            &AgentRequest {
                phase: Phase::Review,
                ..request(TaskMode::Project, false)
            },
            None,
        )
        .join(" ");
        assert!(review.contains("--disallowedTools Edit,Write,MultiEdit,NotebookEdit"));
        let research = args(&request(TaskMode::Research, false), None).join(" ");
        assert!(research.contains("--allowedTools WebSearch,WebFetch"));
        assert!(!research.contains("PreToolUse"));

        let hooked = args(
            &request(TaskMode::Project, false),
            Some("'/bin/hedy' hook pre-tool-use"),
        );
        let settings: Value = serde_json::from_str(
            &hooked[hooked.iter().position(|a| a == "--settings").unwrap() + 1],
        )
        .unwrap();
        let entry = &settings["hooks"]["PreToolUse"][0];
        assert_eq!(entry["matcher"], hook::HOOK_MATCHER);
        assert_eq!(
            entry["hooks"][0]["command"],
            "'/bin/hedy' hook pre-tool-use"
        );
    }

    /// Plays Claude's side of the protocol: asks to edit a file, writes it only
    /// if allowed, then finishes the turn. Echoes its arguments as the model.
    const FAKE_CLAUDE: &str = r#"
import json, os, sys
def out(obj):
    print(json.dumps(obj), flush=True)
args = " ".join(sys.argv[1:])
for line in sys.stdin:
    msg = json.loads(line)
    if msg.get("type") == "control_request":
        out({"type": "control_response", "response": {"subtype": "success", "request_id": msg["request_id"]}})
        continue
    if msg.get("type") != "user":
        continue
    out({"type": "system", "subtype": "init", "session_id": "s-42", "model": args})
    path = os.path.join(os.getcwd(), "hola.txt")
    out({"type": "assistant", "message": {"content": [
        {"type": "text", "text": "Creo el archivo."},
        {"type": "tool_use", "id": "tu1", "name": "Write", "input": {"file_path": path, "content": "hola"}}]}})
    out({"type": "control_request", "request_id": "req-1", "request": {
        "subtype": "can_use_tool", "tool_name": "Write", "tool_use_id": "tu1",
        "input": {"file_path": path, "content": "hola"}}})
    answer = json.loads(sys.stdin.readline())["response"]["response"]
    if answer["behavior"] == "allow":
        open(answer["updatedInput"]["file_path"], "w").write(answer["updatedInput"]["content"])
        out({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "tu1", "content": "ok"}]}})
    else:
        out({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "tu1", "content": answer["message"], "is_error": True}]}})
    out({"type": "result", "subtype": "success", "is_error": False, "result": "Listo", "duration_ms": 5,
         "total_cost_usd": 0.01, "usage": {"input_tokens": 7, "output_tokens": 3}})
"#;

    fn start(dir: &std::path::Path) -> (Box<dyn AgentSession>, mpsc::Receiver<AgentEvent>) {
        let script = dir.join("fake_claude.py");
        std::fs::write(&script, FAKE_CLAUDE).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let launch = LaunchSpec::program(
            "python3",
            vec!["-I".into(), script.to_string_lossy().into_owned()],
        )
        .in_dir(dir.to_owned());
        let session = ClaudeCodeLauncher::default()
            .start(
                AgentRequest {
                    launch,
                    ..request(TaskMode::Project, false)
                },
                Arc::new(move |e| {
                    let _ = tx.lock().unwrap().send(e);
                }),
            )
            .unwrap();
        (session, rx)
    }

    fn next(rx: &mpsc::Receiver<AgentEvent>) -> AgentEvent {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("no event from fake claude")
    }

    fn until_permission(rx: &mpsc::Receiver<AgentEvent>) -> String {
        loop {
            if let AgentEvent::PermissionRequest { request_id, .. } = next(rx) {
                return request_id;
            }
        }
    }

    fn rest_of_turn(rx: &mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        loop {
            let e = next(rx);
            let done = matches!(e, AgentEvent::Finished { .. });
            events.push(e);
            if done {
                return events;
            }
        }
    }

    #[test]
    fn allowed_edit_happens_and_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let (mut session, rx) = start(&dir);

        let AgentEvent::SessionStarted { session_id, model } = next(&rx) else {
            panic!()
        };
        assert_eq!(session_id, "s-42");
        assert!(model.contains("--permission-prompt-tool stdio"), "{model}");
        assert!(matches!(next(&rx), AgentEvent::Commands { .. }));
        assert_eq!(
            next(&rx),
            AgentEvent::Message {
                text: "Creo el archivo.".into()
            }
        );

        let request = until_permission(&rx);
        session.answer(&request, true, None).unwrap();
        let rest = rest_of_turn(&rx);
        assert!(
            rest.contains(&AgentEvent::FileEdited {
                path: "hola.txt".into()
            }),
            "{rest:?}"
        );
        assert!(matches!(
            rest.last(),
            Some(AgentEvent::Finished { ok: true, .. })
        ));
        assert_eq!(
            std::fs::read_to_string(dir.join("hola.txt")).unwrap(),
            "hola"
        );
        session.stop();
    }

    #[test]
    fn denied_edit_does_not_happen_and_follow_ups_work() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let (mut session, rx) = start(&dir);

        let request = until_permission(&rx);
        session.answer(&request, false, Some("no")).unwrap();
        let rest = rest_of_turn(&rx);
        assert!(
            !rest
                .iter()
                .any(|e| matches!(e, AgentEvent::FileEdited { .. })),
            "{rest:?}"
        );
        assert!(!dir.join("hola.txt").exists());

        // Same process, second turn.
        session.send("otra vez", &[]).unwrap();
        let request = until_permission(&rx);
        session.answer(&request, true, None).unwrap();
        rest_of_turn(&rx);
        assert!(dir.join("hola.txt").exists());
        session.stop();
    }

    /// Lines `path` holds once it has at least `count` of them (waits a little).
    pub(crate) fn lines_when(path: &std::path::Path, count: usize) -> Vec<Value> {
        for _ in 0..100 {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let lines: Vec<Value> = text
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect();
            if lines.len() >= count {
                return lines;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("{} never had {count} lines", path.display());
    }

    #[test]
    fn images_go_as_content_blocks_of_the_user_message() {
        let tmp = tempfile::tempdir().unwrap();
        let image = crate::adapters::images::tests::png(tmp.path());
        let stdin = tmp.path().join("stdin.jsonl");
        let launch = LaunchSpec::program(
            "sh",
            vec!["-c".into(), format!("cat > '{}'", stdin.display())],
        );
        let mut session = ClaudeCodeLauncher::default()
            .start(
                AgentRequest {
                    launch,
                    images: vec![image.clone()],
                    ..request(TaskMode::Project, false)
                },
                Arc::new(|_| {}),
            )
            .unwrap();
        session.send("y esta", &[image]).unwrap();
        session.send("sin imagen", &[]).unwrap();
        let lines = lines_when(&stdin, 4);
        session.stop();
        let content = |i: usize| lines[i]["message"]["content"].clone();
        assert_eq!(lines[1]["type"], "user");
        assert_eq!(content(1)[0], json!({"type": "text", "text": "hola"}));
        assert_eq!(content(1)[1]["type"], "image");
        assert_eq!(content(1)[1]["source"]["type"], "base64");
        assert_eq!(content(1)[1]["source"]["media_type"], "image/png");
        assert!(
            content(1)[1]["source"]["data"]
                .as_str()
                .unwrap()
                .starts_with("iVBORw0KGgp4eA==")
        );
        assert_eq!(content(2)[0]["text"], "y esta");
        assert_eq!(content(2)[1]["type"], "image");
        // Without images the content stays a plain string, as before.
        assert_eq!(content(3), json!("sin imagen"));
    }
}
