//! An agent over an OpenAI-compatible chat-completions API (Kimi, Gemini).
//!
//! The model gets a few tools and Hedy runs them itself, inside the task's
//! folder: reads and searches directly (never protected files, never outside
//! the folder), writes and commands only after a permission answer.

use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::paths::relative;
use super::process::command;
use crate::application::ports::{
    AgentBackend, AgentLauncher, AgentRequest, AgentSession, EventSink, MemoryTools, PortError,
    PortResult, Secret, SkillTools,
};
use crate::domain::agent::looks_like_tests;
use crate::domain::permissions::{normalize, protected_file_reason};
use crate::domain::task::SHARED_DEPENDENCY_DIRS;
use crate::domain::{AgentEvent, LaunchSpec, Phase, PromptImage, TaskMode, UsageSample, prompts};
use crate::tr;

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
    Message(String, Vec<PromptImage>),
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
        let read_only = request.phase.read_only_in(request.mode);
        let cwd = request.launch.cwd.clone().unwrap_or_default();
        // Real paths, so symlinks cannot lead out of the folder.
        let cwd = cwd.canonicalize().unwrap_or(cwd);
        // Dependencies shared with the main checkout (symlinks Hedy made) can
        // be read, never written.
        let shared = SHARED_DEPENDENCY_DIRS
            .iter()
            .filter_map(|dir| cwd.join(dir).canonicalize().ok())
            .filter(|target| !target.starts_with(&cwd))
            .collect();
        let mut agent = Agent {
            http: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(300)))
                .http_status_as_error(false)
                .build()
                .into(),
            url: format!("{}/chat/completions", base_url.trim_end_matches('/')),
            key: api_key,
            model,
            cwd,
            shared,
            events,
            read_only,
            phase: request.phase,
            planning: request.plan_first,
            plans: 0,
            messages: Vec::new(),
            queued: VecDeque::new(),
            stopped: Arc::clone(&stopped),
            memory: request.memory.as_ref().map(|m| Arc::clone(&m.tools)),
            skills: request.memory.as_ref().and_then(|m| m.skills.clone()),
        };
        let prompt = request.prompt.clone();
        let images = request.images.clone();
        let mode = request.mode;
        std::thread::spawn(move || agent.run(rx, mode, prompt, images));
        Ok(Box::new(ApiSession {
            commands: tx,
            stopped,
        }))
    }
}

impl AgentSession for ApiSession {
    fn send(&mut self, text: &str, images: &[PromptImage]) -> PortResult<()> {
        self.commands
            .send(Command::Message(text.to_owned(), images.to_vec()))
            .map_err(|_| {
                PortError::new(tr!(
                    "el agente por API ya terminó",
                    "the API agent has ended"
                ))
            })
    }

    fn answer(&mut self, request_id: &str, allow: bool, message: Option<&str>) -> PortResult<()> {
        self.commands
            .send(Command::Answer {
                id: request_id.to_owned(),
                allow,
                message: message.map(str::to_owned),
            })
            .map_err(|_| {
                PortError::new(tr!(
                    "el agente por API ya terminó",
                    "the API agent has ended"
                ))
            })
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
    /// The task folder, canonical.
    cwd: PathBuf,
    /// Canonical targets of the shared dependency folders (read-only).
    shared: Vec<PathBuf>,
    events: EventSink,
    read_only: bool,
    phase: Phase,
    planning: bool,
    plans: u32,
    messages: Vec<Value>,
    /// User messages that arrived while a tool waited for an answer.
    queued: VecDeque<(String, Vec<PromptImage>)>,
    stopped: Arc<AtomicBool>,
    /// The project's memory, when this account reads it.
    memory: Option<Arc<dyn MemoryTools>>,
    /// The learned skills, next to the memory.
    skills: Option<Arc<dyn SkillTools>>,
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"type": "function", "function": {
        "name": name,
        "description": description,
        "parameters": {"type": "object", "properties": properties, "required": required},
    }})
}

fn memory_tools() -> [Value; 4] {
    [
        tool(
            "memory_search",
            "Busca en la memoria del proyecto (decisiones, convenciones, reglas). Devuelve un resumen por nota: id, tema y el comienzo del texto. Vacío: todas.",
            json!({"query": {"type": "string"}}),
            &[],
        ),
        tool(
            "memory_get",
            "Lee una nota completa de la memoria, por id (de memory_search) o por tema.",
            json!({"id": {"type": "integer"}, "topic": {"type": "string"}}),
            &[],
        ),
        tool(
            "memory_topics",
            "Lista los temas de la memoria del proyecto, con la nota de cada uno.",
            json!({}),
            &[],
        ),
        tool(
            "memory_propose",
            "Propone una nota para la memoria del proyecto; el usuario decide si se guarda. Con el tema de una nota existente, propone un cambio de esa nota.",
            json!({"text": {"type": "string"},
                   "category": {"type": "string", "enum": ["decision", "convention", "rule", "preference"]},
                   "topic": {"type": "string", "description": "Tema estable en minúsculas, p. ej. arquitectura/pagos (opcional)"}}),
            &["text", "category"],
        ),
    ]
}

fn skill_tools() -> [Value; 3] {
    [
        tool(
            "skill_list",
            "Lista las habilidades aprobadas del proyecto (procedimientos reutilizables): nombre y cuándo usar cada una.",
            json!({}),
            &[],
        ),
        tool(
            "skill_get",
            "Lee una habilidad aprobada completa, por nombre.",
            json!({"name": {"type": "string"}}),
            &["name"],
        ),
        tool(
            "skill_propose",
            "Propone una habilidad (un procedimiento reutilizable que no es obvio); el usuario decide si se aprueba. Con el nombre de una existente, propone su actualización. Sin secretos.",
            json!({"name": {"type": "string", "description": "Nombre corto en minúsculas con guiones, p. ej. correr-tests-e2e"},
                   "description": {"type": "string", "description": "Una línea: cuándo usarla"},
                   "body": {"type": "string", "description": "Los pasos, en Markdown"}}),
            &["name", "description", "body"],
        ),
    ]
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

/// The end of a command's output: summaries and errors come last.
fn keep_tail(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("[…recortado]\n{}", &text[start..])
}

impl Agent {
    fn emit(&self, event: AgentEvent) {
        (self.events)(event);
    }

    fn tools(&self) -> Value {
        let mut list = tools(self.read_only || self.planning);
        if self.memory.is_some()
            && let Value::Array(list) = &mut list
        {
            list.extend(memory_tools());
        }
        if self.skills.is_some()
            && let Value::Array(list) = &mut list
        {
            list.extend(skill_tools());
        }
        list
    }

    fn system_prompt(&self, mode: TaskMode) -> String {
        let folder = self.cwd.display();
        let mut prompt = tr!(
            "Eres un agente de programación trabajando a través de Hedy en la carpeta {folder}. Usa las herramientas para leer, buscar y, si están disponibles, cambiar archivos y correr comandos. Las rutas son relativas a esa carpeta. Sé breve y responde en español.",
            "You are a coding agent working through Hedy in the folder {folder}. Use the tools to read, search and, when available, change files and run commands. Paths are relative to that folder. Be brief and reply in English."
        );
        match mode {
            TaskMode::Ask => prompt.push_str(&tr!(
                " Modo Pregunta: responde y explica; no modifiques archivos.",
                " Ask mode: answer and explain; do not modify files."
            )),
            TaskMode::Research => prompt.push_str(&tr!(
                " Modo Research: investiga en el código y cita dónde está cada cosa.",
                " Research mode: investigate the code and cite where each thing is."
            )),
            TaskMode::Project => {}
        }
        if self.memory.is_some() {
            prompt.push(' ');
            prompt.push_str(&prompts::memory_guidance(self.phase, mode));
        }
        if self.planning {
            prompt.push(' ');
            prompt.push_str(&prompts::plan_first());
        }
        prompt
    }

    /// A user message, with its images as content parts.
    fn push_user(&mut self, text: &str, images: &[PromptImage]) -> Result<(), String> {
        let content = super::images::openai_content(text, images).map_err(|e| e.message)?;
        self.messages
            .push(json!({"role": "user", "content": content}));
        Ok(())
    }

    fn run(
        &mut self,
        rx: mpsc::Receiver<Command>,
        mode: TaskMode,
        prompt: String,
        images: Vec<PromptImage>,
    ) {
        self.emit(AgentEvent::SessionStarted {
            session_id: format!(
                "api-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ),
            model: self.model.clone(),
        });
        self.messages
            .push(json!({"role": "system", "content": self.system_prompt(mode)}));
        let mut pushed = self.push_user(&prompt, &images);
        loop {
            if let Err(message) = pushed {
                self.emit(AgentEvent::Error {
                    message: message.clone(),
                });
                self.emit(AgentEvent::Finished {
                    ok: false,
                    summary: message,
                    duration_ms: None,
                });
                return;
            }
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
                                .push(json!({"role": "user", "content": prompts::plan_approved()}));
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
            let (text, images) = match self.queued.pop_front() {
                Some(next) => next,
                None => loop {
                    match rx.recv() {
                        Ok(Command::Message(text, images)) => break (text, images),
                        Ok(Command::Answer { .. }) => continue,
                        Ok(Command::Stop) | Err(_) => return,
                    }
                },
            };
            pushed = self.push_user(&text, &images);
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
                return Err(End::Failed(tr!(
                    "La API respondió sin mensaje.",
                    "The API answered without a message."
                )));
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
        Err(End::Failed(tr!(
            "Demasiados pasos seguidos sin terminar; me detengo aquí.",
            "Too many steps in a row without finishing; stopping here."
        )))
    }

    fn complete(&self) -> Result<Value, End> {
        let body = json!({
            "model": self.model,
            "messages": self.messages,
            "tools": self.tools(),
            "tool_choice": "auto",
        });
        let mut response = self
            .http
            .post(&self.url)
            .header("Authorization", &format!("Bearer {}", self.key.expose()))
            .send_json(&body)
            .map_err(|e| {
                End::Failed(tr!(
                    "No se pudo llamar a la API: {e}",
                    "Could not reach the API: {e}"
                ))
            })?;
        let status = response.status().as_u16();
        let json: Value = response.body_mut().read_json().unwrap_or(Value::Null);
        let detail = json["error"]["message"].as_str().unwrap_or("").to_owned();
        match status {
            200..=299 => Ok(json),
            401 | 403 => Err(End::Failed(tr!(
                "La API rechazó la key (HTTP {status}). {detail}",
                "The API rejected the key (HTTP {status}). {detail}"
            ))),
            429 => {
                self.emit(AgentEvent::RateLimit {
                    status: "rejected".into(),
                    utilization: None,
                    resets_at: None,
                });
                Err(End::Failed(tr!(
                    "La API llegó a su límite (HTTP 429). {detail}",
                    "The API hit its limit (HTTP 429). {detail}"
                )))
            }
            _ => Err(End::Failed(tr!(
                "La API respondió HTTP {status}. {detail}",
                "The API answered HTTP {status}. {detail}"
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
                Ok(Command::Message(text, images)) => self.queued.push_back((text, images)),
                Ok(Command::Stop) | Err(_) => return Err(End::Stopped),
            }
        }
    }

    /// The real path of `path` (symlinks followed), if it stays inside the
    /// working folder and is not a protected file; otherwise why not. Reads may
    /// also reach the shared dependency folders.
    fn resolve(&self, path: &str, write: bool) -> Result<PathBuf, String> {
        let outside = || {
            tr!(
                "Solo puedo trabajar dentro de la carpeta de la tarea.",
                "I can only work inside the task folder."
            )
        };
        let lexical = normalize(&self.cwd, Path::new(path));
        if !lexical.starts_with(&self.cwd) {
            return Err(outside());
        }
        // New files: resolve the closest folder that exists.
        let existing = lexical
            .ancestors()
            .find(|p| p.exists())
            .unwrap_or(&self.cwd);
        let real = existing
            .canonicalize()
            .map_err(|e| {
                tr!(
                    "No se pudo resolver {path}: {e}",
                    "Could not resolve {path}: {e}"
                )
            })?
            .join(lexical.strip_prefix(existing).unwrap_or(Path::new("")));
        let inside = real.starts_with(&self.cwd)
            || (!write && self.shared.iter().any(|dir| real.starts_with(dir)));
        if !inside {
            return Err(outside());
        }
        for target in [&lexical, &real] {
            if let Some(reason) = self.protected("Read", &json!({"file_path": target})) {
                return Err(reason);
            }
        }
        Ok(real)
    }

    fn relative(&self, path: &Path) -> String {
        relative(&self.cwd, path)
    }

    fn protected(&self, tool: &str, input: &Value) -> Option<String> {
        protected_file_reason(tool, input, &self.cwd)
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
            return Ok(tr!(
                "Ahora no puedes cambiar archivos ni correr comandos.",
                "You cannot change files or run commands now."
            ));
        }
        Ok(match name {
            "list_files" => self.list_files(&arg("path")),
            "read_file" => self.read_file(&arg("path")),
            "search" => self.search(&arg("pattern"), &arg("path")),
            "memory_search" | "memory_get" | "memory_topics" | "memory_propose" => {
                let Some(memory) = &self.memory else {
                    return Ok(tr!(
                        "Herramienta desconocida: {name}",
                        "Unknown tool: {name}"
                    ));
                };
                let topic = args["topic"].as_str();
                match name {
                    "memory_search" => memory.search(&arg("query")),
                    "memory_get" => memory.get(args["id"].as_i64(), topic),
                    "memory_topics" => memory.topics(),
                    _ => memory.propose(&arg("text"), &arg("category"), topic),
                }
            }
            "skill_list" | "skill_get" | "skill_propose" => {
                let Some(skills) = &self.skills else {
                    return Ok(tr!(
                        "Herramienta desconocida: {name}",
                        "Unknown tool: {name}"
                    ));
                };
                match name {
                    "skill_list" => skills.list(),
                    "skill_get" => skills.get(&arg("name")),
                    _ => skills.propose(&arg("name"), &arg("description"), &arg("body")),
                }
            }
            "write_file" | "edit_file" => {
                let target = match self.resolve(&arg("path"), true) {
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
                    (false, message) => {
                        let message = message.unwrap_or_default();
                        tr!("Rechazado: {message}", "Rejected: {message}")
                    }
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
                    (false, message) => {
                        let message = message.unwrap_or_default();
                        tr!("Rechazado: {message}", "Rejected: {message}")
                    }
                }
            }
            other => tr!("Herramienta desconocida: {other}", "Unknown tool: {other}"),
        })
    }

    fn read_file(&self, path: &str) -> String {
        let target = match self.resolve(path, false) {
            Ok(p) => p,
            Err(e) => return e,
        };
        if !target.is_file() {
            return format!("{path} no es un archivo.");
        }
        self.emit(AgentEvent::ToolUsed {
            tool: "Read".into(),
            summary: self.relative(&target),
        });
        let mut bytes = Vec::new();
        match std::fs::File::open(&target)
            .and_then(|file| file.take(MAX_READ as u64 + 1).read_to_end(&mut bytes))
        {
            Ok(_) => truncate(String::from_utf8_lossy(&bytes).into_owned(), MAX_READ),
            Err(e) => tr!("No se pudo leer: {e}", "Could not read: {e}"),
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
                // Not followed: symlinks may lead outside the folder.
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let is_dir = kind.is_dir();
                if kind.is_symlink() || (is_dir && SKIPPED_DIRS.contains(&name.as_str())) {
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
        let start = match self.resolve(if path.is_empty() { "." } else { path }, false) {
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
            tr!("(vacío)", "(empty)")
        } else {
            lines.join("\n")
        }
    }

    fn search(&self, pattern: &str, path: &str) -> String {
        let start = match self.resolve(if path.is_empty() { "." } else { path }, false) {
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
            if std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() < 1_000_000)
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
            tr!("Sin resultados.", "No results.")
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
                tr!("Hecho.", "Done.")
            }
            Err(e) => tr!(
                "No se pudo cambiar el archivo: {e}",
                "Could not change the file: {e}"
            ),
        }
    }

    /// Runs a shell command in its own process group, with stderr merged into
    /// stdout. Timeouts and stops kill the whole group, and processes left in
    /// the background never keep the agent waiting.
    fn run_command(&self, command_line: &str) -> String {
        // `exec 2>&1` first: one pipe, so a chatty stderr cannot fill up unread.
        let script = format!("exec 2>&1\n{command_line}");
        let spec = LaunchSpec::program("sh", vec!["-c".into(), script]).in_dir(self.cwd.clone());
        let mut cmd = command(&spec);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let child = cmd.spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => return tr!("No se pudo ejecutar: {e}", "Could not run it: {e}"),
        };
        let mut out = child.stdout.take().expect("stdout is piped");
        let output = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done) = mpsc::channel();
        {
            let output = Arc::clone(&output);
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n @ 1..) = out.read(&mut buf) {
                    output.lock().unwrap().extend_from_slice(&buf[..n]);
                }
                let _ = done_tx.send(());
            });
        }
        let kill_group = |child: &mut std::process::Child| {
            #[cfg(unix)]
            let _ = std::process::Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .status();
            let _ = child.kill();
            let _ = child.wait();
        };
        let started = Instant::now();
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None)
                    if started.elapsed() > COMMAND_TIMEOUT
                        || self.stopped.load(Ordering::SeqCst) =>
                {
                    kill_group(&mut child);
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => break None,
            }
        };
        // Background processes (`server &`) keep the pipe open: wait a moment
        // for the output, then end them.
        if done.recv_timeout(Duration::from_secs(2)).is_err() {
            kill_group(&mut child);
            let _ = done.recv_timeout(Duration::from_secs(1));
        }
        let bytes = std::mem::take(&mut *output.lock().unwrap());
        let output = keep_tail(&String::from_utf8_lossy(&bytes), MAX_OUTPUT);
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
        let code = code.map_or("?".into(), |c| c.to_string());
        tr!(
            "Código de salida: {code}\n{output}",
            "Exit code: {code}\n{output}"
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
            images: Vec::new(),
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
            memory: None,
            mcp: vec![],
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

    /// An agent on `dir` that is only used to call its tools.
    fn local_agent(dir: &Path) -> Agent {
        Agent {
            http: ureq::Agent::new_with_defaults(),
            url: String::new(),
            key: Secret::new("x"),
            model: String::new(),
            cwd: dir.canonicalize().unwrap(),
            shared: Vec::new(),
            events: Arc::new(|_| {}),
            read_only: false,
            phase: Phase::Implement,
            planning: false,
            plans: 0,
            messages: Vec::new(),
            queued: VecDeque::new(),
            stopped: Arc::new(AtomicBool::new(false)),
            memory: None,
            skills: None,
        }
    }

    /// Records what the agent asks the skills for.
    #[derive(Default)]
    struct SkillRecorder(Mutex<Vec<String>>);

    impl SkillTools for SkillRecorder {
        fn list(&self) -> String {
            self.0.lock().unwrap().push("list".into());
            "- run-tests [proyecto]: Antes de entregar".into()
        }
        fn get(&self, name: &str) -> String {
            self.0.lock().unwrap().push(format!("get {name}"));
            "# run-tests".into()
        }
        fn propose(&self, name: &str, description: &str, body: &str) -> String {
            self.0
                .lock()
                .unwrap()
                .push(format!("propose {name} | {description} | {body}"));
            "ok".into()
        }
    }

    #[test]
    fn skill_tools_reach_the_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let recorder = Arc::new(SkillRecorder::default());
        let mut agent = local_agent(tmp.path());
        let names = |agent: &Agent| -> Vec<String> {
            agent
                .tools()
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["function"]["name"].as_str().unwrap().to_owned())
                .collect()
        };
        assert!(!names(&agent).contains(&"skill_list".to_owned()));
        agent.skills = Some(recorder.clone());
        let names = names(&agent);
        for tool in ["skill_list", "skill_get", "skill_propose"] {
            assert!(names.contains(&tool.to_owned()), "{tool}");
        }
        let (_tx, rx) = mpsc::channel();
        let mut run = |name: &str, args: Value| {
            agent
                .run_tool(&call("s", name, args), &rx)
                .unwrap_or_else(|_| panic!("{name} ended the agent"))
        };
        assert!(run("skill_list", json!({})).contains("run-tests"));
        run("skill_get", json!({"name": "run-tests"}));
        run(
            "skill_propose",
            json!({"name": "lint", "description": "Antes de commit", "body": "npm run lint"}),
        );
        assert_eq!(
            *recorder.0.lock().unwrap(),
            [
                "list",
                "get run-tests",
                "propose lint | Antes de commit | npm run lint"
            ]
        );
    }

    #[test]
    fn symlinks_cannot_reach_secrets_or_leave_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join(".env"), "SECRETO=42").unwrap();
        std::fs::write(outside.path().join("private.txt"), "fuera").unwrap();
        std::os::unix::fs::symlink(dir.join(".env"), dir.join("notes.txt")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.join("link")).unwrap();
        std::os::unix::fs::symlink("/dev/zero", dir.join("zero")).unwrap();
        let agent = local_agent(dir);

        assert!(agent.read_file("notes.txt").contains("Hedy no permite"));
        assert!(
            agent
                .read_file("link/private.txt")
                .contains("dentro de la carpeta")
        );
        assert!(!agent.read_file("zero").contains('\0'));
        assert!(!agent.search("SECRETO", "").contains("42"));
        assert!(agent.resolve("link/new.txt", true).is_err());
        assert!(agent.resolve("nuevo/archivo.txt", true).is_ok());
    }

    #[test]
    fn commands_with_noisy_stderr_or_background_jobs_return() {
        let tmp = tempfile::tempdir().unwrap();
        let agent = local_agent(tmp.path());
        let started = Instant::now();
        let out = agent.run_command("head -c 200000 /dev/zero | tr '\\0' x 1>&2; echo listo");
        assert!(
            out.contains("listo") && out.contains("Código de salida: 0"),
            "{}",
            &out[..80]
        );
        let out = agent.run_command("sleep 30 & echo en-segundo-plano");
        assert!(out.contains("en-segundo-plano"));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
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

    #[test]
    fn images_go_as_image_url_parts() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().canonicalize().unwrap();
        let image = crate::adapters::images::tests::png(&dir);
        let (url, seen) = serve(vec![reply("Veo un gato.", json!(null))]);
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let request = AgentRequest {
            backend: AgentBackend::OpenAiCompatible {
                base_url: url,
                api_key: Secret::new("sk-test"),
            },
            launch: LaunchSpec::program("", vec![]).in_dir(dir.clone()),
            mode: TaskMode::Ask,
            phase: Phase::Implement,
            plan_first: false,
            model: Some("kimi-k3".into()),
            prompt: "¿Qué ves?".into(),
            images: vec![image],
            resume: None,
            memory: None,
            mcp: vec![],
        };
        let mut session = ApiAgentLauncher
            .start(request, Arc::new(move |e| drop(tx.lock().unwrap().send(e))))
            .unwrap();
        until(&rx, |e| matches!(e, AgentEvent::Finished { .. }));
        session.stop();
        let requests = seen.lock().unwrap();
        let user = &requests[0].1["messages"][1];
        assert_eq!(user["role"], "user");
        assert_eq!(
            user["content"][0],
            json!({"type": "text", "text": "¿Qué ves?"})
        );
        assert_eq!(user["content"][1]["type"], "image_url");
        assert!(
            user["content"][1]["image_url"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,iVBORw0KGgp4eA==")
        );
    }
}
