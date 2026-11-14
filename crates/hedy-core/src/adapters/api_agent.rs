//! An agent over an OpenAI-compatible chat-completions API (Kimi, Gemini).
//!
//! The model gets a few tools and Hedy runs them itself, inside the task's
//! folder: reads and searches directly (never protected files, never outside
//! the folder), writes and commands only after a permission answer.

use std::collections::VecDeque;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::process::command;
use crate::application::ports::{
    AgentBackend, AgentLauncher, AgentRequest, AgentSession, EventSink, PortError, PortResult,
    Secret,
};
use crate::domain::agent::looks_like_tests;
use crate::domain::permissions::{DEFAULT_BLOCKED, protected_file_reason};
use crate::domain::{AgentEvent, LaunchSpec, TaskMode, UsageSample, prompts};

pub struct ApiAgentLauncher;

/// Tool rounds in one turn before giving up.
const MAX_ROUNDS: usize = 60;
const MAX_READ: usize = 200_000;
const MAX_OUTPUT: usize = 20_000;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(600);
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".hedy",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "dist",
    "build",
    "__pycache__",
];

enum Command {
    Message(String),
    Answer {
        id: String,
        allow: bool,
        message: Option<String>,
    },
    Stop,
}

/// Why a turn ended early.
enum End {
    Stopped,
    Failed(String),
}

pub struct ApiSession {
    commands: mpsc::Sender<Command>,
    stopped: Arc<AtomicBool>,
}

impl AgentLauncher for ApiAgentLauncher {
    fn start(&self, request: AgentRequest, events: EventSink) -> PortResult<Box<dyn AgentSession>> {
        let AgentBackend::OpenAiCompatible { base_url, api_key } = request.backend.clone() else {
            return Err(PortError::new(
                "el agente por API necesita una URL y una API key",
            ));
        };
        let model = request
            .model
            .clone()
            .ok_or_else(|| PortError::new("el agente por API necesita un modelo"))?;
        let (tx, rx) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let read_only = request.mode != TaskMode::Project || request.phase.read_only();
        let mut agent = Agent {
            http: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(300)))
                .http_status_as_error(false)
                .build()
                .into(),
            url: format!("{}/chat/completions", base_url.trim_end_matches('/')),
            key: api_key,
            model,
            cwd: request.launch.cwd.clone().unwrap_or_default(),
            events,
            read_only,
            planning: request.plan_first,
            plans: 0,
            messages: Vec::new(),
            queued: VecDeque::new(),
            stopped: Arc::clone(&stopped),
        };
        let prompt = request.prompt.clone();
        let mode = request.mode;
        std::thread::spawn(move || agent.run(rx, mode, prompt));
        Ok(Box::new(ApiSession {
            commands: tx,
            stopped,
        }))
    }
}

impl AgentSession for ApiSession {
    fn send(&mut self, text: &str) -> PortResult<()> {
        self.commands
            .send(Command::Message(text.to_owned()))
            .map_err(|_| PortError::new("el agente por API ya terminó"))
    }

    fn answer(&mut self, request_id: &str, allow: bool, message: Option<&str>) -> PortResult<()> {
        self.commands
            .send(Command::Answer {
                id: request_id.to_owned(),
                allow,
                message: message.map(str::to_owned),
            })
            .map_err(|_| PortError::new("el agente por API ya terminó"))
    }

    fn stop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.commands.send(Command::Stop);
    }
}

impl Drop for ApiSession {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Agent {
    http: ureq::Agent,
    url: String,
    key: Secret,
    model: String,
    cwd: PathBuf,
    events: EventSink,
    read_only: bool,
    planning: bool,
    plans: u32,
    messages: Vec<Value>,
    /// User messages that arrived while a tool waited for an answer.
    queued: VecDeque<String>,
    stopped: Arc<AtomicBool>,
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"type": "function", "function": {
        "name": name,
        "description": description,
        "parameters": {"type": "object", "properties": properties, "required": required},
    }})
}

fn tools(read_only: bool) -> Value {
    let path = json!({"type": "string", "description": "Ruta relativa a la carpeta de trabajo"});
    let mut list = vec![
        tool(
            "list_files",
            "Lista archivos y carpetas (recursivo, sin dependencias).",
            json!({"path": path}),
            &[],
        ),
        tool(
            "read_file",
            "Lee un archivo de texto.",
            json!({"path": path}),
            &["path"],
        ),
        tool(
            "search",
            "Busca un texto (sin distinguir mayúsculas) en los archivos.",
            json!({"pattern": {"type": "string"}, "path": path}),
            &["pattern"],
        ),
    ];
    if !read_only {
        list.extend([
            tool(
                "write_file",
                "Crea o reemplaza un archivo completo.",
                json!({"path": path, "content": {"type": "string"}}),
                &["path", "content"],
            ),
            tool(
                "edit_file",
                "Reemplaza un fragmento exacto (que aparezca una sola vez) por otro.",
                json!({"path": path, "old_string": {"type": "string"}, "new_string": {"type": "string"}}),
                &["path", "old_string", "new_string"],
            ),
            tool(
                "run_command",
                "Ejecuta un comando de shell en la carpeta de trabajo (por ejemplo, los tests).",
                json!({"command": {"type": "string"}}),
                &["command"],
            ),
        ]);
    }
    Value::Array(list)
}

fn truncate(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut cut = max;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n[…recortado]");
    }
    text
}

impl Agent {
    fn emit(&self, event: AgentEvent) {
        (self.events)(event);
    }

    fn system_prompt(&self, mode: TaskMode) -> String {
        let mut prompt = format!(
            "Eres un agente de programación trabajando a través de Hedy en la carpeta {}. \
             Usa las herramientas para leer, buscar y, si están disponibles, cambiar archivos y correr comandos. \
             Las rutas son relativas a esa carpeta. Sé breve y responde en español.",
            self.cwd.display()
        );
        match mode {
            TaskMode::Ask => {
                prompt.push_str(" Modo Pregunta: responde y explica; no modifiques archivos.")
            }
            TaskMode::Research => prompt
                .push_str(" Modo Research: investiga en el código y cita dónde está cada cosa."),
            TaskMode::Project => {}
        }
        if self.planning {
            prompt.push(' ');
            prompt.push_str(prompts::PLAN_FIRST);
        }
        prompt
    }

    fn run(&mut self, rx: mpsc::Receiver<Command>, mode: TaskMode, prompt: String) {
        self.emit(AgentEvent::SessionStarted {
            session_id: format!("api-{}", std::process::id()),
            model: self.model.clone(),
        });
        self.messages
            .push(json!({"role": "system", "content": self.system_prompt(mode)}));
        self.messages
            .push(json!({"role": "user", "content": prompt}));
        loop {
            let started = Instant::now();
            match self.turn(&rx) {
                Ok(text) if self.planning => {
                    self.plans += 1;
                    let id = format!("api-plan-{}", self.plans);
                    self.emit(AgentEvent::Plan {
                        text,
                        request_id: Some(id.clone()),
                    });
                    match self.wait_answer(&rx, &id) {
                        Ok((true, _)) => {
                            self.planning = false;
                            self.messages
                                .push(json!({"role": "user", "content": prompts::PLAN_APPROVED}));
                        }
                        Ok((false, message)) => {
                            let feedback = message.unwrap_or_else(|| "Revisa el plan.".into());
                            self.messages
                                .push(json!({"role": "user", "content": feedback}));
                        }
                        Err(_) => return,
                    }
                    continue;
                }
                Ok(text) => self.emit(AgentEvent::Finished {
                    ok: true,
                    summary: text,
                    duration_ms: Some(started.elapsed().as_millis() as u64),
                }),
                Err(End::Stopped) => return,
                Err(End::Failed(message)) => {
                    self.emit(AgentEvent::Error {
                        message: message.clone(),
                    });
                    self.emit(AgentEvent::Finished {
                        ok: false,
                        summary: message,
                        duration_ms: None,
                    });
                }
            }
            // Wait for the next user message.
            let next = match self.queued.pop_front() {
                Some(text) => text,
                None => loop {
                    match rx.recv() {
                        Ok(Command::Message(text)) => break text,
                        Ok(Command::Answer { .. }) => continue,
                        Ok(Command::Stop) | Err(_) => return,
                    }
                },
            };
            self.messages.push(json!({"role": "user", "content": next}));
        }
    }

    /// Calls the model until it answers without tools. Returns its last text.
    fn turn(&mut self, rx: &mpsc::Receiver<Command>) -> Result<String, End> {
        for _ in 0..MAX_ROUNDS {
            let response = self.complete()?;
            if self.stopped.load(Ordering::SeqCst) {
                return Err(End::Stopped);
            }
            self.report_usage(&response["usage"]);
            let message = response["choices"][0]["message"].clone();
            if !message.is_object() {
                return Err(End::Failed("La API respondió sin mensaje.".into()));
            }
            self.messages.push(message.clone());
            let text = message["content"].as_str().unwrap_or("").trim().to_owned();
            let calls = message["tool_calls"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !text.is_empty() && !(calls.is_empty() && self.planning) {
                self.emit(AgentEvent::Message { text: text.clone() });
            }
            if calls.is_empty() {
                return Ok(text);
            }
            for call in calls {
                let result = self.run_tool(&call, rx)?;
                self.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call["id"],
                    "content": result,
                }));
            }
        }
        Err(End::Failed(
            "Demasiados pasos seguidos sin terminar; me detengo aquí.".into(),
        ))
    }

    fn complete(&self) -> Result<Value, End> {
        let body = json!({
            "model": self.model,
            "messages": self.messages,
            "tools": tools(self.read_only || self.planning),
            "tool_choice": "auto",
        });
        let mut response = self
            .http
            .post(&self.url)
            .header("Authorization", &format!("Bearer {}", self.key.expose()))
            .send_json(&body)
            .map_err(|e| End::Failed(format!("No se pudo llamar a la API: {e}")))?;
        let status = response.status().as_u16();
        let json: Value = response.body_mut().read_json().unwrap_or(Value::Null);
        let detail = json["error"]["message"].as_str().unwrap_or("").to_owned();
        match status {
            200..=299 => Ok(json),
            401 | 403 => Err(End::Failed(format!(
                "La API rechazó la key (HTTP {status}). {detail}"
            ))),
            429 => {
                self.emit(AgentEvent::RateLimit {
                    status: "rejected".into(),
                    utilization: None,
                    resets_at: None,
                });
                Err(End::Failed(format!(
                    "La API llegó a su límite (HTTP 429). {detail}"
                )))
            }
            _ => Err(End::Failed(format!(
                "La API respondió HTTP {status}. {detail}"
            ))),
        }
    }

    fn report_usage(&self, usage: &Value) {
        if !usage.is_object() {
            return;
        }
        let n = |v: &Value| v.as_u64().unwrap_or(0);
        let cached =
            n(&usage["prompt_tokens_details"]["cached_tokens"]).max(n(&usage["cached_tokens"]));
        self.emit(AgentEvent::Usage(UsageSample {
            input_tokens: n(&usage["prompt_tokens"]).saturating_sub(cached),
            output_tokens: n(&usage["completion_tokens"]),
            cache_read_tokens: cached,
            cache_write_tokens: 0,
            cost_usd: None,
        }));
    }

    /// Waits for the answer to `request_id`; queues user messages meanwhile.
    fn wait_answer(
        &mut self,
        rx: &mpsc::Receiver<Command>,
        request_id: &str,
    ) -> Result<(bool, Option<String>), End> {
        loop {
            match rx.recv() {
                Ok(Command::Answer { id, allow, message }) if id == request_id => {
                    return Ok((allow, message));
                }
                Ok(Command::Answer { .. }) => {}
                Ok(Command::Message(text)) => self.queued.push_back(text),
                Ok(Command::Stop) | Err(_) => return Err(End::Stopped),
            }
        }
    }

    /// `path` inside the working folder, or why not.
    fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let joined = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.cwd.join(path)
        };
        let mut out = PathBuf::new();
        for part in joined.components() {
            match part {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other),
            }
        }
        if out.starts_with(&self.cwd) {
            Ok(out)
        } else {
            Err("Solo puedo trabajar dentro de la carpeta de la tarea.".into())
        }
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.cwd)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned())
    }

    fn protected(&self, tool: &str, input: &Value) -> Option<String> {
        let blocked: Vec<String> = DEFAULT_BLOCKED.iter().map(|s| s.to_string()).collect();
        protected_file_reason(tool, input, &self.cwd, &blocked)
    }

    fn run_tool(&mut self, call: &Value, rx: &mpsc::Receiver<Command>) -> Result<String, End> {
        let name = call["function"]["name"].as_str().unwrap_or("");
        let args: Value =
            serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or("{}"))
                .unwrap_or(Value::Null);
        let id = call["id"].as_str().unwrap_or("call").to_owned();
        let arg = |key: &str| args[key].as_str().unwrap_or("").to_owned();
        let writes = matches!(name, "write_file" | "edit_file" | "run_command");
        if writes && (self.read_only || self.planning) {
            return Ok("Ahora no puedes cambiar archivos ni correr comandos.".into());
        }
        Ok(match name {
            "list_files" => self.list_files(&arg("path")),
            "read_file" => self.read_file(&arg("path")),
            "search" => self.search(&arg("pattern"), &arg("path")),
            "write_file" | "edit_file" => {
                let target = match self.resolve(&arg("path")) {
                    Ok(p) => p,
                    Err(e) => return Ok(e),
                };
                let tool = if name == "write_file" {
                    "Write"
                } else {
                    "Edit"
                };
                let input = json!({"file_path": target});
                if let Some(reason) = self.protected(tool, &input) {
                    return Ok(reason);
                }
                self.emit(AgentEvent::PermissionRequest {
                    request_id: id.clone(),
                    tool: tool.into(),
                    summary: self.relative(&target),
                    input,
                });
                match self.wait_answer(rx, &id)? {
                    (true, _) => self.change_file(name, &target, &args),
                    (false, message) => format!("Rechazado: {}", message.unwrap_or_default()),
                }
            }
            "run_command" => {
                let command = arg("command");
                let input = json!({"command": command});
                if let Some(reason) = self.protected("Bash", &input) {
                    return Ok(reason);
                }
                self.emit(AgentEvent::PermissionRequest {
                    request_id: id.clone(),
                    tool: "Bash".into(),
                    summary: command.clone(),
                    input,
                });
                match self.wait_answer(rx, &id)? {
                    (true, _) => self.run_command(&command),
                    (false, message) => format!("Rechazado: {}", message.unwrap_or_default()),
                }
            }
            other => format!("Herramienta desconocida: {other}"),
        })
    }

    fn read_file(&self, path: &str) -> String {
        let target = match self.resolve(path) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if let Some(reason) = self.protected("Read", &json!({"file_path": target})) {
            return reason;
        }
        self.emit(AgentEvent::ToolUsed {
            tool: "Read".into(),
            summary: self.relative(&target),
        });
        match std::fs::read(&target) {
            Ok(bytes) => truncate(String::from_utf8_lossy(&bytes).into_owned(), MAX_READ),
            Err(e) => format!("No se pudo leer: {e}"),
        }
    }

    fn walk(&self, start: &Path, mut visit: impl FnMut(&Path, bool) -> bool) {
        let mut stack = vec![start.to_owned()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
                if is_dir && SKIPPED_DIRS.contains(&name.as_str()) {
                    continue;
                }
                if !visit(&path, is_dir) {
                    return;
                }
                if is_dir {
                    stack.push(path);
                }
            }
        }
    }

    fn list_files(&self, path: &str) -> String {
        let start = match self.resolve(if path.is_empty() { "." } else { path }) {
            Ok(p) => p,
            Err(e) => return e,
        };
        self.emit(AgentEvent::ToolUsed {
            tool: "Glob".into(),
            summary: self.relative(&start),
        });
        let mut lines = Vec::new();
        self.walk(&start, |p, is_dir| {
            lines.push(format!(
                "{}{}",
                self.relative(p),
                if is_dir { "/" } else { "" }
            ));
            lines.len() < 400
        });
        if lines.is_empty() {
            "(vacío)".into()
        } else {
            lines.join("\n")
        }
    }

    fn search(&self, pattern: &str, path: &str) -> String {
        let start = match self.resolve(if path.is_empty() { "." } else { path }) {
            Ok(p) => p,
            Err(e) => return e,
        };
        self.emit(AgentEvent::ToolUsed {
            tool: "Grep".into(),
            summary: pattern.to_owned(),
        });
        let needle = pattern.to_lowercase();
        let mut hits = Vec::new();
        self.walk(&start, |p, is_dir| {
            if is_dir || self.protected("Read", &json!({"file_path": p})).is_some() {
                return true;
            }
            if std::fs::metadata(p).is_ok_and(|m| m.len() < 1_000_000)
                && let Ok(text) = std::fs::read_to_string(p)
            {
                for (n, line) in text.lines().enumerate() {
                    if line.to_lowercase().contains(&needle) {
                        hits.push(format!("{}:{}: {}", self.relative(p), n + 1, line.trim()));
                    }
                }
            }
            hits.len() < 100
        });
        if hits.is_empty() {
            "Sin resultados.".into()
        } else {
            hits.join("\n")
        }
    }

    fn change_file(&self, name: &str, target: &Path, args: &Value) -> String {
        let result = if name == "write_file" {
            target
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(target, args["content"].as_str().unwrap_or("")))
                .map_err(|e| e.to_string())
        } else {
            let old = args["old_string"].as_str().unwrap_or("");
            let new = args["new_string"].as_str().unwrap_or("");
            match std::fs::read_to_string(target) {
                Ok(text) if old.is_empty() || text.matches(old).count() != 1 => {
                    Err("el fragmento a reemplazar debe aparecer exactamente una vez".into())
                }
                Ok(text) => {
                    std::fs::write(target, text.replacen(old, new, 1)).map_err(|e| e.to_string())
                }
                Err(e) => Err(e.to_string()),
            }
        };
        match result {
            Ok(()) => {
                self.emit(AgentEvent::FileEdited {
                    path: self.relative(target),
                });
                "Hecho.".into()
            }
            Err(e) => format!("No se pudo cambiar el archivo: {e}"),
        }
    }

    fn run_command(&self, command_line: &str) -> String {
        let spec = LaunchSpec::program("sh", vec!["-c".into(), command_line.to_owned()])
            .in_dir(self.cwd.clone());
        let child = command(&spec)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => return format!("No se pudo ejecutar: {e}"),
        };
        let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = out.read_to_string(&mut text);
            let mut errors = String::new();
            let _ = err.read_to_string(&mut errors);
            text + &errors
        });
        let started = Instant::now();
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None)
                    if started.elapsed() > COMMAND_TIMEOUT
                        || self.stopped.load(Ordering::SeqCst) =>
                {
                    let _ = child.kill();
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => break None,
            }
        };
        let output = truncate(reader.join().unwrap_or_default(), MAX_OUTPUT);
        let passed = code == Some(0);
        self.emit(if looks_like_tests(command_line) {
            AgentEvent::TestsRun {
                command: command_line.to_owned(),
                passed,
            }
        } else {
            AgentEvent::CommandRun {
                command: command_line.to_owned(),
                tests: false,
            }
        });
        format!(
            "Código de salida: {}\n{output}",
            code.map_or("?".into(), |c| c.to_string())
        )
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    use super::*;
    use crate::domain::Phase;

    type Seen = Arc<Mutex<Vec<(String, Value)>>>;

    /// A local HTTP server that answers each request with the next response.
    fn serve(responses: Vec<Value>) -> (String, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let seen: Seen = Arc::default();
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for (stream, body) in listener.incoming().zip(responses) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let (mut auth, mut length) = (String::new(), 0usize);
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    let lower = line.to_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    if lower.starts_with("authorization:") {
                        auth = line["authorization:".len()..].trim().to_owned();
                    }
                }
                let mut request = vec![0; length];
                std::io::Read::read_exact(&mut reader, &mut request).unwrap();
                log.lock()
                    .unwrap()
                    .push((auth, serde_json::from_slice(&request).unwrap()));
                let text = body.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                )
                .unwrap();
            }
        });
        (url, seen)
    }

    fn reply(content: &str, calls: Value) -> Value {
        json!({"choices": [{"message": {"role": "assistant", "content": content, "tool_calls": calls}}],
               "usage": {"prompt_tokens": 50, "completion_tokens": 5, "prompt_tokens_details": {"cached_tokens": 20}}})
    }

    fn call(id: &str, name: &str, args: Value) -> Value {
        json!({"id": id, "type": "function", "function": {"name": name, "arguments": args.to_string()}})
    }

    fn start(
        url: String,
        dir: &Path,
        mode: TaskMode,
        phase: Phase,
    ) -> (Box<dyn AgentSession>, mpsc::Receiver<AgentEvent>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let request = AgentRequest {
            backend: AgentBackend::OpenAiCompatible {
                base_url: url,
                api_key: Secret::new("sk-test"),
            },
            launch: LaunchSpec::program("", vec![]).in_dir(dir.to_owned()),
            mode,
            phase,
            plan_first: false,
            model: Some("kimi-k3".into()),
            prompt: "Crea hola.txt".into(),
            resume: None,
        };
        let session = ApiAgentLauncher
            .start(request, Arc::new(move |e| drop(tx.lock().unwrap().send(e))))
            .unwrap();
        (session, rx)
    }

    fn until(
        rx: &mpsc::Receiver<AgentEvent>,
        stop: impl Fn(&AgentEvent) -> bool,
    ) -> Vec<AgentEvent> {
        let mut seen = Vec::new();
        loop {
            let e = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("no event from the API agent");
            let done = stop(&e);
            seen.push(e);
            if done {
                return seen;
            }
        }
    }

    #[test]
    fn writes_only_after_permission_and_reports_usage() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let (url, seen) = serve(vec![
            reply(
                "Lo creo.",
                json!([call(
                    "c1",
                    "write_file",
                    json!({"path": "hola.txt", "content": "hola"})
                )]),
            ),
            reply("Listo.", json!(null)),
        ]);
        let (mut session, rx) = start(url, &dir, TaskMode::Project, Phase::Implement);

        let events = until(&rx, |e| matches!(e, AgentEvent::PermissionRequest { .. }));
        assert!(events.contains(&AgentEvent::Message {
            text: "Lo creo.".into()
        }));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Usage(u) if u.input_tokens == 30 && u.cache_read_tokens == 20)));
        assert!(!dir.join("hola.txt").exists(), "nothing before the answer");
        session.answer("c1", true, None).unwrap();

        let events = until(&rx, |e| matches!(e, AgentEvent::Finished { .. }));
        assert!(events.contains(&AgentEvent::FileEdited {
            path: "hola.txt".into()
        }));
        assert!(
            matches!(events.last(), Some(AgentEvent::Finished { ok: true, summary, .. }) if summary == "Listo.")
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("hola.txt")).unwrap(),
            "hola"
        );

        let requests = seen.lock().unwrap();
        assert_eq!(requests[0].0, "Bearer sk-test");
        assert_eq!(requests[0].1["model"], "kimi-k3");
        let tool_names: Vec<_> = requests[0].1["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect();
        assert!(tool_names.contains(&"run_command"));
        let last = requests[1].1["messages"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .clone();
        assert_eq!(
            (last["role"].as_str(), last["tool_call_id"].as_str()),
            (Some("tool"), Some("c1"))
        );
        session.stop();
    }

    #[test]
    fn read_only_agents_cannot_write_or_read_secrets() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        std::fs::write(dir.join(".env"), "SECRETO=42").unwrap();
        std::fs::write(dir.join("app.py"), "def crear_cita(): pass\n").unwrap();
        let (url, seen) = serve(vec![
            reply(
                "",
                json!([
                    call("r1", "read_file", json!({"path": ".env"})),
                    call("r2", "read_file", json!({"path": "../../etc/passwd"})),
                    call("r3", "search", json!({"pattern": "crear_cita"})),
                    call("w1", "write_file", json!({"path": "x.py", "content": "x"})),
                ]),
            ),
            reply("Está en app.py.", json!(null)),
        ]);
        let (_session, rx) = start(url, &dir, TaskMode::Ask, Phase::Implement);
        let events = until(&rx, |e| matches!(e, AgentEvent::Finished { .. }));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, AgentEvent::PermissionRequest { .. }))
        );

        let requests = seen.lock().unwrap();
        let tool_names: Vec<_> = requests[0].1["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect();
        assert_eq!(tool_names, vec!["list_files", "read_file", "search"]);
        let results: Vec<String> = requests[1].1["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "tool")
            .map(|m| m["content"].as_str().unwrap().to_owned())
            .collect();
        assert!(results[0].contains("Hedy no permite") && !results[0].contains("42"));
        assert!(results[1].contains("dentro de la carpeta"));
        assert!(results[2].contains("app.py:1: def crear_cita(): pass"));
        assert!(results[3].contains("no puedes cambiar"));
        assert!(!dir.join("x.py").exists());
    }
}
