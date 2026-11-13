//! Prototipos de la fase 0.
//!
//! Lanzan los binarios oficiales (`claude`, `codex`) con la carpeta de
//! configuración de una cuenta concreta y traducen lo que emiten al idioma
//! común de eventos de Hedy. Hedy solo elige la carpeta: nunca lee tokens.

use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

#[derive(Parser)]
#[command(name = "hedy-fase0", about = "Prototipos de validación de la fase 0")]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// `claude auth status` con la carpeta de la cuenta (sin leer credenciales).
    ClaudeEstado {
        #[arg(long)]
        config_dir: Option<PathBuf>,
    },
    /// Claude Code en modo headless (stream-json en ambos sentidos).
    Claude {
        #[arg(long)]
        config_dir: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        #[arg(long, value_enum, default_value_t = Permisos::Preguntar)]
        permisos: Permisos,
        #[arg(long)]
        modelo: Option<String>,
        pedido: String,
    },
    /// Codex vía `codex app-server` (JSON-RPC por stdio).
    Codex {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        #[arg(long, default_value = "codex")]
        binario: PathBuf,
        #[arg(long, value_enum, default_value_t = Permisos::Preguntar)]
        permisos: Permisos,
        /// Solo `initialize` + `account/read` + `account/rateLimits/read`.
        #[arg(long)]
        solo_cuenta: bool,
        #[arg(default_value = "")]
        pedido: String,
    },
    /// Gemini CLI vía ACP (`gemini --acp`, JSON-RPC por stdio).
    Gemini {
        /// Carpeta que contendrá `.gemini/` (`GEMINI_CLI_HOME`).
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        #[arg(long, default_value = "gemini")]
        binario: PathBuf,
        #[arg(long, value_enum, default_value_t = Permisos::Preguntar)]
        permisos: Permisos,
        #[arg(default_value = "")]
        pedido: String,
    },
    /// Lanza un binario dentro de una PTY y muestra su pantalla durante N segundos.
    Pty {
        #[arg(long)]
        config_dir: Option<PathBuf>,
        #[arg(long)]
        codex_home: Option<PathBuf>,
        #[arg(long)]
        gemini_home: Option<PathBuf>,
        #[arg(long, default_value_t = 6)]
        segundos: u64,
        #[arg(trailing_var_arg = true, required = true)]
        programa: Vec<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Permisos {
    Preguntar,
    Permitir,
    Rechazar,
}

/// Idioma común de eventos (sección 4 del documento de Hedy).
#[derive(Debug)]
enum Evento {
    Mensaje(String),
    Plan(String),
    PideComando {
        id: String,
        que: String,
    },
    EditoArchivo(String),
    CorrioTests(String),
    Termino(String),
    Error(String),
    Consumo(String),
    /// Lo que todavía no se traduce; se imprime para estudiar el formato.
    Otro(String),
}

impl Evento {
    fn mostrar(&self) {
        let (etiqueta, color, texto) = match self {
            Evento::Mensaje(t) => ("Mensaje", 36, t.clone()),
            Evento::Plan(t) => ("Plan", 35, t.clone()),
            Evento::PideComando { id, que } => ("PideComando", 33, format!("[{id}] {que}")),
            Evento::EditoArchivo(t) => ("EditoArchivo", 34, t.clone()),
            Evento::CorrioTests(t) => ("CorrioTests", 32, t.clone()),
            Evento::Termino(t) => ("Termino", 32, t.clone()),
            Evento::Error(t) => ("Error", 31, t.clone()),
            Evento::Consumo(t) => ("Consumo", 33, t.clone()),
            Evento::Otro(t) => ("·", 90, t.clone()),
        };
        println!(
            "\x1b[{color}m{etiqueta:>12}\x1b[0m {}",
            recortar(&texto, 220)
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().comando {
        Comando::ClaudeEstado { config_dir } => claude_estado(config_dir.as_deref()).await,
        Comando::Claude {
            config_dir,
            cwd,
            permisos,
            modelo,
            pedido,
        } => claude_headless(config_dir.as_deref(), &cwd, permisos, modelo, &pedido).await,
        Comando::Codex {
            home,
            cwd,
            binario,
            permisos,
            solo_cuenta,
            pedido,
        } => {
            codex_app_server(
                &binario,
                home.as_deref(),
                &cwd,
                permisos,
                solo_cuenta,
                &pedido,
            )
            .await
        }
        Comando::Gemini {
            home,
            cwd,
            binario,
            permisos,
            pedido,
        } => gemini_acp(&binario, home.as_deref(), &cwd, permisos, &pedido).await,
        Comando::Pty {
            config_dir,
            codex_home,
            gemini_home,
            segundos,
            programa,
        } => {
            let carpetas = [
                ("CLAUDE_CONFIG_DIR", config_dir),
                ("CODEX_HOME", codex_home),
                ("GEMINI_CLI_HOME", gemini_home),
            ];
            pty(&carpetas, segundos, &programa)
        }
    }
}

// ---------------------------------------------------------------- entorno

/// Variables que no deben heredar los procesos hijos.
///
/// - Las de una sesión de Claude Code que nos lanzó (`CLAUDECODE`, `CLAUDE_CODE_*`).
/// - Las API keys y tokens: si `ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`,
///   `OPENAI_API_KEY`, `GEMINI_API_KEY`... están presentes, la CLI usa esa
///   credencial en vez del login de la carpeta de la cuenta.
/// - `GEMINI_FORCE_ENCRYPTED_FILE_STORAGE`: guarda el login de Gemini en el
///   llavero con una clave única para todas las carpetas, lo que mezclaría cuentas.
fn entorno_limpio() -> HashMap<String, String> {
    std::env::vars()
        .filter(|(k, _)| {
            !(k == "CLAUDECODE"
                || k.starts_with("CLAUDE_CODE_")
                || k == "CLAUDE_PID"
                || k == "CLAUDE_EFFORT"
                || k == "CLAUDE_CONFIG_DIR"
                || k == "CODEX_HOME"
                || k == "ANTHROPIC_API_KEY"
                || k == "ANTHROPIC_AUTH_TOKEN"
                || k == "OPENAI_API_KEY"
                || k == "CODEX_API_KEY"
                || k == "CODEX_ACCESS_TOKEN"
                || k == "GEMINI_CLI_HOME"
                || k == "GEMINI_API_KEY"
                || k == "GOOGLE_API_KEY"
                || k == "GOOGLE_GENAI_USE_VERTEXAI"
                || k == "GEMINI_FORCE_ENCRYPTED_FILE_STORAGE"
                || k == "AI_AGENT")
        })
        .collect()
}

fn comando_claude(config_dir: Option<&Path>) -> Command {
    let mut cmd = Command::new("claude");
    cmd.env_clear().envs(entorno_limpio());
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    cmd
}

// ---------------------------------------------------------------- claude

async fn claude_estado(config_dir: Option<&Path>) -> Result<()> {
    let salida = comando_claude(config_dir)
        .args(["auth", "status", "--json"])
        .output()
        .await
        .context("no se pudo lanzar `claude`")?;
    let estado: Value = serde_json::from_slice(&salida.stdout)?;
    // Solo campos no sensibles; el correo y la organización no se imprimen.
    println!(
        "logueado={} método={} suscripción={} carpeta={} (código de salida {})",
        estado["loggedIn"],
        estado["authMethod"],
        estado["subscriptionType"],
        estado["configDirectory"],
        salida.status.code().unwrap_or(-1)
    );
    Ok(())
}

async fn claude_headless(
    config_dir: Option<&Path>,
    cwd: &Path,
    permisos: Permisos,
    modelo: Option<String>,
    pedido: &str,
) -> Result<()> {
    let mut cmd = comando_claude(config_dir);
    cmd.current_dir(cwd).args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        // Las peticiones de permiso llegan como `control_request` por stdout.
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
        "manual",
    ]);
    if let Some(m) = modelo {
        cmd.args(["--model", &m]);
    }
    let mut hijo = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("no se pudo lanzar `claude`")?;
    let mut entrada = hijo.stdin.take().expect("stdin");
    let mut lineas = BufReader::new(hijo.stdout.take().expect("stdout")).lines();

    enviar(
        &mut entrada,
        &json!({"type": "control_request", "request_id": "init-1",
                "request": {"subtype": "initialize"}}),
    )
    .await?;
    enviar(
        &mut entrada,
        &json!({"type": "user", "session_id": "", "parent_tool_use_id": null,
                "message": {"role": "user", "content": pedido}}),
    )
    .await?;

    let inicio = Instant::now();
    while let Some(linea) = lineas.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&linea) else {
            Evento::Otro(format!("línea no JSON: {linea}")).mostrar();
            continue;
        };
        guardar_muestra("claude", &msg);
        for evento in traducir_claude(&msg) {
            evento.mostrar();
            if let Evento::PideComando { id, .. } = &evento {
                let decision = decidir(permisos).await?;
                let input = msg["request"]["input"].clone();
                let respuesta = match decision {
                    Decision::Permitir | Decision::PermitirSesion => {
                        json!({"behavior": "allow", "updatedInput": input})
                    }
                    Decision::Rechazar => {
                        json!({"behavior": "deny", "message": "Rechazado desde Hedy"})
                    }
                };
                enviar(
                    &mut entrada,
                    &json!({"type": "control_response", "response": {
                        "subtype": "success", "request_id": id, "response": respuesta}}),
                )
                .await?;
            }
        }
        if msg["type"] == "result" {
            // Una sola vuelta: cerrar stdin termina el proceso limpiamente.
            drop(entrada);
            break;
        }
    }
    let estado = hijo.wait().await?;
    println!("claude terminó con {estado} en {:.1?}", inicio.elapsed());
    Ok(())
}

fn traducir_claude(msg: &Value) -> Vec<Evento> {
    let tipo = msg["type"].as_str().unwrap_or("");
    let subtipo = msg["subtype"].as_str().unwrap_or("");
    match tipo {
        "system" if subtipo == "init" => vec![Evento::Otro(format!(
            "sesión {} · modelo {} · permisos {} · {} herramientas · mcp {}",
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
            .filter_map(|bloque| match bloque["type"].as_str()? {
                "text" => Some(Evento::Mensaje(bloque["text"].as_str()?.to_owned())),
                "tool_use" => Some(traducir_herramienta_claude(bloque)),
                _ => None,
            })
            .collect(),
        "control_request" if msg["request"]["subtype"] == "can_use_tool" => {
            vec![Evento::PideComando {
                id: msg["request_id"].as_str().unwrap_or("").to_owned(),
                que: format!(
                    "{} {}",
                    msg["request"]["tool_name"], msg["request"]["input"]
                ),
            }]
        }
        "result" => {
            let mut eventos = vec![Evento::Consumo(format!(
                "costo_usd={} uso={} turnos={} duración_ms={}",
                msg["total_cost_usd"], msg["usage"], msg["num_turns"], msg["duration_ms"]
            ))];
            eventos.push(if msg["is_error"].as_bool().unwrap_or(false) {
                Evento::Error(format!("{subtipo}: {}", msg["result"]))
            } else {
                Evento::Termino(format!("{subtipo}: {}", msg["result"]))
            });
            eventos
        }
        "rate_limit_event" => vec![Evento::Consumo(format!("límite: {}", msg))],
        "user" => vec![], // resultados de herramientas; no se muestran
        _ => vec![Evento::Otro(format!("{tipo}/{subtipo}: {msg}"))],
    }
}

fn traducir_herramienta_claude(bloque: &Value) -> Evento {
    let nombre = bloque["name"].as_str().unwrap_or("");
    let input = &bloque["input"];
    match nombre {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            Evento::EditoArchivo(input["file_path"].as_str().unwrap_or("?").to_owned())
        }
        "TodoWrite" | "ExitPlanMode" => Evento::Plan(input.to_string()),
        "Bash" => {
            let comando = input["command"].as_str().unwrap_or("").to_owned();
            if parece_test(&comando) {
                Evento::CorrioTests(comando)
            } else {
                Evento::Otro(format!("Bash: {comando}"))
            }
        }
        _ => Evento::Otro(format!("herramienta {nombre}: {input}")),
    }
}

// ---------------------------------------------------------------- codex

async fn codex_app_server(
    binario: &Path,
    home: Option<&Path>,
    cwd: &Path,
    permisos: Permisos,
    solo_cuenta: bool,
    pedido: &str,
) -> Result<()> {
    let mut cmd = Command::new(binario);
    cmd.env_clear().envs(entorno_limpio());
    if let Some(h) = home {
        cmd.env("CODEX_HOME", h);
    }
    let mut hijo: Child = cmd
        .args(["app-server", "--listen", "stdio://"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("no se pudo lanzar {}", binario.display()))?;
    let mut entrada = hijo.stdin.take().expect("stdin");
    let mut lineas = BufReader::new(hijo.stdout.take().expect("stdout")).lines();
    let mut rpc = Rpc::default();

    rpc.pedir(
        &mut entrada,
        "initialize",
        json!({"clientInfo": {"name": "hedy", "title": "Hedy", "version": "0.0.0"}}),
    )
    .await?;
    rpc.notificar(&mut entrada, "initialized").await?;
    let id_cuenta = rpc
        .pedir(&mut entrada, "account/read", json!({"refreshToken": false}))
        .await?;
    let id_limites = rpc
        .pedir(&mut entrada, "account/rateLimits/read", json!({}))
        .await?;
    let mut id_hilo = None;
    if !solo_cuenta {
        id_hilo = Some(
            rpc.pedir(
                &mut entrada,
                "thread/start",
                json!({"cwd": cwd.canonicalize()?, "approvalPolicy": "untrusted",
                       "sandbox": "workspace-write", "ephemeral": true}),
            )
            .await?,
        );
    }

    // En modo `solo_cuenta` se termina al recibir las dos respuestas de cuenta.
    let mut respuestas_cuenta = 0;
    while let Some(linea) = lineas.next_line().await? {
        let msg: Value = serde_json::from_str(&linea)?;
        guardar_muestra("codex", &msg);
        // Respuestas a nuestras peticiones.
        if let Some(id) = msg["id"].as_u64().filter(|_| msg.get("method").is_none()) {
            if id == id_cuenta || id == id_limites {
                respuestas_cuenta += 1;
            }
            let fin_cuenta = solo_cuenta && respuestas_cuenta == 2;
            if let Some(error) = msg.get("error") {
                Evento::Error(format!("rpc {id}: {error}")).mostrar();
                if Some(id) == id_hilo || fin_cuenta {
                    break;
                }
                continue;
            }
            let r = &msg["result"];
            if id == id_cuenta {
                // Solo tipo de cuenta y plan; nada de correo ni tokens.
                Evento::Otro(format!(
                    "cuenta tipo={} plan={} requiere_login={}",
                    r["account"]["type"], r["account"]["planType"], r["requiresOpenaiAuth"]
                ))
                .mostrar();
            } else if id == id_limites {
                Evento::Consumo(format!("límites: {}", r["rateLimits"])).mostrar();
            } else if Some(id) == id_hilo {
                let hilo = r["thread"]["id"].clone();
                Evento::Otro(format!("hilo {hilo} · modelo {}", r["model"])).mostrar();
                rpc.pedir(
                    &mut entrada,
                    "turn/start",
                    json!({"threadId": hilo, "input": [{"type": "text", "text": pedido}]}),
                )
                .await?;
            }
            if fin_cuenta {
                break;
            }
            continue;
        }
        let metodo = msg["method"].as_str().unwrap_or("");
        let p = &msg["params"];
        // Peticiones del servidor: aprobaciones.
        if let Some(id) = msg.get("id").cloned() {
            let que = match metodo {
                "item/commandExecution/requestApproval" => format!("comando {}", p["command"]),
                "item/fileChange/requestApproval" => format!("cambio de archivos {}", p["reason"]),
                otro => format!("{otro}: {p}"),
            };
            Evento::PideComando {
                id: id.to_string(),
                que,
            }
            .mostrar();
            let decision = match decidir(permisos).await? {
                Decision::Permitir => "accept",
                Decision::PermitirSesion => "acceptForSession",
                Decision::Rechazar => "decline",
            };
            enviar(
                &mut entrada,
                &json!({"jsonrpc": "2.0", "id": id, "result": {"decision": decision}}),
            )
            .await?;
            continue;
        }
        let evento = match metodo {
            "item/completed" => traducir_item_codex(&p["item"]),
            "turn/plan/updated" => Some(Evento::Plan(p["plan"].to_string())),
            "thread/tokenUsage/updated" => Some(Evento::Consumo(format!(
                "tokens {}",
                p["tokenUsage"]["total"]
            ))),
            "account/rateLimits/updated" => {
                Some(Evento::Consumo(format!("límites {}", p["rateLimits"])))
            }
            "error" => Some(Evento::Error(p.to_string())),
            "turn/completed" => {
                Evento::Termino(format!("turno {}", p["turn"]["status"])).mostrar();
                break;
            }
            m if m.ends_with("/delta") || m.ends_with("Delta") => None,
            _ => Some(Evento::Otro(format!("{metodo}: {p}"))),
        };
        if let Some(e) = evento {
            e.mostrar();
        }
    }
    drop(entrada);
    hijo.kill().await.ok();
    Ok(())
}

fn traducir_item_codex(item: &Value) -> Option<Evento> {
    match item["type"].as_str()? {
        "agentMessage" => Some(Evento::Mensaje(item["text"].as_str()?.to_owned())),
        "plan" => Some(Evento::Plan(item["text"].as_str()?.to_owned())),
        "fileChange" => Some(Evento::EditoArchivo(
            item["changes"]
                .as_array()?
                .iter()
                .filter_map(|c| c["path"].as_str())
                .collect::<Vec<_>>()
                .join(", "),
        )),
        "commandExecution" => {
            let comando = item["command"].as_str().unwrap_or("").to_owned();
            Some(if parece_test(&comando) {
                Evento::CorrioTests(format!("{comando} → {}", item["exitCode"]))
            } else {
                Evento::Otro(format!("comando {comando} → {}", item["exitCode"]))
            })
        }
        "userMessage" | "reasoning" => None,
        otro => Some(Evento::Otro(format!("item {otro}"))),
    }
}

#[derive(Default)]
struct Rpc {
    siguiente: u64,
}

impl Rpc {
    async fn pedir(
        &mut self,
        entrada: &mut ChildStdin,
        metodo: &str,
        params: Value,
    ) -> Result<u64> {
        self.siguiente += 1;
        let id = self.siguiente;
        enviar(
            entrada,
            &json!({"jsonrpc": "2.0", "id": id, "method": metodo, "params": params}),
        )
        .await?;
        Ok(id)
    }

    async fn notificar(&mut self, entrada: &mut ChildStdin, metodo: &str) -> Result<()> {
        enviar(entrada, &json!({"jsonrpc": "2.0", "method": metodo})).await
    }
}

// ---------------------------------------------------------------- gemini

async fn gemini_acp(
    binario: &Path,
    home: Option<&Path>,
    cwd: &Path,
    permisos: Permisos,
    pedido: &str,
) -> Result<()> {
    let mut cmd = Command::new(binario);
    cmd.env_clear().envs(entorno_limpio());
    if let Some(h) = home {
        cmd.env("GEMINI_CLI_HOME", h);
    }
    let mut hijo = cmd
        .arg("--acp")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("no se pudo lanzar {}", binario.display()))?;
    let mut entrada = hijo.stdin.take().expect("stdin");
    let mut lineas = BufReader::new(hijo.stdout.take().expect("stdout")).lines();
    let mut rpc = Rpc::default();

    let id_init = rpc
        .pedir(
            &mut entrada,
            "initialize",
            json!({"protocolVersion": 1,
                   "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}},
                   "clientInfo": {"name": "hedy", "version": "0.0.0"}}),
        )
        .await?;
    let mut id_sesion = None;
    let mut id_prompt = None;

    while let Some(linea) = lineas.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&linea) else {
            continue;
        };
        guardar_muestra("gemini", &msg);
        let metodo = msg["method"].as_str().unwrap_or("");
        // Respuestas a nuestras peticiones.
        if let Some(id) = msg["id"].as_u64().filter(|_| metodo.is_empty()) {
            if let Some(error) = msg.get("error") {
                Evento::Error(format!("rpc {id}: {error}")).mostrar();
                break;
            }
            let r = &msg["result"];
            if id == id_init {
                Evento::Otro(format!(
                    "agente {} · protocolo {} · métodos de login {}",
                    r["agentInfo"], r["protocolVersion"], r["authMethods"]
                ))
                .mostrar();
                if pedido.is_empty() {
                    break;
                }
                id_sesion = Some(
                    rpc.pedir(
                        &mut entrada,
                        "session/new",
                        json!({"cwd": cwd.canonicalize()?, "mcpServers": []}),
                    )
                    .await?,
                );
            } else if Some(id) == id_sesion {
                Evento::Otro(format!("sesión {} · modos {}", r["sessionId"], r["modes"])).mostrar();
                id_prompt = Some(
                    rpc.pedir(
                        &mut entrada,
                        "session/prompt",
                        json!({"sessionId": r["sessionId"],
                               "prompt": [{"type": "text", "text": pedido}]}),
                    )
                    .await?,
                );
            } else if Some(id) == id_prompt {
                Evento::Termino(format!("stopReason {}", r["stopReason"])).mostrar();
                break;
            }
            continue;
        }
        let p = &msg["params"];
        if metodo == "session/request_permission" {
            let id = msg["id"].clone();
            Evento::PideComando {
                id: id.to_string(),
                que: format!("{} {}", p["toolCall"]["title"], p["toolCall"]["kind"]),
            }
            .mostrar();
            // Las opciones las da el agente; se elige por su `kind`.
            let tipo = match decidir(permisos).await? {
                Decision::Permitir => "allow_once",
                Decision::PermitirSesion => "allow_always",
                Decision::Rechazar => "reject_once",
            };
            let opcion = p["options"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["kind"] == tipo)
                .map(|o| o["optionId"].clone());
            let resultado = match opcion {
                Some(id_opcion) => {
                    json!({"outcome": {"outcome": "selected", "optionId": id_opcion}})
                }
                None => json!({"outcome": {"outcome": "cancelled"}}),
            };
            enviar(
                &mut entrada,
                &json!({"jsonrpc": "2.0", "id": id, "result": resultado}),
            )
            .await?;
            continue;
        }
        if metodo == "session/update" {
            if let Some(e) = traducir_gemini(&p["update"]) {
                e.mostrar();
            }
            continue;
        }
        Evento::Otro(format!("{metodo}: {p}")).mostrar();
    }
    drop(entrada);
    hijo.kill().await.ok();
    Ok(())
}

fn traducir_gemini(update: &Value) -> Option<Evento> {
    match update["sessionUpdate"].as_str()? {
        // Los trozos de mensaje llegan uno a uno; aquí se muestran tal cual.
        "agent_message_chunk" => Some(Evento::Mensaje(
            update["content"]["text"].as_str()?.to_owned(),
        )),
        "agent_thought_chunk" | "user_message_chunk" => None,
        "plan" => Some(Evento::Plan(update["entries"].to_string())),
        "tool_call" => {
            let titulo = update["title"].as_str().unwrap_or("").to_owned();
            Some(match update["kind"].as_str() {
                Some("edit") => Evento::EditoArchivo(titulo),
                Some("execute") if parece_test(&titulo) => Evento::CorrioTests(titulo),
                _ => Evento::Otro(format!("herramienta {} {titulo}", update["kind"])),
            })
        }
        "tool_call_update" => None,
        otro => Some(Evento::Otro(format!("{otro}: {update}"))),
    }
}

// ---------------------------------------------------------------- pty

fn pty(carpetas: &[(&str, Option<PathBuf>)], segundos: u64, programa: &[String]) -> Result<()> {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};

    let sistema = native_pty_system();
    let par = sistema.openpty(PtySize {
        rows: 30,
        cols: 100,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(&programa[0]);
    cmd.args(&programa[1..]);
    // portable-pty arranca en $HOME si no se indica la carpeta.
    cmd.cwd(std::env::current_dir()?);
    cmd.env_clear();
    for (k, v) in entorno_limpio() {
        cmd.env(k, v);
    }
    cmd.env("TERM", "xterm-256color");
    for (variable, carpeta) in carpetas {
        if let Some(dir) = carpeta {
            cmd.env(variable, dir);
        }
    }
    let mut hijo = par.slave.spawn_command(cmd)?;
    drop(par.slave);

    let mut lector = par.master.try_clone_reader()?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = lector.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let mut pantalla = Vec::new();
    let limite = Instant::now() + Duration::from_secs(segundos);
    while Instant::now() < limite {
        if let Ok(trozo) = rx.recv_timeout(Duration::from_millis(200)) {
            pantalla.extend(trozo);
        }
        if hijo.try_wait()?.is_some() {
            break;
        }
    }
    hijo.kill().ok();
    let texto = quitar_ansi(&String::from_utf8_lossy(&pantalla));
    let lineas: Vec<&str> = texto
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect();
    println!(
        "{} bytes recibidos por la PTY; texto visible:",
        pantalla.len()
    );
    for l in lineas.iter().take(40) {
        println!("  │ {l}");
    }
    Ok(())
}

// ---------------------------------------------------------------- utilidades

enum Decision {
    Permitir,
    PermitirSesion,
    Rechazar,
}

async fn decidir(permisos: Permisos) -> Result<Decision> {
    Ok(match permisos {
        Permisos::Permitir => Decision::Permitir,
        Permisos::Rechazar => Decision::Rechazar,
        Permisos::Preguntar => {
            print!("    ¿[p]ermitir, permitir en [e]ste chat o [r]echazar? ");
            std::io::stdout().flush()?;
            let mut linea = String::new();
            BufReader::new(tokio::io::stdin())
                .read_line(&mut linea)
                .await?;
            match linea.trim() {
                "p" => Decision::Permitir,
                "e" => Decision::PermitirSesion,
                "r" | "" => Decision::Rechazar,
                otra => bail!("respuesta no válida: {otra}"),
            }
        }
    })
}

async fn enviar(entrada: &mut ChildStdin, valor: &Value) -> Result<()> {
    let mut linea = serde_json::to_vec(valor)?;
    linea.push(b'\n');
    entrada.write_all(&linea).await?;
    entrada.flush().await?;
    Ok(())
}

/// Guarda cada mensaje crudo en `HEDY_MUESTRAS/<cli>.jsonl` para documentar formatos.
fn guardar_muestra(cli: &str, msg: &Value) {
    let Some(dir) = std::env::var_os("HEDY_MUESTRAS") else {
        return;
    };
    let ruta = Path::new(&dir).join(format!("{cli}.jsonl"));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ruta)
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn parece_test(comando: &str) -> bool {
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
    .any(|t| comando.contains(t))
}

fn recortar(texto: &str, max: usize) -> String {
    let una_linea = texto.replace('\n', " ⏎ ");
    if una_linea.chars().count() <= max {
        una_linea
    } else {
        una_linea.chars().take(max).collect::<String>() + "…"
    }
}

fn quitar_ansi(texto: &str) -> String {
    let mut salida = String::with_capacity(texto.len());
    let mut chars = texto.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            if c != '\r' {
                salida.push(c);
            }
            continue;
        }
        match chars.next() {
            // CSI: termina en un byte entre @ y ~.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: termina en BEL o ESC \.
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
    salida
}
