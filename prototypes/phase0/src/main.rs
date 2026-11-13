//! Phase 0 prototypes.
//!
//! They launch the official binaries (`claude`, `codex`, `gemini`) with the
//! config folder of a specific account and translate what they emit into
//! Hedy's common event language. Hedy only picks the folder: it never reads tokens.
//!
//! Throwaway code: the production version lives in `hedy-core` behind ports.

use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};

#[derive(Parser)]
#[command(name = "hedy-phase0", about = "Phase 0 validation prototypes")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// `claude auth status` for the account folder (never reads credentials).
    ClaudeStatus {
        #[arg(long)]
        config_dir: Option<PathBuf>,
    },
    /// Claude Code in headless mode (stream-json both ways).
    Claude {
        #[arg(long)]
        config_dir: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        #[arg(long, value_enum, default_value_t = Approvals::Ask)]
        approvals: Approvals,
        #[arg(long)]
        model: Option<String>,
        prompt: String,
    },
    /// Codex through `codex app-server` (JSON-RPC over stdio).
    Codex {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        #[arg(long, default_value = "codex")]
        binary: PathBuf,
        #[arg(long, value_enum, default_value_t = Approvals::Ask)]
        approvals: Approvals,
        /// Only `initialize` + `account/read` + `account/rateLimits/read`.
        #[arg(long)]
        account_only: bool,
        #[arg(default_value = "")]
        prompt: String,
    },
    /// Gemini CLI through ACP (`gemini --acp`, JSON-RPC over stdio).
    Gemini {
        /// Folder that will contain `.gemini/` (`GEMINI_CLI_HOME`).
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        #[arg(long, default_value = "gemini")]
        binary: PathBuf,
        #[arg(long, value_enum, default_value_t = Approvals::Ask)]
        approvals: Approvals,
        #[arg(default_value = "")]
        prompt: String,
    },
    /// Runs a binary inside a PTY and prints its screen for N seconds.
    Pty {
        #[arg(long)]
        config_dir: Option<PathBuf>,
        #[arg(long)]
        codex_home: Option<PathBuf>,
        #[arg(long)]
        gemini_home: Option<PathBuf>,
        #[arg(long, default_value_t = 6)]
        seconds: u64,
        #[arg(trailing_var_arg = true, required = true)]
        program: Vec<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Approvals {
    Ask,
    Allow,
    Deny,
}

/// Common event language (section 4 of the Hedy spec).
#[derive(Debug)]
enum Event {
    Message(String),
    Plan(String),
    PermissionRequest {
        id: String,
        what: String,
    },
    FileEdited(String),
    TestsRun(String),
    Finished(String),
    Error(String),
    Usage(String),
    /// Not translated yet; printed to study the format.
    Other(String),
}

impl Event {
    fn print(&self) {
        let (label, color, text) = match self {
            Event::Message(t) => ("Message", 36, t.clone()),
            Event::Plan(t) => ("Plan", 35, t.clone()),
            Event::PermissionRequest { id, what } => ("Permission", 33, format!("[{id}] {what}")),
            Event::FileEdited(t) => ("FileEdited", 34, t.clone()),
            Event::TestsRun(t) => ("TestsRun", 32, t.clone()),
            Event::Finished(t) => ("Finished", 32, t.clone()),
            Event::Error(t) => ("Error", 31, t.clone()),
            Event::Usage(t) => ("Usage", 33, t.clone()),
            Event::Other(t) => ("·", 90, t.clone()),
        };
        println!("\x1b[{color}m{label:>12}\x1b[0m {}", truncate(&text, 220));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Cmd::ClaudeStatus { config_dir } => claude_status(config_dir.as_deref()).await,
        Cmd::Claude {
            config_dir,
            cwd,
            approvals,
            model,
            prompt,
        } => claude_headless(config_dir.as_deref(), &cwd, approvals, model, &prompt).await,
        Cmd::Codex {
            home,
            cwd,
            binary,
            approvals,
            account_only,
            prompt,
        } => {
            codex_app_server(
                &binary,
                home.as_deref(),
                &cwd,
                approvals,
                account_only,
                &prompt,
            )
            .await
        }
        Cmd::Gemini {
            home,
            cwd,
            binary,
            approvals,
            prompt,
        } => gemini_acp(&binary, home.as_deref(), &cwd, approvals, &prompt).await,
        Cmd::Pty {
            config_dir,
            codex_home,
            gemini_home,
            seconds,
            program,
        } => {
            let folders = [
                ("CLAUDE_CONFIG_DIR", config_dir),
                ("CODEX_HOME", codex_home),
                ("GEMINI_CLI_HOME", gemini_home),
            ];
            pty(&folders, seconds, &program)
        }
    }
}

// ---------------------------------------------------------------- environment

/// Variables that child processes must not inherit.
///
/// - Leftovers from a Claude Code session that launched us (`CLAUDECODE`, `CLAUDE_CODE_*`).
/// - API keys and tokens: when `ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`,
///   `OPENAI_API_KEY`, `GEMINI_API_KEY`... are present, the CLI uses them instead
///   of the login stored in the account folder.
/// - `GEMINI_FORCE_ENCRYPTED_FILE_STORAGE`: stores the Gemini login in the keyring
///   under one fixed key for every folder, which would mix accounts.
fn clean_env() -> HashMap<String, String> {
    const BLOCKED: &[&str] = &[
        "CLAUDECODE",
        "CLAUDE_PID",
        "CLAUDE_EFFORT",
        "CLAUDE_CONFIG_DIR",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CODEX_HOME",
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "GEMINI_CLI_HOME",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "GOOGLE_GENAI_USE_VERTEXAI",
        "GEMINI_FORCE_ENCRYPTED_FILE_STORAGE",
        "AI_AGENT",
    ];
    std::env::vars()
        .filter(|(k, _)| !k.starts_with("CLAUDE_CODE_") && !BLOCKED.contains(&k.as_str()))
        .collect()
}

fn claude_command(config_dir: Option<&Path>) -> Command {
    let mut cmd = Command::new("claude");
    cmd.env_clear().envs(clean_env());
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    cmd
}

// ---------------------------------------------------------------- claude

async fn claude_status(config_dir: Option<&Path>) -> Result<()> {
    let output = claude_command(config_dir)
        .args(["auth", "status", "--json"])
        .output()
        .await
        .context("could not launch `claude`")?;
    let status: Value = serde_json::from_slice(&output.stdout)?;
    // Non-identifying fields only; email and organization are not printed.
    println!(
        "logged_in={} method={} subscription={} folder={} (exit code {})",
        status["loggedIn"],
        status["authMethod"],
        status["subscriptionType"],
        status["configDirectory"],
        output.status.code().unwrap_or(-1)
    );
    Ok(())
}

async fn claude_headless(
    config_dir: Option<&Path>,
    cwd: &Path,
    approvals: Approvals,
    model: Option<String>,
    prompt: &str,
) -> Result<()> {
    let mut cmd = claude_command(config_dir);
    cmd.current_dir(cwd).args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        // Permission requests arrive as `control_request` on stdout.
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
        "manual",
    ]);
    if let Some(m) = model {
        cmd.args(["--model", &m]);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("could not launch `claude`")?;
    let mut stdin = child.stdin.take().expect("stdin");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();

    send(
        &mut stdin,
        &json!({"type": "control_request", "request_id": "init-1",
                "request": {"subtype": "initialize"}}),
    )
    .await?;
    send(
        &mut stdin,
        &json!({"type": "user", "session_id": "", "parent_tool_use_id": null,
                "message": {"role": "user", "content": prompt}}),
    )
    .await?;

    let start = Instant::now();
    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            Event::Other(format!("non-JSON line: {line}")).print();
            continue;
        };
        save_sample("claude", &msg);
        for event in translate_claude(&msg) {
            event.print();
            if let Event::PermissionRequest { id, .. } = &event {
                let input = msg["request"]["input"].clone();
                let response = match decide(approvals).await? {
                    Decision::Allow | Decision::AllowForSession => {
                        json!({"behavior": "allow", "updatedInput": input})
                    }
                    Decision::Deny => json!({"behavior": "deny", "message": "Denied in Hedy"}),
                };
                send(
                    &mut stdin,
                    &json!({"type": "control_response", "response": {
                        "subtype": "success", "request_id": id, "response": response}}),
                )
                .await?;
            }
        }
        if msg["type"] == "result" {
            // Single turn: closing stdin ends the process cleanly.
            drop(stdin);
            break;
        }
    }
    let status = child.wait().await?;
    println!("claude exited with {status} after {:.1?}", start.elapsed());
    Ok(())
}

fn translate_claude(msg: &Value) -> Vec<Event> {
    let kind = msg["type"].as_str().unwrap_or("");
    let subtype = msg["subtype"].as_str().unwrap_or("");
    match kind {
        "system" if subtype == "init" => vec![Event::Other(format!(
            "session {} · model {} · permissions {} · {} tools · mcp {}",
            msg["session_id"],
            msg["model"],
            msg["permissionMode"],
            msg["tools"].as_array().map_or(0, Vec::len),
            msg["mcp_servers"]
        ))],
        "assistant" => msg["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| match block["type"].as_str()? {
                "text" => Some(Event::Message(block["text"].as_str()?.to_owned())),
                "tool_use" => Some(translate_claude_tool(block)),
                _ => None,
            })
            .collect(),
        "control_request" if msg["request"]["subtype"] == "can_use_tool" => {
            vec![Event::PermissionRequest {
                id: msg["request_id"].as_str().unwrap_or("").to_owned(),
                what: format!(
                    "{} {}",
                    msg["request"]["tool_name"], msg["request"]["input"]
                ),
            }]
        }
        "result" => {
            let usage = Event::Usage(format!(
                "cost_usd={} usage={} turns={} duration_ms={}",
                msg["total_cost_usd"], msg["usage"], msg["num_turns"], msg["duration_ms"]
            ));
            let end = if msg["is_error"].as_bool().unwrap_or(false) {
                Event::Error(format!("{subtype}: {}", msg["result"]))
            } else {
                Event::Finished(format!("{subtype}: {}", msg["result"]))
            };
            vec![usage, end]
        }
        "rate_limit_event" => vec![Event::Usage(format!("limit: {msg}"))],
        "user" => vec![], // tool results; not shown
        _ => vec![Event::Other(format!("{kind}/{subtype}: {msg}"))],
    }
}

fn translate_claude_tool(block: &Value) -> Event {
    let name = block["name"].as_str().unwrap_or("");
    let input = &block["input"];
    match name {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            Event::FileEdited(input["file_path"].as_str().unwrap_or("?").to_owned())
        }
        "TodoWrite" | "ExitPlanMode" => Event::Plan(input.to_string()),
        "Bash" => {
            let command = input["command"].as_str().unwrap_or("").to_owned();
            if looks_like_tests(&command) {
                Event::TestsRun(command)
            } else {
                Event::Other(format!("Bash: {command}"))
            }
        }
        _ => Event::Other(format!("tool {name}: {input}")),
    }
}

// ---------------------------------------------------------------- codex

async fn codex_app_server(
    binary: &Path,
    home: Option<&Path>,
    cwd: &Path,
    approvals: Approvals,
    account_only: bool,
    prompt: &str,
) -> Result<()> {
    let mut cmd = Command::new(binary);
    cmd.env_clear().envs(clean_env());
    if let Some(h) = home {
        cmd.env("CODEX_HOME", h);
    }
    let mut child = cmd
        .args(["app-server", "--listen", "stdio://"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("could not launch {}", binary.display()))?;
    let mut stdin = child.stdin.take().expect("stdin");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let mut rpc = Rpc::default();

    rpc.request(
        &mut stdin,
        "initialize",
        json!({"clientInfo": {"name": "hedy", "title": "Hedy", "version": "0.0.0"}}),
    )
    .await?;
    rpc.notify(&mut stdin, "initialized").await?;
    let account_id = rpc
        .request(&mut stdin, "account/read", json!({"refreshToken": false}))
        .await?;
    let limits_id = rpc
        .request(&mut stdin, "account/rateLimits/read", json!({}))
        .await?;
    let mut thread_id = None;
    if !account_only {
        thread_id = Some(
            rpc.request(
                &mut stdin,
                "thread/start",
                json!({"cwd": cwd.canonicalize()?, "approvalPolicy": "untrusted",
                       "sandbox": "workspace-write", "ephemeral": true}),
            )
            .await?,
        );
    }

    // In `account_only` mode we stop once both account responses arrived.
    let mut account_responses = 0;
    while let Some(line) = lines.next_line().await? {
        let msg: Value = serde_json::from_str(&line)?;
        save_sample("codex", &msg);
        // Responses to our requests.
        if let Some(id) = msg["id"].as_u64().filter(|_| msg.get("method").is_none()) {
            if id == account_id || id == limits_id {
                account_responses += 1;
            }
            let account_done = account_only && account_responses == 2;
            if let Some(error) = msg.get("error") {
                Event::Error(format!("rpc {id}: {error}")).print();
                if Some(id) == thread_id || account_done {
                    break;
                }
                continue;
            }
            let r = &msg["result"];
            if id == account_id {
                // Account type and plan only; no email, no tokens.
                Event::Other(format!(
                    "account type={} plan={} requires_login={}",
                    r["account"]["type"], r["account"]["planType"], r["requiresOpenaiAuth"]
                ))
                .print();
            } else if id == limits_id {
                Event::Usage(format!("limits: {}", r["rateLimits"])).print();
            } else if Some(id) == thread_id {
                let thread = r["thread"]["id"].clone();
                Event::Other(format!("thread {thread} · model {}", r["model"])).print();
                rpc.request(
                    &mut stdin,
                    "turn/start",
                    json!({"threadId": thread, "input": [{"type": "text", "text": prompt}]}),
                )
                .await?;
            }
            if account_done {
                break;
            }
            continue;
        }
        let method = msg["method"].as_str().unwrap_or("");
        let p = &msg["params"];
        // Server-initiated requests: approvals.
        if let Some(id) = msg.get("id").cloned() {
            let what = match method {
                "item/commandExecution/requestApproval" => format!("command {}", p["command"]),
                "item/fileChange/requestApproval" => format!("file change {}", p["reason"]),
                other => format!("{other}: {p}"),
            };
            Event::PermissionRequest {
                id: id.to_string(),
                what,
            }
            .print();
            let decision = match decide(approvals).await? {
                Decision::Allow => "accept",
                Decision::AllowForSession => "acceptForSession",
                Decision::Deny => "decline",
            };
            send(
                &mut stdin,
                &json!({"jsonrpc": "2.0", "id": id, "result": {"decision": decision}}),
            )
            .await?;
            continue;
        }
        let event = match method {
            "item/completed" => translate_codex_item(&p["item"]),
            "turn/plan/updated" => Some(Event::Plan(p["plan"].to_string())),
            "thread/tokenUsage/updated" => {
                Some(Event::Usage(format!("tokens {}", p["tokenUsage"]["total"])))
            }
            "account/rateLimits/updated" => {
                Some(Event::Usage(format!("limits {}", p["rateLimits"])))
            }
            "error" => Some(Event::Error(p.to_string())),
            "turn/completed" => {
                Event::Finished(format!("turn {}", p["turn"]["status"])).print();
                break;
            }
            m if m.ends_with("/delta") || m.ends_with("Delta") => None,
            _ => Some(Event::Other(format!("{method}: {p}"))),
        };
        if let Some(e) = event {
            e.print();
        }
    }
    drop(stdin);
    child.kill().await.ok();
    Ok(())
}

fn translate_codex_item(item: &Value) -> Option<Event> {
    match item["type"].as_str()? {
        "agentMessage" => Some(Event::Message(item["text"].as_str()?.to_owned())),
        "plan" => Some(Event::Plan(item["text"].as_str()?.to_owned())),
        "fileChange" => Some(Event::FileEdited(
            item["changes"]
                .as_array()?
                .iter()
                .filter_map(|c| c["path"].as_str())
                .collect::<Vec<_>>()
                .join(", "),
        )),
        "commandExecution" => {
            let command = item["command"].as_str().unwrap_or("").to_owned();
            Some(if looks_like_tests(&command) {
                Event::TestsRun(format!("{command} → {}", item["exitCode"]))
            } else {
                Event::Other(format!("command {command} → {}", item["exitCode"]))
            })
        }
        "userMessage" | "reasoning" => None,
        other => Some(Event::Other(format!("item {other}"))),
    }
}

#[derive(Default)]
struct Rpc {
    next_id: u64,
}

impl Rpc {
    async fn request(
        &mut self,
        stdin: &mut ChildStdin,
        method: &str,
        params: Value,
    ) -> Result<u64> {
        self.next_id += 1;
        let id = self.next_id;
        send(
            stdin,
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )
        .await?;
        Ok(id)
    }

    async fn notify(&mut self, stdin: &mut ChildStdin, method: &str) -> Result<()> {
        send(stdin, &json!({"jsonrpc": "2.0", "method": method})).await
    }
}

// ---------------------------------------------------------------- gemini

async fn gemini_acp(
    binary: &Path,
    home: Option<&Path>,
    cwd: &Path,
    approvals: Approvals,
    prompt: &str,
) -> Result<()> {
    let mut cmd = Command::new(binary);
    cmd.env_clear().envs(clean_env());
    if let Some(h) = home {
        cmd.env("GEMINI_CLI_HOME", h);
    }
    let mut child = cmd
        .arg("--acp")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("could not launch {}", binary.display()))?;
    let mut stdin = child.stdin.take().expect("stdin");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
    let mut rpc = Rpc::default();

    let init_id = rpc
        .request(
            &mut stdin,
            "initialize",
            json!({"protocolVersion": 1,
                   "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}},
                   "clientInfo": {"name": "hedy", "version": "0.0.0"}}),
        )
        .await?;
    let mut session_id = None;
    let mut prompt_id = None;

    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        save_sample("gemini", &msg);
        let method = msg["method"].as_str().unwrap_or("");
        // Responses to our requests.
        if let Some(id) = msg["id"].as_u64().filter(|_| method.is_empty()) {
            if let Some(error) = msg.get("error") {
                Event::Error(format!("rpc {id}: {error}")).print();
                break;
            }
            let r = &msg["result"];
            if id == init_id {
                Event::Other(format!(
                    "agent {} · protocol {} · auth methods {}",
                    r["agentInfo"], r["protocolVersion"], r["authMethods"]
                ))
                .print();
                if prompt.is_empty() {
                    break;
                }
                session_id = Some(
                    rpc.request(
                        &mut stdin,
                        "session/new",
                        json!({"cwd": cwd.canonicalize()?, "mcpServers": []}),
                    )
                    .await?,
                );
            } else if Some(id) == session_id {
                Event::Other(format!("session {} · modes {}", r["sessionId"], r["modes"])).print();
                prompt_id = Some(
                    rpc.request(
                        &mut stdin,
                        "session/prompt",
                        json!({"sessionId": r["sessionId"],
                               "prompt": [{"type": "text", "text": prompt}]}),
                    )
                    .await?,
                );
            } else if Some(id) == prompt_id {
                Event::Finished(format!("stopReason {}", r["stopReason"])).print();
                break;
            }
            continue;
        }
        let p = &msg["params"];
        if method == "session/request_permission" {
            let id = msg["id"].clone();
            Event::PermissionRequest {
                id: id.to_string(),
                what: format!("{} {}", p["toolCall"]["title"], p["toolCall"]["kind"]),
            }
            .print();
            // The agent supplies the options; we pick one by its `kind`.
            let kind = match decide(approvals).await? {
                Decision::Allow => "allow_once",
                Decision::AllowForSession => "allow_always",
                Decision::Deny => "reject_once",
            };
            let option = p["options"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["kind"] == kind)
                .map(|o| o["optionId"].clone());
            let outcome = match option {
                Some(option_id) => {
                    json!({"outcome": {"outcome": "selected", "optionId": option_id}})
                }
                None => json!({"outcome": {"outcome": "cancelled"}}),
            };
            send(
                &mut stdin,
                &json!({"jsonrpc": "2.0", "id": id, "result": outcome}),
            )
            .await?;
            continue;
        }
        if method == "session/update" {
            if let Some(e) = translate_gemini(&p["update"]) {
                e.print();
            }
            continue;
        }
        Event::Other(format!("{method}: {p}")).print();
    }
    drop(stdin);
    child.kill().await.ok();
    Ok(())
}

fn translate_gemini(update: &Value) -> Option<Event> {
    match update["sessionUpdate"].as_str()? {
        // Message chunks arrive one by one; printed as they come.
        "agent_message_chunk" => Some(Event::Message(
            update["content"]["text"].as_str()?.to_owned(),
        )),
        "agent_thought_chunk" | "user_message_chunk" | "tool_call_update" => None,
        "plan" => Some(Event::Plan(update["entries"].to_string())),
        "tool_call" => {
            let title = update["title"].as_str().unwrap_or("").to_owned();
            Some(match update["kind"].as_str() {
                Some("edit") => Event::FileEdited(title),
                Some("execute") if looks_like_tests(&title) => Event::TestsRun(title),
                _ => Event::Other(format!("tool {} {title}", update["kind"])),
            })
        }
        other => Some(Event::Other(format!("{other}: {update}"))),
    }
}

// ---------------------------------------------------------------- pty

fn pty(folders: &[(&str, Option<PathBuf>)], seconds: u64, program: &[String]) -> Result<()> {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};

    let system = native_pty_system();
    let pair = system.openpty(PtySize {
        rows: 30,
        cols: 100,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(&program[0]);
    cmd.args(&program[1..]);
    // portable-pty starts in $HOME unless told otherwise.
    cmd.cwd(std::env::current_dir()?);
    cmd.env_clear();
    for (k, v) in clean_env() {
        cmd.env(k, v);
    }
    cmd.env("TERM", "xterm-256color");
    for (var, folder) in folders {
        if let Some(dir) = folder {
            cmd.env(var, dir);
        }
    }
    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let mut screen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(200)) {
            screen.extend(chunk);
        }
        if child.try_wait()?.is_some() {
            break;
        }
    }
    child.kill().ok();
    let text = strip_ansi(&String::from_utf8_lossy(&screen));
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect();
    println!(
        "{} bytes received from the PTY; visible text:",
        screen.len()
    );
    for l in lines.iter().take(40) {
        println!("  │ {l}");
    }
    Ok(())
}

// ---------------------------------------------------------------- helpers

enum Decision {
    Allow,
    AllowForSession,
    Deny,
}

async fn decide(approvals: Approvals) -> Result<Decision> {
    Ok(match approvals {
        Approvals::Allow => Decision::Allow,
        Approvals::Deny => Decision::Deny,
        Approvals::Ask => {
            print!("    [a]llow, allow for this [s]ession or [d]eny? ");
            std::io::stdout().flush()?;
            let mut line = String::new();
            BufReader::new(tokio::io::stdin())
                .read_line(&mut line)
                .await?;
            match line.trim() {
                "a" => Decision::Allow,
                "s" => Decision::AllowForSession,
                "d" | "" => Decision::Deny,
                other => bail!("invalid answer: {other}"),
            }
        }
    })
}

async fn send(stdin: &mut ChildStdin, value: &Value) -> Result<()> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    stdin.write_all(&line).await?;
    stdin.flush().await?;
    Ok(())
}

/// Appends every raw message to `HEDY_SAMPLES/<cli>.jsonl` to document formats.
fn save_sample(cli: &str, msg: &Value) {
    let Some(dir) = std::env::var_os("HEDY_SAMPLES") else {
        return;
    };
    let path = Path::new(&dir).join(format!("{cli}.jsonl"));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn looks_like_tests(command: &str) -> bool {
    [
        "cargo test",
        "pytest",
        "npm test",
        "pnpm test",
        "bun test",
        "vitest",
        "jest",
        "go test",
    ]
    .iter()
    .any(|t| command.contains(t))
}

fn truncate(text: &str, max: usize) -> String {
    let one_line = text.replace('\n', " ⏎ ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        one_line.chars().take(max).collect::<String>() + "…"
    }
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            if c != '\r' {
                out.push(c);
            }
            continue;
        }
        match chars.next() {
            // CSI: ends with a byte between @ and ~.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: ends with BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.peek() == Some(&'\\')) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}
