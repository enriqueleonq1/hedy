//! Agents that speak the Agent Client Protocol (ACP v1: JSON-RPC 2.0 over
//! stdio, one JSON per line): Gemini CLI (`gemini --acp`), Kimi Code
//! (`kimi acp`) and Antigravity (`agy acp`). Fields follow the official
//! schema (`@agentclientprotocol/sdk` 2.7, `schema/v1`; docs/decisions.md §14).
//!
//! Hedy offers no file system or terminal to the agent: it uses its own tools
//! and asks through `session/request_permission`, which become Hedy's cards.
//! ACP has no plan-approval step, so `plan_first` is emulated as with Codex: a
//! turn that only proposes a plan while edits and commands are refused.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{Value, json};

use super::paths::relative;
use super::process::{headless_command, launch_error, wait_unlocked};
use crate::application::ports::{
    AgentBackend, AgentLauncher, AgentRequest, AgentSession, EventSink, PortError, PortResult,
};
use crate::domain::agent::looks_like_tests;
use crate::domain::command::slash_command;
use crate::domain::{
    AgentCommand, AgentEvent, CommandSource, Decision, PlanStep, PromptImage, StepStatus,
    UsageSample, prompts,
};
use crate::tr;

/// The protocol version Hedy speaks.
const PROTOCOL_VERSION: u64 = 1;

pub struct AcpLauncher;

/// What a response to one of our requests means.
enum Pending {
    Initialize,
    Session,
    Prompt,
}

/// A user turn: text and the images that go with it.
#[derive(Debug, Clone, Default)]
struct Turn {
    text: String,
    images: Vec<PromptImage>,
}

impl Turn {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            images: Vec::new(),
        }
    }
}

/// A tool call as the agent describes it, kept until it completes.
#[derive(Default, Clone)]
struct ToolCall {
    kind: String,
    title: String,
    paths: Vec<String>,
    raw_input: Value,
    started: bool,
    reported: bool,
}

struct State {
    /// Shown in messages: "Gemini CLI", "Kimi Code", "Antigravity".
    name: String,
    next_id: u64,
    pending: HashMap<u64, Pending>,
    session: Option<String>,
    cwd: PathBuf,
    mcp_servers: Value,
    resume: Option<String>,
    /// Replayed history of a loaded session is not shown again.
    loading: bool,
    read_only: bool,
    /// The current turn only proposes a plan.
    planning: bool,
    plans: u32,
    first_prompt: Option<Turn>,
    /// The agent's `promptCapabilities.image`: without it, images are not sent.
    takes_images: bool,
    /// The agent reads PDFs (its backend says so) and advertised
    /// `promptCapabilities.embeddedContext`: without both, PDFs are not sent.
    takes_pdf: bool,
    reads_pdf: bool,
    /// A prompt is running; further messages wait in `queued`.
    in_turn: bool,
    turn_started: Option<Instant>,
    queued: VecDeque<Turn>,
    message: String,
    last_message: String,
    tools: HashMap<String, ToolCall>,
    /// Permission request ids (as sent to the UI) → (JSON-RPC id, options).
    approvals: HashMap<String, (Value, Vec<Value>)>,
    /// Last cumulative cost reported, to record only what each turn added.
    cost_usd: f64,
}

struct Shared {
    stdin: Mutex<ChildStdin>,
    state: Mutex<State>,
    events: EventSink,
}

impl Shared {
    fn write(&self, value: &Value) -> PortResult<()> {
        let mut line = value.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().unwrap();
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| {
                PortError::new(tr!(
                    "el agente ya no responde: {e}",
                    "the agent no longer answers: {e}"
                ))
            })
    }

    fn request(&self, method: &str, params: Value, kind: Pending) -> PortResult<()> {
        let id = {
            let mut s = self.state.lock().unwrap();
            s.next_id += 1;
            let id = s.next_id;
            s.pending.insert(id, kind);
            id
        };
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    }

    fn emit(&self, event: AgentEvent) {
        (self.events)(event);
    }

    /// Ends the stage with an error the user can act on.
    fn fail(&self, message: String) {
        self.emit(AgentEvent::Error {
            message: message.clone(),
        });
        self.emit(AgentEvent::Finished {
            ok: false,
            summary: message,
            duration_ms: None,
        });
    }

    /// Sends a prompt now, or after the running one. Images and PDFs only
    /// go to an agent that said it takes them; otherwise the user is told
    /// and only the text goes.
    fn prompt(&self, turn: Turn) -> PortResult<()> {
        let (session, takes_images, takes_pdf, name) = {
            let mut s = self.state.lock().unwrap();
            if s.in_turn || s.session.is_none() {
                s.queued.push_back(turn);
                return Ok(());
            }
            s.in_turn = true;
            s.turn_started = Some(Instant::now());
            (
                s.session.clone().unwrap_or_default(),
                s.takes_images,
                s.takes_pdf,
                s.name.clone(),
            )
        };
        let (pdfs, images): (Vec<PromptImage>, Vec<PromptImage>) =
            turn.images.into_iter().partition(|i| i.media_type.is_pdf());
        if !takes_images && !images.is_empty() {
            self.emit(AgentEvent::Error {
                message: tr!(
                    "{name} no acepta imágenes: se envió solo el texto.",
                    "{name} does not take images: only the text was sent."
                ),
            });
        }
        if !takes_pdf && !pdfs.is_empty() {
            self.emit(AgentEvent::Error {
                message: tr!(
                    "{name} no lee PDF: se envió sin ellos.",
                    "{name} does not read PDFs: the message went without them."
                ),
            });
        }
        let sent: Vec<PromptImage> = (if takes_images { images } else { Vec::new() })
            .into_iter()
            .chain(if takes_pdf { pdfs } else { Vec::new() })
            .collect();
        let prompt = match super::images::acp_prompt(&turn.text, &sent) {
            Ok(prompt) => prompt,
            Err(e) => {
                self.state.lock().unwrap().in_turn = false;
                return Err(e);
            }
        };
        self.request(
            "session/prompt",
            json!({"sessionId": session, "prompt": prompt}),
            Pending::Prompt,
        )
    }

    /// Emits the message streamed so far, if any.
    fn flush_message(&self) {
        let text = {
            let mut s = self.state.lock().unwrap();
            let text = std::mem::take(&mut s.message);
            if !text.trim().is_empty() {
                s.last_message = text.trim().to_owned();
            }
            text
        };
        if !text.trim().is_empty() {
            self.emit(AgentEvent::Message {
                text: text.trim().to_owned(),
            });
        }
    }
}

pub struct AcpSession {
    shared: Arc<Shared>,
    child: Arc<Mutex<Child>>,
    stopped: Arc<AtomicBool>,
}

impl AgentLauncher for AcpLauncher {
    fn start(&self, request: AgentRequest, events: EventSink) -> PortResult<Box<dyn AgentSession>> {
        let AgentBackend::Acp {
            name,
            model_flag,
            reads_pdf,
        } = &request.backend
        else {
            return Err(PortError::new("este agente no habla ACP"));
        };
        let mut spec = request.launch.clone();
        if let (Some(flag), Some(model)) = (model_flag, &request.model) {
            spec.args.splice(0..0, [flag.to_string(), model.clone()]);
        }
        let mut child = headless_command(&spec)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| launch_error(&spec, e))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");

        let mcp_servers: Vec<Value> = request
            .mcp_servers()
            .into_iter()
            .map(|s| {
                let env: Vec<Value> = s
                    .env
                    .iter()
                    .map(|(name, value)| json!({"name": name, "value": value}))
                    .collect();
                json!({"name": s.name, "command": s.command, "args": s.args, "env": env})
            })
            .collect();
        // ACP v1 has no stable system prompt: the guidance opens the first prompt.
        let mut intro = vec![prompts::reply_language()];
        if request.phase.read_only_in(request.mode) {
            intro.push(tr!(
                "Modo de solo lectura de Hedy: responde e investiga; no modifiques archivos.",
                "Hedy's read-only mode: answer and investigate; do not modify files."
            ));
        }
        if let Some(memory) = &request.memory {
            intro.push(prompts::memory_context(
                request.phase,
                request.mode,
                memory.team.as_deref(),
            ));
        }
        if request.plan_first {
            intro.push(prompts::plan_first());
        }
        intro.push(request.prompt.clone());
        // A slash command only runs when the prompt starts with it, so it
        // goes alone; Hedy's checks still refuse what the mode forbids.
        if slash_command(&request.prompt).is_some() {
            intro = vec![request.prompt.clone()];
        }

        let shared = Arc::new(Shared {
            stdin: Mutex::new(stdin),
            state: Mutex::new(State {
                name: name.clone(),
                next_id: 0,
                pending: HashMap::new(),
                session: None,
                cwd: spec.cwd.clone().unwrap_or_default(),
                mcp_servers: Value::Array(mcp_servers),
                resume: request.resume.clone(),
                loading: false,
                read_only: request.phase.read_only_in(request.mode),
                planning: request.plan_first,
                plans: 0,
                first_prompt: Some(Turn {
                    text: intro.join("\n\n"),
                    images: request.images.clone(),
                }),
                takes_images: false,
                takes_pdf: false,
                reads_pdf: *reads_pdf,
                in_turn: false,
                turn_started: None,
                queued: VecDeque::new(),
                message: String::new(),
                last_message: String::new(),
                tools: HashMap::new(),
                approvals: HashMap::new(),
                cost_usd: 0.0,
            }),
            events,
        });
        let child = Arc::new(Mutex::new(child));
        let stopped = Arc::new(AtomicBool::new(false));
        {
            let (shared, child, stopped) = (
                Arc::clone(&shared),
                Arc::clone(&child),
                Arc::clone(&stopped),
            );
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Ok(msg) = serde_json::from_str::<Value>(&line)
                        && let Err(e) = handle(&shared, &msg)
                    {
                        shared.emit(AgentEvent::Error { message: e.message });
                    }
                }
                let code = wait_unlocked(&child);
                if !stopped.load(Ordering::SeqCst) {
                    let name = shared.state.lock().unwrap().name.clone();
                    let code = code.map_or("?".into(), |c| c.to_string());
                    shared.fail(tr!(
                        "{name} se detuvo (código {code}).",
                        "{name} stopped (code {code})."
                    ));
                }
            });
        }
        shared.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false,
                },
                "clientInfo": {"name": "hedy", "title": "Hedy", "version": env!("CARGO_PKG_VERSION")},
            }),
            Pending::Initialize,
        )?;
        Ok(Box::new(AcpSession {
            shared,
            child,
            stopped,
        }))
    }
}

fn handle(shared: &Shared, msg: &Value) -> PortResult<()> {
    match (msg["method"].as_str(), msg.get("id")) {
        (None, Some(id)) => response(shared, id.as_u64().unwrap_or(0), msg),
        (Some(method), Some(id)) => client_request(shared, method, id.clone(), &msg["params"]),
        (Some("session/update"), None) => update(shared, &msg["params"]["update"]),
        _ => Ok(()),
    }
}

/// Why the agent refused: a login problem gets a hint the user can act on.
fn error_message(name: &str, error: &Value) -> String {
    let text = error["message"].as_str().unwrap_or("error");
    if text.to_lowercase().contains("auth") {
        tr!(
            "{name} no tiene la sesión iniciada: inicia sesión desde Cuentas y modelos.",
            "{name} is not logged in: log in from Accounts and models."
        )
    } else {
        format!("{name}: {text}")
    }
}

fn response(shared: &Shared, id: u64, msg: &Value) -> PortResult<()> {
    let (kind, name) = {
        let mut s = shared.state.lock().unwrap();
        (s.pending.remove(&id), s.name.clone())
    };
    if let Some(error) = msg.get("error") {
        let message = error_message(&name, error);
        match kind {
            // Without a session no turn ever starts; a failed turn ends too.
            Some(Pending::Initialize | Pending::Session | Pending::Prompt) => {
                shared.state.lock().unwrap().in_turn = false;
                shared.fail(message);
            }
            None => shared.emit(AgentEvent::Error { message }),
        }
        return Ok(());
    }
    let result = &msg["result"];
    match kind {
        Some(Pending::Initialize) => {
            let version = result["protocolVersion"].as_u64().unwrap_or(0);
            if version != PROTOCOL_VERSION {
                shared.fail(tr!(
                    "{name} habla ACP versión {version}; Hedy usa la {PROTOCOL_VERSION}.",
                    "{name} speaks ACP version {version}; Hedy uses {PROTOCOL_VERSION}."
                ));
                return Ok(());
            }
            let can_load = result["agentCapabilities"]["loadSession"]
                .as_bool()
                .unwrap_or(false);
            // Omitted capabilities are unsupported (ACP initialization).
            let capabilities = &result["agentCapabilities"]["promptCapabilities"];
            let takes_images = capabilities["image"].as_bool().unwrap_or(false);
            let embedded = capabilities["embeddedContext"].as_bool().unwrap_or(false);
            let (cwd, mcp, resume) = {
                let mut s = shared.state.lock().unwrap();
                s.takes_images = takes_images;
                s.takes_pdf = embedded && s.reads_pdf;
                let resume = s.resume.clone().filter(|_| can_load);
                s.loading = resume.is_some();
                (s.cwd.clone(), s.mcp_servers.clone(), resume)
            };
            match resume {
                Some(session) => shared.request(
                    "session/load",
                    json!({"sessionId": session, "cwd": cwd, "mcpServers": mcp}),
                    Pending::Session,
                ),
                None => shared.request(
                    "session/new",
                    json!({"cwd": cwd, "mcpServers": mcp}),
                    Pending::Session,
                ),
            }
        }
        Some(Pending::Session) => {
            let (session, prompt) = {
                let mut s = shared.state.lock().unwrap();
                let session = result["sessionId"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| s.resume.clone())
                    .unwrap_or_default();
                s.session = Some(session.clone());
                s.loading = false;
                (session, s.first_prompt.take())
            };
            shared.emit(AgentEvent::SessionStarted {
                session_id: session,
                model: String::new(),
            });
            match prompt {
                Some(prompt) => shared.prompt(prompt),
                None => Ok(()),
            }
        }
        Some(Pending::Prompt) => turn_ended(shared, result["stopReason"].as_str().unwrap_or("")),
        None => Ok(()),
    }
}

fn turn_ended(shared: &Shared, stop_reason: &str) -> PortResult<()> {
    shared.flush_message();
    let ok = matches!(stop_reason, "end_turn" | "max_tokens" | "max_turn_requests");
    let (planning, plans, message, duration_ms, next) = {
        let mut s = shared.state.lock().unwrap();
        s.in_turn = false;
        if s.planning && ok {
            s.plans += 1;
        }
        (
            s.planning,
            s.plans,
            s.last_message.clone(),
            s.turn_started
                .take()
                .map(|t| t.elapsed().as_millis() as u64),
            s.queued.pop_front(),
        )
    };
    if planning && ok {
        shared.emit(AgentEvent::Plan {
            text: message,
            request_id: Some(format!("acp-plan-{plans}")),
        });
    } else {
        let summary = match stop_reason {
            "refusal" => tr!(
                "El agente se negó a continuar.",
                "The agent refused to go on."
            ),
            "cancelled" => tr!("Detenido.", "Stopped."),
            _ => message,
        };
        shared.emit(AgentEvent::Finished {
            ok,
            summary,
            duration_ms,
        });
    }
    match next {
        Some(turn) => shared.prompt(turn),
        None => Ok(()),
    }
}

/// `session/update` notifications: text, tool activity, plan, cost and the
/// agent's slash commands.
fn update(shared: &Shared, update: &Value) -> PortResult<()> {
    // Still true while a loaded session replays its history.
    if update["sessionUpdate"] == "available_commands_update" {
        shared.emit(AgentEvent::Commands {
            commands: available_commands(&update["availableCommands"]),
        });
        return Ok(());
    }
    if shared.state.lock().unwrap().loading {
        return Ok(());
    }
    match update["sessionUpdate"].as_str().unwrap_or("") {
        "agent_message_chunk" => {
            if let Some(text) = update["content"]["text"].as_str() {
                shared.state.lock().unwrap().message.push_str(text);
            }
        }
        "tool_call" | "tool_call_update" => {
            shared.flush_message();
            let tool = remember_tool(shared, update);
            if let Some((id, tool)) = tool {
                report_tool(shared, &id, &tool, update["status"].as_str().unwrap_or(""));
            }
        }
        "plan" => {
            let steps = update["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| PlanStep {
                    text: entry["content"].as_str().unwrap_or("").to_owned(),
                    status: match entry["status"].as_str() {
                        Some("completed") => StepStatus::Done,
                        Some("in_progress") => StepStatus::InProgress,
                        _ => StepStatus::Pending,
                    },
                })
                .collect();
            shared.emit(AgentEvent::Steps { steps });
        }
        // ACP reports the session's cumulative cost, not tokens spent.
        "usage_update" => {
            let cost = &update["cost"];
            if cost["currency"].as_str() == Some("USD")
                && let Some(total) = cost["amount"].as_f64()
            {
                let added = {
                    let mut s = shared.state.lock().unwrap();
                    let added = total - s.cost_usd;
                    s.cost_usd = total;
                    added
                };
                if added > 0.0 {
                    shared.emit(AgentEvent::Usage(UsageSample {
                        cost_usd: Some(added),
                        ..UsageSample::default()
                    }));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// ACP's `AvailableCommand` list: `name` (no slash), `description` and an
/// optional `input.hint`. ACP does not say where a command comes from.
fn available_commands(list: &Value) -> Vec<AgentCommand> {
    let text = |v: &Value| {
        v.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|command| {
            let name = text(&command["name"])?;
            Some(AgentCommand {
                name: name.trim_start_matches('/').to_owned(),
                description: text(&command["description"]),
                source: CommandSource::Unknown,
                argument_hint: text(&command["input"]["hint"]),
            })
        })
        .collect()
}

/// Merges a tool call or update into what is known about it.
fn remember_tool(shared: &Shared, update: &Value) -> Option<(String, ToolCall)> {
    let id = update["toolCallId"].as_str()?.to_owned();
    let mut s = shared.state.lock().unwrap();
    let tool = s.tools.entry(id.clone()).or_default();
    if let Some(kind) = update["kind"].as_str() {
        tool.kind = kind.to_owned();
    }
    if let Some(title) = update["title"].as_str() {
        tool.title = title.to_owned();
    }
    if let Some(locations) = update["locations"].as_array() {
        tool.paths = locations
            .iter()
            .filter_map(|l| l["path"].as_str().map(str::to_owned))
            .collect();
    }
    if !update["rawInput"].is_null() {
        tool.raw_input = update["rawInput"].clone();
    }
    Some((id, tool.clone()))
}

/// The shell command of an `execute` tool call.
fn command_of(tool: &ToolCall) -> String {
    match &tool.raw_input["command"] {
        Value::String(c) => c.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" "),
        _ => tool.title.clone(),
    }
}

/// That a tool call started (once), then its activity lines when it finishes (once).
fn report_tool(shared: &Shared, id: &str, tool: &ToolCall, status: &str) {
    if !tool.started && !tool.reported && !tool.kind.is_empty() {
        if let Some(t) = shared.state.lock().unwrap().tools.get_mut(id) {
            t.started = true;
        }
        let detail = if tool.kind == "execute" {
            command_of(tool)
        } else {
            tool.title.clone()
        };
        shared.emit(AgentEvent::ToolStarted {
            tool: tool.kind.clone(),
            detail,
        });
    }
    if tool.reported || !matches!(status, "completed" | "failed") {
        return;
    }
    let cwd = {
        let mut s = shared.state.lock().unwrap();
        if let Some(t) = s.tools.get_mut(id) {
            t.reported = true;
        }
        s.cwd.clone()
    };
    let ok = status == "completed";
    match tool.kind.as_str() {
        "edit" | "delete" | "move" if ok => {
            for path in &tool.paths {
                shared.emit(AgentEvent::FileEdited {
                    path: relative(&cwd, Path::new(path)),
                });
            }
        }
        "execute" => {
            let command = command_of(tool);
            if looks_like_tests(&command) {
                shared.emit(AgentEvent::TestsRun {
                    command,
                    passed: ok,
                });
            } else if ok {
                shared.emit(AgentEvent::CommandRun {
                    command,
                    tests: false,
                });
            }
        }
        "read" | "search" | "fetch" if ok => shared.emit(AgentEvent::ToolUsed {
            tool: tool_name(&tool.kind).to_owned(),
            summary: tool.title.clone(),
        }),
        _ => {}
    }
}

/// Hedy's name for an ACP tool kind (as in Claude Code), for cards and rules.
fn tool_name(kind: &str) -> &'static str {
    match kind {
        "edit" | "delete" | "move" => "Edit",
        "execute" => "Bash",
        "read" => "Read",
        "search" => "Grep",
        "fetch" => "WebFetch",
        _ => "Tool",
    }
}

/// Requests from the agent. Only permissions are offered; files and terminals
/// stay with the agent's own tools.
fn client_request(shared: &Shared, method: &str, id: Value, params: &Value) -> PortResult<()> {
    if method != "session/request_permission" {
        return shared.write(&json!({"jsonrpc": "2.0", "id": id,
            "error": {"code": -32601, "message": format!("Hedy no ofrece {method}")}}));
    }
    shared.flush_message();
    let (_, tool) = remember_tool(shared, &params["toolCall"]).unwrap_or_default();
    let options = params["options"].as_array().cloned().unwrap_or_default();
    let changes = matches!(tool.kind.as_str(), "edit" | "delete" | "move" | "execute");
    let request_id = format!(
        "acp-{}",
        id.as_u64()
            .map_or_else(|| id.to_string(), |n| n.to_string())
    );
    let (blocked, cwd) = {
        let s = shared.state.lock().unwrap();
        let reason = if s.planning {
            Some("Primero se aprueba el plan.")
        } else if s.read_only {
            Some("En este modo no se cambian archivos.")
        } else {
            None
        };
        (reason.filter(|_| changes), s.cwd.clone())
    };
    if let Some(reason) = blocked {
        shared.emit(AgentEvent::PermissionResolved {
            request_id,
            decision: Decision::Deny,
            automatic: true,
            reason: Some(reason.to_owned()),
        });
        return answer_with(shared, &id, &options, false);
    }
    let name = tool_name(&tool.kind);
    let (summary, input) = match name {
        "Edit" => {
            let shown: Vec<String> = tool
                .paths
                .iter()
                .map(|p| relative(&cwd, Path::new(p)))
                .collect();
            (
                if shown.is_empty() {
                    tool.title.clone()
                } else {
                    shown.join(", ")
                },
                // `kind`: a move or a delete is never allowed unattended.
                json!({"file_path": tool.paths.first(), "paths": tool.paths, "kind": tool.kind}),
            )
        }
        "Bash" => {
            let command = command_of(&tool);
            (command.clone(), json!({"command": command}))
        }
        "Read" => (tool.title.clone(), json!({"file_path": tool.paths.first()})),
        _ => (tool.title.clone(), tool.raw_input.clone()),
    };
    shared
        .state
        .lock()
        .unwrap()
        .approvals
        .insert(request_id.clone(), (id, options));
    shared.emit(AgentEvent::PermissionRequest {
        request_id,
        tool: name.to_owned(),
        summary,
        input,
    });
    Ok(())
}

/// Picks the agent's option for a yes or a no (never an "always" when a
/// one-time option exists: Hedy keeps its own rules).
fn answer_with(shared: &Shared, id: &Value, options: &[Value], allow: bool) -> PortResult<()> {
    let wanted: &[&str] = if allow {
        &["allow_once", "allow_always"]
    } else {
        &["reject_once", "reject_always"]
    };
    let option = wanted.iter().find_map(|kind| {
        options
            .iter()
            .find(|o| o["kind"].as_str() == Some(kind))
            .and_then(|o| o["optionId"].as_str())
    });
    let outcome = match option {
        Some(option) => json!({"outcome": "selected", "optionId": option}),
        None => json!({"outcome": "cancelled"}),
    };
    shared.write(&json!({"jsonrpc": "2.0", "id": id, "result": {"outcome": outcome}}))
}

impl AgentSession for AcpSession {
    fn send(&mut self, text: &str, images: &[PromptImage]) -> PortResult<()> {
        self.shared.prompt(Turn {
            text: text.to_owned(),
            images: images.to_vec(),
        })
    }

    fn answer(&mut self, request_id: &str, allow: bool, message: Option<&str>) -> PortResult<()> {
        if request_id.starts_with("acp-plan-") {
            if allow {
                self.shared.state.lock().unwrap().planning = false;
                return self.shared.prompt(Turn::text(prompts::plan_approved()));
            }
            return self
                .shared
                .prompt(Turn::text(message.unwrap_or("Revisa el plan.")));
        }
        let (id, options) = self
            .shared
            .state
            .lock()
            .unwrap()
            .approvals
            .remove(request_id)
            .ok_or_else(|| {
                PortError::new(format!("el agente no espera la respuesta {request_id}"))
            })?;
        answer_with(&self.shared, &id, &options, allow)
    }

    fn stop(&mut self) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            if let Some(session) = self.shared.state.lock().unwrap().session.clone() {
                let _ = self
                    .shared
                    .write(&json!({"jsonrpc": "2.0", "method": "session/cancel",
                    "params": {"sessionId": session}}));
            }
            let mut child = self.child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for AcpSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::domain::{LaunchSpec, Phase, TaskMode};

    /// An ACP v1 agent: proposes a plan, then asks to edit a file and runs the
    /// tests. `FAKE_ACP_MODE` changes how it answers `initialize`.
    const FAKE_ACP: &str = r#"
import json, os, sys
mode = os.environ.get("FAKE_ACP_MODE", "")
turns = 0
def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n"); sys.stdout.flush()
def update(u):
    send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s-1", "update": u}})
pending_prompt = None
for line in sys.stdin:
    msg = json.loads(line)
    method, mid, params = msg.get("method"), msg.get("id"), msg.get("params", {})
    if method == "initialize":
        assert params["protocolVersion"] == 1
        assert params["clientCapabilities"]["fs"]["writeTextFile"] is False
        version = 2 if mode == "v2" else 1
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": version,
              "agentCapabilities": {"loadSession": False}, "authMethods": []}})
    elif method == "session/new":
        if mode == "auth":
            send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32000, "message": "Authentication required"}})
            continue
        assert params["cwd"] and params["mcpServers"] == []
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s-1"}})
        update({"sessionUpdate": "available_commands_update", "availableCommands": [
            {"name": "web", "description": "Search the web", "input": {"hint": "query to search for"}},
            {"name": "/init", "description": ""}]})
    elif method == "session/prompt" and mode == "echo":
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": params["prompt"][0]["text"]}})
        send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
    elif method == "session/prompt":
        turns += 1
        text = params["prompt"][0]["text"]
        if turns == 1:
            update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "1. Crear "}})
            update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "hola.txt"}})
            send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
        else:
            pending_prompt = mid
            update({"sessionUpdate": "plan", "entries": [
                {"content": "Crear hola.txt", "priority": "high", "status": "in_progress"}]})
            update({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Write hola.txt",
                    "kind": "edit", "status": "pending",
                    "locations": [{"path": os.path.join(os.getcwd(), "hola.txt")}]})
            send({"jsonrpc": "2.0", "id": 99, "method": "session/request_permission", "params": {
                "sessionId": "s-1", "toolCall": {"toolCallId": "t1"},
                "options": [{"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                            {"optionId": "always", "name": "Always", "kind": "allow_always"},
                            {"optionId": "no", "name": "Reject", "kind": "reject_once"}]}})
    elif mid == 99:
        assert msg["result"]["outcome"] == {"outcome": "selected", "optionId": "yes"}, msg
        update({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed"})
        update({"sessionUpdate": "tool_call", "toolCallId": "t2", "title": "pytest -q",
                "kind": "execute", "status": "completed", "rawInput": {"command": "pytest -q"}})
        update({"sessionUpdate": "usage_update", "used": 1200, "size": 200000,
                "cost": {"amount": 0.25, "currency": "USD"}})
        update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Listo."}})
        send({"jsonrpc": "2.0", "id": pending_prompt, "result": {"stopReason": "end_turn"}})
"#;

    fn start(
        dir: &Path,
        mode: &str,
        plan_first: bool,
    ) -> (Box<dyn AgentSession>, mpsc::Receiver<AgentEvent>) {
        start_with(dir, mode, plan_first, "Crea hola.txt")
    }

    fn start_with(
        dir: &Path,
        mode: &str,
        plan_first: bool,
        prompt: &str,
    ) -> (Box<dyn AgentSession>, mpsc::Receiver<AgentEvent>) {
        let script = dir.join("fake_acp.py");
        std::fs::write(&script, FAKE_ACP).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let mut launch = LaunchSpec::program(
            "env",
            vec![
                format!("FAKE_ACP_MODE={mode}"),
                "python3".into(),
                "-I".into(),
                script.to_string_lossy().into_owned(),
            ],
        )
        .in_dir(dir.to_owned());
        launch.args.push("acp".into());
        let request = AgentRequest {
            images: Vec::new(),
            backend: AgentBackend::Acp {
                name: "Kimi Code".into(),
                model_flag: None,
                reads_pdf: true,
            },
            launch,
            mode: TaskMode::Project,
            phase: if plan_first {
                Phase::Plan
            } else {
                Phase::Implement
            },
            plan_first,
            model: None,
            prompt: prompt.into(),
            resume: None,
            memory: None,
            mcp: vec![],
            conversation: false,
            no_shell: false,
            no_tools: false,
            plugins: Vec::new(),
            output: None,
            blocked: Default::default(),
            board: None,
        };
        let session = AcpLauncher
            .start(request, Arc::new(move |e| drop(tx.lock().unwrap().send(e))))
            .unwrap();
        (session, rx)
    }

    fn until(
        rx: &mpsc::Receiver<AgentEvent>,
        stop: impl Fn(&AgentEvent) -> bool,
    ) -> Vec<AgentEvent> {
        let mut seen = Vec::new();
        while let Ok(event) = rx.recv_timeout(Duration::from_secs(10)) {
            let done = stop(&event);
            seen.push(event);
            if done {
                return seen;
            }
        }
        panic!("timed out; saw {seen:?}");
    }

    #[test]
    fn plans_then_implements_with_cards() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut session, rx) = start(tmp.path(), "", true);
        let seen = until(&rx, |e| matches!(e, AgentEvent::Plan { .. }));
        assert!(seen.contains(&AgentEvent::SessionStarted {
            session_id: "s-1".into(),
            model: String::new()
        }));
        assert!(seen.contains(&AgentEvent::Plan {
            text: "1. Crear hola.txt".into(),
            request_id: Some("acp-plan-1".into()),
        }));

        session.answer("acp-plan-1", true, None).unwrap();
        let seen = until(&rx, |e| matches!(e, AgentEvent::PermissionRequest { .. }));
        let Some(AgentEvent::PermissionRequest {
            request_id,
            tool,
            summary,
            input,
        }) = seen.last()
        else {
            unreachable!()
        };
        assert_eq!((tool.as_str(), summary.as_str()), ("Edit", "hola.txt"));
        assert!(input["file_path"].as_str().unwrap().ends_with("/hola.txt"));
        assert!(seen.iter().any(|e| matches!(e, AgentEvent::Steps { steps } if steps[0].status == StepStatus::InProgress)));

        session.answer(request_id, true, None).unwrap();
        let seen = until(&rx, |e| matches!(e, AgentEvent::Finished { .. }));
        assert!(seen.contains(&AgentEvent::FileEdited {
            path: "hola.txt".into()
        }));
        assert!(seen.contains(&AgentEvent::TestsRun {
            command: "pytest -q".into(),
            passed: true
        }));
        assert!(
            seen.iter()
                .any(|e| matches!(e, AgentEvent::Usage(u) if u.cost_usd == Some(0.25)))
        );
        assert!(
            matches!(seen.last(), Some(AgentEvent::Finished { ok: true, summary, .. }) if summary == "Listo.")
        );
        session.stop();
    }

    #[test]
    fn reports_the_agents_commands_and_sends_a_command_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut session, rx) = start_with(tmp.path(), "echo", false, "/web acp schema");
        let seen = until(&rx, |e| matches!(e, AgentEvent::Finished { .. }));
        assert!(seen.contains(&AgentEvent::Commands {
            commands: vec![
                AgentCommand {
                    name: "web".into(),
                    description: Some("Search the web".into()),
                    source: CommandSource::Unknown,
                    argument_hint: Some("query to search for".into()),
                },
                AgentCommand::new("init", CommandSource::Unknown),
            ]
        }));
        // No guidance before it: the command must open the prompt.
        assert!(seen.contains(&AgentEvent::Message {
            text: "/web acp schema".into()
        }));
        session.stop();

        let (mut session, rx) = start_with(tmp.path(), "echo", false, "¿Qué hace?");
        let seen = until(&rx, |e| matches!(e, AgentEvent::Finished { .. }));
        assert!(seen.iter().any(
            |e| matches!(e, AgentEvent::Message { text } if text.ends_with("¿Qué hace?") && text.len() > 20)
        ));
        session.stop();
    }

    #[test]
    fn edits_are_refused_while_planning() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut session, rx) = start(tmp.path(), "", true);
        until(&rx, |e| matches!(e, AgentEvent::Plan { .. }));
        // Asking for changes keeps the plan-only turn: the edit is refused.
        session
            .answer("acp-plan-1", false, Some("Hazlo en Python"))
            .unwrap();
        let seen = until(&rx, |e| matches!(e, AgentEvent::PermissionResolved { .. }));
        assert!(matches!(
            seen.last(),
            Some(AgentEvent::PermissionResolved {
                decision: Decision::Deny,
                automatic: true,
                ..
            })
        ));
        session.stop();
    }

    #[test]
    fn login_and_version_problems_end_the_stage() {
        let tmp = tempfile::tempdir().unwrap();
        for (mode, expected) in [("auth", "inicia sesión"), ("v2", "versión 2")] {
            let (mut session, rx) = start(tmp.path(), mode, false);
            let seen = until(&rx, |e| matches!(e, AgentEvent::Finished { .. }));
            assert!(
                matches!(seen.last(), Some(AgentEvent::Finished { ok: false, summary, .. }) if summary.contains(expected)),
                "{mode}: {seen:?}"
            );
            session.stop();
        }
    }

    /// Says whether it takes images (`FAKE_ACP_IMAGES`) and answers each
    /// prompt with the types of its blocks.
    const IMAGE_ACP: &str = r#"
import json, os, sys
images = os.environ.get("FAKE_ACP_IMAGES") == "1"
embedded = os.environ.get("FAKE_ACP_EMBEDDED") == "1"
def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n"); sys.stdout.flush()
for line in sys.stdin:
    msg = json.loads(line)
    method, mid, params = msg.get("method"), msg.get("id"), msg.get("params", {})
    if method == "initialize":
        caps = {"loadSession": False}
        caps["promptCapabilities"] = {"image": images, "embeddedContext": embedded}
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1, "agentCapabilities": caps}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s-1"}})
    elif method == "session/prompt":
        blocks = params["prompt"]
        def kind(b):
            if b["type"] == "image":
                return "image:" + b["mimeType"]
            if b["type"] == "resource":
                r = b["resource"]
                return "resource:" + r["mimeType"] + ":" + str(bool(r.get("blob"))) + ":" + r["uri"][:7]
            return b["type"]
        kinds = ",".join(kind(b) for b in blocks)
        send({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s-1",
              "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": kinds}}}})
        send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
"#;

    fn image_turn(takes_images: bool) -> Vec<AgentEvent> {
        attachment_turn(takes_images, false, false)
    }

    /// A turn with a PNG, and a PDF when `pdf`; `embedded` is the agent's
    /// `promptCapabilities.embeddedContext`.
    fn attachment_turn(takes_images: bool, embedded: bool, pdf: bool) -> Vec<AgentEvent> {
        let tmp = tempfile::tempdir().unwrap();
        let mut images = vec![crate::adapters::images::tests::png(tmp.path())];
        if pdf {
            images.push(crate::adapters::images::tests::pdf(tmp.path()));
        }
        let script = tmp.path().join("image_acp.py");
        std::fs::write(&script, IMAGE_ACP).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let flag = |on: bool| if on { "1" } else { "0" };
        let request = AgentRequest {
            backend: AgentBackend::Acp {
                name: "Gemini CLI".into(),
                model_flag: None,
                reads_pdf: true,
            },
            launch: LaunchSpec::program(
                "env",
                vec![
                    format!("FAKE_ACP_IMAGES={}", flag(takes_images)),
                    format!("FAKE_ACP_EMBEDDED={}", flag(embedded)),
                    "python3".into(),
                    "-I".into(),
                    script.to_string_lossy().into_owned(),
                ],
            )
            .in_dir(tmp.path().to_owned()),
            mode: TaskMode::Ask,
            phase: Phase::Implement,
            plan_first: false,
            model: None,
            prompt: "mira".into(),
            images,
            resume: None,
            memory: None,
            mcp: vec![],
            conversation: false,
            no_shell: false,
            no_tools: false,
            plugins: Vec::new(),
            output: None,
            blocked: Default::default(),
            board: None,
        };
        let mut session = AcpLauncher
            .start(request, Arc::new(move |e| drop(tx.lock().unwrap().send(e))))
            .unwrap();
        let seen = until(&rx, |e| matches!(e, AgentEvent::Finished { .. }));
        session.stop();
        seen
    }

    #[test]
    fn images_go_only_to_agents_that_take_them() {
        let seen = image_turn(true);
        assert!(
            seen.contains(&AgentEvent::Message {
                text: "text,image:image/png".into()
            }),
            "{seen:?}"
        );

        let seen = image_turn(false);
        assert!(seen.contains(&AgentEvent::Message {
            text: "text".into()
        }));
        assert!(
            seen.iter().any(
                |e| matches!(e, AgentEvent::Error { message } if message.contains("no acepta imágenes"))
            ),
            "the user hears the images did not go: {seen:?}"
        );
    }

    #[test]
    fn pdfs_go_as_embedded_resources_only_when_the_agent_takes_them() {
        let seen = attachment_turn(true, true, true);
        assert!(
            seen.contains(&AgentEvent::Message {
                text: "text,image:image/png,resource:application/pdf:True:file://".into()
            }),
            "{seen:?}"
        );

        let seen = attachment_turn(true, false, true);
        assert!(
            seen.contains(&AgentEvent::Message {
                text: "text,image:image/png".into()
            }),
            "{seen:?}"
        );
        assert!(
            seen.iter().any(
                |e| matches!(e, AgentEvent::Error { message } if message.contains("no lee PDF"))
            ),
            "the user hears the PDF did not go: {seen:?}"
        );
    }
}
