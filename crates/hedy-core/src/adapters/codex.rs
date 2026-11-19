//! Codex through `codex app-server` (JSON-RPC 2.0 over stdio, one JSON per
//! line). Methods and fields come from the schema the CLI generates
//! (`codex app-server generate-json-schema`, docs/decisions.md §4).
//!
//! Codex has no plan-approval step, so `plan_first` is emulated: a read-only
//! turn that only proposes a plan, then a writable turn once it is approved.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::paths::relative;
use super::process::{headless_command, launch_error, wait_unlocked};
use crate::application::ports::{
    AgentLauncher, AgentRequest, AgentSession, EventSink, PortError, PortResult,
};
use crate::domain::agent::looks_like_tests;
use crate::domain::{AgentEvent, PlanStep, PromptImage, StepStatus, UsageSample, prompts};
use crate::tr;

pub struct CodexLauncher;

/// What a response to one of our requests means.
enum Pending {
    Initialize,
    Thread,
    Other,
}

struct State {
    next_id: u64,
    pending: HashMap<u64, Pending>,
    thread: Option<String>,
    cwd: PathBuf,
    read_only: bool,
    /// The current turn only proposes a plan.
    planning: bool,
    plans: u32,
    first_prompt: Option<String>,
    /// Images of the first prompt.
    first_images: Vec<PromptImage>,
    model: Option<String>,
    resume: Option<String>,
    /// Extra instructions for the thread (the memory guidance).
    instructions: Option<String>,
    /// Approval request ids (as sent to the UI) → JSON-RPC ids to answer.
    approvals: HashMap<String, Value>,
    /// fileChange item id → paths.
    changes: HashMap<String, Vec<String>>,
    last_message: String,
    last_usage: Option<UsageSample>,
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
            .map_err(|e| PortError::new(format!("Codex ya no responde: {e}")))
    }

    fn request(&self, method: &str, params: Value, kind: Pending) -> PortResult<()> {
        let id = {
            let mut state = self.state.lock().unwrap();
            state.next_id += 1;
            let id = state.next_id;
            state.pending.insert(id, kind);
            id
        };
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    }

    fn sandbox(read_only: bool) -> Value {
        if read_only {
            json!({"type": "readOnly"})
        } else {
            json!({"type": "workspaceWrite"})
        }
    }

    /// A user turn. `read_only` overrides the session's sandbox for this turn.
    fn turn(&self, text: &str, images: &[PromptImage], read_only: bool) -> PortResult<()> {
        let thread = self.state.lock().unwrap().thread.clone();
        let Some(thread) = thread else {
            return Err(PortError::new(tr!(
                "Codex todavía no abrió la conversación",
                "Codex has not opened the conversation yet"
            )));
        };
        self.request(
            "turn/start",
            json!({"threadId": thread, "input": super::images::codex_input(text, images),
                   "sandboxPolicy": Self::sandbox(read_only)}),
            Pending::Other,
        )
    }

    fn emit(&self, event: AgentEvent) {
        (self.events)(event);
    }
}

pub struct CodexSession {
    shared: Arc<Shared>,
    child: Arc<Mutex<Child>>,
    stopped: Arc<AtomicBool>,
}

impl AgentLauncher for CodexLauncher {
    fn start(&self, request: AgentRequest, events: EventSink) -> PortResult<Box<dyn AgentSession>> {
        let mut spec = request.launch.clone();
        spec.args.extend(mcp_overrides(&request));
        spec.args
            .extend(["app-server", "--listen", "stdio://"].map(String::from));
        let mut child = headless_command(&spec)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| launch_error(&spec, e))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");

        let read_only = request.phase.read_only_in(request.mode);
        let first_prompt = if request.plan_first {
            format!("{}\n\n{}", prompts::plan_first(), request.prompt)
        } else {
            request.prompt.clone()
        };
        let shared = Arc::new(Shared {
            stdin: Mutex::new(stdin),
            state: Mutex::new(State {
                next_id: 0,
                pending: HashMap::new(),
                thread: None,
                cwd: spec.cwd.clone().unwrap_or_default(),
                read_only,
                planning: request.plan_first,
                plans: 0,
                first_prompt: Some(first_prompt),
                first_images: request.images.clone(),
                model: request.model.clone(),
                resume: request.resume.clone(),
                instructions: request.memory.as_ref().map(|memory| {
                    prompts::memory_context(request.phase, request.mode, memory.team.as_deref())
                }),
                approvals: HashMap::new(),
                changes: HashMap::new(),
                last_message: String::new(),
                last_usage: None,
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
                // The app-server only exits when stopped; anything else is a crash.
                let code = wait_unlocked(&child);
                if !stopped.load(Ordering::SeqCst) {
                    let code = code.map_or("?".into(), |c| c.to_string());
                    let message = tr!(
                        "Codex se detuvo (código {code}).",
                        "Codex stopped (code {code})."
                    );
                    shared.emit(AgentEvent::Error {
                        message: message.clone(),
                    });
                    shared.emit(AgentEvent::Finished {
                        ok: false,
                        summary: message,
                        duration_ms: None,
                    });
                }
            });
        }
        shared.request(
            "initialize",
            json!({"clientInfo": {"name": "hedy", "title": "Hedy", "version": env!("CARGO_PKG_VERSION")}}),
            Pending::Initialize,
        )?;
        Ok(Box::new(CodexSession {
            shared,
            child,
            stopped,
        }))
    }
}

/// MCP servers as `-c` config overrides (TOML values) for this run only;
/// the account's `config.toml` is left untouched.
fn mcp_overrides(request: &AgentRequest) -> Vec<String> {
    // JSON strings are valid TOML basic strings.
    let toml = |s: &str| Value::String(s.to_owned()).to_string();
    let mut out = Vec::new();
    for server in request.mcp_servers() {
        let key = format!("mcp_servers.{}", server.name);
        let args: Vec<String> = server.args.iter().map(|a| toml(a)).collect();
        out.extend([
            "-c".into(),
            format!("{key}.command={}", toml(&server.command)),
            "-c".into(),
            format!("{key}.args=[{}]", args.join(", ")),
        ]);
        if !server.env.is_empty() {
            let env: Vec<String> = server
                .env
                .iter()
                .map(|(k, v)| format!("{k} = {}", toml(v)))
                .collect();
            out.extend(["-c".into(), format!("{key}.env={{ {} }}", env.join(", "))]);
        }
    }
    out
}

fn handle(shared: &Shared, msg: &Value) -> PortResult<()> {
    let method = msg["method"].as_str();
    match (method, msg.get("id")) {
        // A response to one of our requests.
        (None, Some(id)) => response(shared, id.as_u64().unwrap_or(0), msg),
        // A request from Codex: approvals.
        (Some(method), Some(id)) => server_request(shared, method, id.clone(), &msg["params"]),
        (Some(method), None) => notification(shared, method, &msg["params"]),
        _ => Ok(()),
    }
}

fn response(shared: &Shared, id: u64, msg: &Value) -> PortResult<()> {
    let kind = shared.state.lock().unwrap().pending.remove(&id);
    if let Some(error) = msg.get("error") {
        let message = format!("Codex: {}", error["message"].as_str().unwrap_or("error"));
        shared.emit(AgentEvent::Error {
            message: message.clone(),
        });
        // Without a thread no turn ever starts: end the stage.
        if matches!(kind, Some(Pending::Initialize | Pending::Thread)) {
            shared.emit(AgentEvent::Finished {
                ok: false,
                summary: message,
                duration_ms: None,
            });
        }
        return Ok(());
    }
    match kind {
        Some(Pending::Initialize) => {
            shared.write(&json!({"jsonrpc": "2.0", "method": "initialized"}))?;
            let (cwd, read_only, model, resume, instructions) = {
                let s = shared.state.lock().unwrap();
                (
                    s.cwd.clone(),
                    s.read_only,
                    s.model.clone(),
                    s.resume.clone(),
                    s.instructions.clone(),
                )
            };
            let mut params = json!({
                "cwd": cwd,
                "approvalPolicy": "untrusted",
                "sandbox": if read_only { "read-only" } else { "workspace-write" },
            });
            if let Some(model) = model {
                params["model"] = json!(model);
            }
            if let Some(instructions) = instructions {
                params["developerInstructions"] = json!(instructions);
            }
            match resume {
                Some(thread) => {
                    params["threadId"] = json!(thread);
                    shared.request("thread/resume", params, Pending::Thread)
                }
                None => shared.request("thread/start", params, Pending::Thread),
            }
        }
        Some(Pending::Thread) => {
            let result = &msg["result"];
            let thread = result["thread"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let (prompt, images, read_only) = {
                let mut s = shared.state.lock().unwrap();
                s.thread = Some(thread.clone());
                (
                    s.first_prompt.take(),
                    std::mem::take(&mut s.first_images),
                    s.read_only || s.planning,
                )
            };
            shared.emit(AgentEvent::SessionStarted {
                session_id: thread,
                model: result["model"].as_str().unwrap_or("codex").to_owned(),
            });
            match prompt {
                Some(prompt) => shared.turn(&prompt, &images, read_only),
                None => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

fn server_request(shared: &Shared, method: &str, id: Value, params: &Value) -> PortResult<()> {
    let request_id = format!("codex-{id}");
    let (tool, summary, input) = match method {
        "item/commandExecution/requestApproval" => {
            let command = match &params["command"] {
                Value::String(s) => s.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => String::new(),
            };
            ("Bash", command.clone(), json!({"command": command}))
        }
        "item/fileChange/requestApproval" => {
            let (cwd, paths) = {
                let s = shared.state.lock().unwrap();
                let item = params["itemId"].as_str().unwrap_or("");
                (
                    s.cwd.clone(),
                    s.changes.get(item).cloned().unwrap_or_default(),
                )
            };
            let shown: Vec<String> = paths.iter().map(|p| relative(&cwd, Path::new(p))).collect();
            let summary = if shown.is_empty() {
                params["reason"].as_str().unwrap_or("cambios").to_owned()
            } else {
                shown.join(", ")
            };
            let first = paths.first().cloned().unwrap_or_default();
            ("Edit", summary, json!({"file_path": first, "paths": paths}))
        }
        // Anything else Codex asks (user input, MCP elicitation) is declined.
        _ => {
            return shared.write(&json!({"jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": "Hedy does not support this request"}}));
        }
    };
    shared
        .state
        .lock()
        .unwrap()
        .approvals
        .insert(request_id.clone(), id);
    shared.emit(AgentEvent::PermissionRequest {
        request_id,
        tool: tool.into(),
        summary,
        input,
    });
    Ok(())
}

fn notification(shared: &Shared, method: &str, p: &Value) -> PortResult<()> {
    let cwd = shared.state.lock().unwrap().cwd.clone();
    match method {
        "item/started" if p["item"]["type"] == "commandExecution" => {
            shared.emit(AgentEvent::ToolStarted {
                tool: "commandExecution".into(),
                detail: p["item"]["command"].as_str().unwrap_or("").to_owned(),
            });
        }
        "item/started" if p["item"]["type"] == "fileChange" => {
            shared.emit(AgentEvent::ToolStarted {
                tool: "fileChange".into(),
                detail: String::new(),
            });
            let paths = p["item"]["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["path"].as_str().map(str::to_owned))
                .collect();
            let id = p["item"]["id"].as_str().unwrap_or("").to_owned();
            shared.state.lock().unwrap().changes.insert(id, paths);
        }
        "item/completed" => {
            let item = &p["item"];
            let completed = item["status"]
                .as_str()
                .is_none_or(|s| s == "completed" || s == "failed");
            match item["type"].as_str() {
                Some("agentMessage") => {
                    let text = item["text"].as_str().unwrap_or("").trim().to_owned();
                    if !text.is_empty() {
                        shared.state.lock().unwrap().last_message = text.clone();
                        shared.emit(AgentEvent::Message { text });
                    }
                }
                Some("commandExecution") if completed => {
                    let command = item["command"].as_str().unwrap_or("").to_owned();
                    let passed = item["exitCode"].as_i64() == Some(0);
                    shared.emit(if looks_like_tests(&command) {
                        AgentEvent::TestsRun { command, passed }
                    } else {
                        AgentEvent::CommandRun {
                            command,
                            tests: false,
                        }
                    });
                }
                Some("fileChange") if item["status"] == "completed" => {
                    for change in item["changes"].as_array().into_iter().flatten() {
                        if let Some(path) = change["path"].as_str() {
                            shared.emit(AgentEvent::FileEdited {
                                path: relative(&cwd, Path::new(path)),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        "turn/plan/updated" => {
            let steps = p["plan"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|s| PlanStep {
                    text: s["step"].as_str().unwrap_or("").to_owned(),
                    status: match s["status"].as_str() {
                        Some("completed") => StepStatus::Done,
                        Some("inProgress") => StepStatus::InProgress,
                        _ => StepStatus::Pending,
                    },
                })
                .collect();
            shared.emit(AgentEvent::Steps { steps });
        }
        "thread/tokenUsage/updated" => {
            let last = &p["tokenUsage"]["last"];
            let tokens = |k: &str| last[k].as_u64().unwrap_or(0);
            shared.state.lock().unwrap().last_usage = Some(UsageSample {
                input_tokens: tokens("inputTokens").saturating_sub(tokens("cachedInputTokens")),
                output_tokens: tokens("outputTokens"),
                cache_read_tokens: tokens("cachedInputTokens"),
                cache_write_tokens: tokens("cacheWriteInputTokens"),
                cost_usd: None,
            });
        }
        "account/rateLimits/updated" => {
            // The most used window decides; its length is reported, not assumed.
            let limits = &p["rateLimits"];
            let busiest = ["primary", "secondary"]
                .iter()
                .map(|k| &limits[*k])
                .filter(|w| w.is_object())
                .max_by(|a, b| {
                    a["usedPercent"]
                        .as_f64()
                        .unwrap_or(0.0)
                        .total_cmp(&b["usedPercent"].as_f64().unwrap_or(0.0))
                });
            if let Some(window) = busiest {
                let percent = window["usedPercent"].as_f64();
                shared.emit(AgentEvent::RateLimit {
                    status: if percent.is_some_and(|p| p >= 100.0) {
                        "rejected"
                    } else {
                        "allowed"
                    }
                    .into(),
                    utilization: percent,
                    resets_at: window["resetsAt"].as_i64(),
                });
            }
        }
        "turn/completed" => {
            let turn = &p["turn"];
            let (planning, usage, message, plans) = {
                let mut s = shared.state.lock().unwrap();
                if s.planning {
                    s.plans += 1;
                }
                (
                    s.planning,
                    s.last_usage.take(),
                    std::mem::take(&mut s.last_message),
                    s.plans,
                )
            };
            if let Some(usage) = usage {
                shared.emit(AgentEvent::Usage(usage));
            }
            let ok = turn["status"] == "completed";
            if planning && ok {
                shared.emit(AgentEvent::Plan {
                    text: message,
                    request_id: Some(format!("codex-plan-{plans}")),
                });
            } else {
                let error = turn["error"]["message"].as_str().map(str::to_owned);
                shared.emit(AgentEvent::Finished {
                    ok,
                    summary: error.unwrap_or(message),
                    duration_ms: turn["durationMs"].as_u64(),
                });
            }
        }
        "error" if p["willRetry"] != true => {
            shared.emit(AgentEvent::Error {
                message: format!(
                    "Codex: {}",
                    p["error"]["message"].as_str().unwrap_or("error")
                ),
            });
        }
        _ => {}
    }
    Ok(())
}

impl AgentSession for CodexSession {
    fn send(&mut self, text: &str, images: &[PromptImage]) -> PortResult<()> {
        let read_only = {
            let s = self.shared.state.lock().unwrap();
            s.read_only || s.planning
        };
        self.shared.turn(text, images, read_only)
    }

    fn answer(&mut self, request_id: &str, allow: bool, message: Option<&str>) -> PortResult<()> {
        if request_id.starts_with("codex-plan-") {
            if allow {
                let read_only = {
                    let mut s = self.shared.state.lock().unwrap();
                    s.planning = false;
                    s.read_only
                };
                return self.shared.turn(&prompts::plan_approved(), &[], read_only);
            }
            return self
                .shared
                .turn(message.unwrap_or("Revisa el plan."), &[], true);
        }
        let id = self
            .shared
            .state
            .lock()
            .unwrap()
            .approvals
            .remove(request_id)
            .ok_or_else(|| PortError::new(format!("Codex no espera la respuesta {request_id}")))?;
        let decision = if allow { "accept" } else { "decline" };
        self.shared
            .write(&json!({"jsonrpc": "2.0", "id": id, "result": {"decision": decision}}))
    }

    fn stop(&mut self) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            let mut child = self.child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for CodexSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::application::ports::AgentBackend;
    use crate::domain::{LaunchSpec, Phase, TaskMode};

    /// Plays `codex app-server`: a read-only turn proposes a plan; a writable
    /// turn asks to add hola.txt and writes it only when accepted.
    const FAKE_CODEX: &str = r#"
import json, os, sys
def out(obj): print(json.dumps(obj), flush=True)
def note(method, params): out({"jsonrpc": "2.0", "method": method, "params": params})
for line in sys.stdin:
    msg = json.loads(line)
    method, mid, p = msg.get("method"), msg.get("id"), msg.get("params", {})
    if method == "initialize":
        out({"jsonrpc": "2.0", "id": mid, "result": {"userAgent": "fake"}})
    elif method in ("thread/start", "thread/resume"):
        out({"jsonrpc": "2.0", "id": mid, "result": {"thread": {"id": p.get("threadId", "th-1")},
             "model": p.get("model", "gpt-test") + "|" + p["sandbox"]}})
    elif method == "turn/start":
        out({"jsonrpc": "2.0", "id": mid, "result": {"turn": {"id": "tu"}}})
        text = p["input"][0]["text"]
        if p["sandboxPolicy"]["type"] == "readOnly":
            note("item/completed", {"item": {"type": "agentMessage", "id": "m", "text": "1. Crear hola.txt"}})
        else:
            path = os.path.join(os.getcwd(), "hola.txt")
            note("turn/plan/updated", {"plan": [{"step": "Crear hola.txt", "status": "inProgress"}]})
            note("item/started", {"item": {"type": "fileChange", "id": "fc1", "status": "inProgress",
                 "changes": [{"path": path, "kind": "add", "diff": ""}]}})
            out({"jsonrpc": "2.0", "id": 100, "method": "item/fileChange/requestApproval",
                 "params": {"itemId": "fc1", "threadId": "th-1", "turnId": "tu", "startedAtMs": 0}})
            answer = json.loads(sys.stdin.readline())["result"]["decision"]
            status = "declined"
            if answer == "accept":
                open(path, "w").write("hola")
                status = "completed"
            note("item/completed", {"item": {"type": "fileChange", "id": "fc1", "status": status,
                 "changes": [{"path": path, "kind": "add", "diff": ""}]}})
            note("item/completed", {"item": {"type": "agentMessage", "id": "m2", "text": "Listo: " + text}})
        note("thread/tokenUsage/updated", {"tokenUsage": {"last": {"inputTokens": 10, "cachedInputTokens": 4, "outputTokens": 3}, "total": {}}})
        note("account/rateLimits/updated", {"rateLimits": {"primary": {"usedPercent": 95, "windowDurationMins": 300, "resetsAt": 123},
             "secondary": {"usedPercent": 20, "windowDurationMins": 10080, "resetsAt": 999}}})
        note("turn/completed", {"threadId": "th-1", "turn": {"id": "tu", "status": "completed", "items": [], "durationMs": 7}})
"#;

    #[test]
    fn a_crashed_app_server_ends_the_stage() {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let request = AgentRequest {
            images: Vec::new(),
            backend: AgentBackend::Codex,
            launch: LaunchSpec::program("sh", vec!["-c".into(), "read x; exit 3".into()]),
            mode: TaskMode::Project,
            phase: Phase::Implement,
            plan_first: false,
            model: None,
            prompt: "hola".into(),
            resume: None,
            memory: None,
            mcp: vec![],
            conversation: false,
            no_shell: false,
            no_tools: false,
            plugins: Vec::new(),
        };
        let _session = CodexLauncher
            .start(request, Arc::new(move |e| drop(tx.lock().unwrap().send(e))))
            .unwrap();
        let finished = rx
            .iter()
            .find(|e| matches!(e, AgentEvent::Finished { .. }))
            .expect("a Finished event");
        assert!(
            matches!(finished, AgentEvent::Finished { ok: false, summary, .. } if summary.contains("código 3"))
        );
    }

    #[test]
    fn mcp_servers_are_config_overrides() {
        let mut request = AgentRequest {
            images: Vec::new(),
            backend: AgentBackend::Codex,
            launch: LaunchSpec::program("codex", vec![]),
            mode: TaskMode::Project,
            phase: Phase::Implement,
            plan_first: false,
            model: None,
            prompt: String::new(),
            resume: None,
            memory: None,
            mcp: vec![],
            conversation: false,
            no_shell: false,
            no_tools: false,
            plugins: Vec::new(),
        };
        assert!(mcp_overrides(&request).is_empty());
        request.mcp = vec![crate::application::ports::McpServer {
            name: "codebase-memory".into(),
            command: "/bin/cbm".into(),
            args: vec!["a\"b".into()],
            env: vec![("CBM_ALLOWED_ROOT".into(), "/code/x".into())],
        }];
        assert_eq!(
            mcp_overrides(&request),
            [
                "-c",
                r#"mcp_servers.codebase-memory.command="/bin/cbm""#,
                "-c",
                r#"mcp_servers.codebase-memory.args=["a\"b"]"#,
                "-c",
                r#"mcp_servers.codebase-memory.env={ CBM_ALLOWED_ROOT = "/code/x" }"#,
            ]
        );
    }

    fn start(dir: &Path, plan_first: bool) -> (Box<dyn AgentSession>, mpsc::Receiver<AgentEvent>) {
        let script = dir.join("fake_codex.py");
        std::fs::write(&script, FAKE_CODEX).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let request = AgentRequest {
            images: Vec::new(),
            backend: AgentBackend::Codex,
            launch: LaunchSpec::program(
                "python3",
                vec!["-I".into(), script.to_string_lossy().into_owned()],
            )
            .in_dir(dir.to_owned()),
            mode: TaskMode::Project,
            phase: if plan_first {
                Phase::Plan
            } else {
                Phase::Implement
            },
            plan_first,
            model: Some("gpt-5".into()),
            prompt: "Crea hola.txt".into(),
            resume: None,
            memory: None,
            mcp: vec![],
            conversation: false,
            no_shell: false,
            no_tools: false,
            plugins: Vec::new(),
        };
        let session = CodexLauncher
            .start(request, Arc::new(move |e| drop(tx.lock().unwrap().send(e))))
            .unwrap();
        (session, rx)
    }

    fn until<T>(
        rx: &mpsc::Receiver<AgentEvent>,
        mut pick: impl FnMut(&AgentEvent) -> Option<T>,
    ) -> (T, Vec<AgentEvent>) {
        let mut seen = Vec::new();
        loop {
            let e = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("no event from fake codex");
            if let Some(found) = pick(&e) {
                seen.push(e);
                return (found, seen);
            }
            seen.push(e);
        }
    }

    #[test]
    fn plans_read_only_then_implements_with_approval() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let (mut session, rx) = start(&dir, true);

        let (plan_id, seen) = until(&rx, |e| match e {
            AgentEvent::Plan { request_id, .. } => request_id.clone(),
            _ => None,
        });
        let started = seen.iter().find_map(|e| match e {
            AgentEvent::SessionStarted { session_id, model } => {
                Some((session_id.clone(), model.clone()))
            }
            _ => None,
        });
        assert_eq!(
            started,
            Some(("th-1".into(), "gpt-5|workspace-write".into()))
        );
        assert!(seen.contains(&AgentEvent::Plan {
            text: "1. Crear hola.txt".into(),
            request_id: Some(plan_id.clone())
        }));
        assert!(!dir.join("hola.txt").exists());

        session.answer(&plan_id, true, None).unwrap();
        let (request, _) = until(&rx, |e| match e {
            AgentEvent::PermissionRequest {
                request_id,
                tool,
                summary,
                input,
            } => {
                assert_eq!((tool.as_str(), summary.as_str()), ("Edit", "hola.txt"));
                assert!(input["paths"][0].as_str().unwrap().ends_with("hola.txt"));
                Some(request_id.clone())
            }
            _ => None,
        });
        session.answer(&request, true, None).unwrap();
        let (_, seen) = until(&rx, |e| {
            matches!(e, AgentEvent::Finished { .. }).then_some(())
        });
        assert!(seen.contains(&AgentEvent::FileEdited {
            path: "hola.txt".into()
        }));
        assert!(seen.contains(&AgentEvent::RateLimit {
            status: "allowed".into(),
            utilization: Some(95.0),
            resets_at: Some(123)
        }));
        assert!(seen.iter().any(
            |e| matches!(e, AgentEvent::Usage(u) if u.input_tokens == 6 && u.cache_read_tokens == 4)
        ));
        assert!(
            matches!(seen.last(), Some(AgentEvent::Finished { ok: true, summary, .. }) if summary.contains(&prompts::plan_approved()))
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("hola.txt")).unwrap(),
            "hola"
        );
        session.stop();
    }

    #[test]
    fn declined_changes_do_not_happen() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let (mut session, rx) = start(&dir, false);
        let (request, _) = until(&rx, |e| match e {
            AgentEvent::PermissionRequest { request_id, .. } => Some(request_id.clone()),
            _ => None,
        });
        session.answer(&request, false, Some("no")).unwrap();
        let (_, seen) = until(&rx, |e| {
            matches!(e, AgentEvent::Finished { .. }).then_some(())
        });
        assert!(
            !seen
                .iter()
                .any(|e| matches!(e, AgentEvent::FileEdited { .. }))
        );
        assert!(!dir.join("hola.txt").exists());
        session.stop();
    }

    /// Answers the handshake and records each `turn/start`'s input.
    const RECORDING_CODEX: &str = r#"
import json, sys
log = open(sys.argv[1], "a")
def out(obj): print(json.dumps(obj), flush=True)
for line in sys.stdin:
    msg = json.loads(line)
    method, mid, p = msg.get("method"), msg.get("id"), msg.get("params", {})
    if method == "initialize":
        out({"jsonrpc": "2.0", "id": mid, "result": {"userAgent": "fake"}})
    elif method == "thread/start":
        out({"jsonrpc": "2.0", "id": mid, "result": {"thread": {"id": "th-1"}, "model": "gpt"}})
    elif method == "turn/start":
        log.write(json.dumps(p["input"]) + "\n"); log.flush()
        out({"jsonrpc": "2.0", "id": mid, "result": {"turn": {"id": "tu"}}})
        out({"jsonrpc": "2.0", "method": "turn/completed", "params": {"threadId": "th-1",
             "turn": {"id": "tu", "status": "completed", "items": [], "durationMs": 1}}})
"#;

    #[test]
    fn images_go_as_local_image_items() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let image = crate::adapters::images::tests::png(&dir);
        let script = dir.join("recording_codex.py");
        std::fs::write(&script, RECORDING_CODEX).unwrap();
        let log = dir.join("turns.jsonl");
        let request = AgentRequest {
            backend: AgentBackend::Codex,
            launch: LaunchSpec::program(
                "python3",
                vec![
                    "-I".into(),
                    script.to_string_lossy().into_owned(),
                    log.to_string_lossy().into_owned(),
                ],
            )
            .in_dir(dir.clone()),
            mode: TaskMode::Ask,
            phase: Phase::Implement,
            plan_first: false,
            model: None,
            prompt: "mira".into(),
            images: vec![image.clone()],
            resume: None,
            memory: None,
            mcp: vec![],
            conversation: false,
            no_shell: false,
            no_tools: false,
            plugins: Vec::new(),
        };
        let mut session = CodexLauncher.start(request, Arc::new(|_| {})).unwrap();
        let turns = crate::adapters::claude::tests::lines_when(&log, 1);
        assert_eq!(
            turns[0],
            json!([{"type": "text", "text": "mira"}, {"type": "localImage", "path": image.path}])
        );
        session.send("otra", &[]).unwrap();
        let turns = crate::adapters::claude::tests::lines_when(&log, 2);
        assert_eq!(turns[1], json!([{"type": "text", "text": "otra"}]));
        session.stop();
    }
}
