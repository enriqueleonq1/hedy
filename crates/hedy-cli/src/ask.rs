//! `hedy ask`: a quick question from the terminal. It is a conversation like
//! the app's "Pregunta rápida", started with the same use cases and kept in
//! the same store, so it shows up in Hedy → Conversaciones with its answer.
//!
//! The answer goes to stdout as it arrives; tool activity and status lines go
//! to stderr, so `hedy ask "…" > answer.md` keeps only the answer.

use std::io::{ErrorKind, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::sync::{Arc, mpsc};

use anyhow::{Context, Result, anyhow};
use hedy_core::application::ports::TaskEvents;
use hedy_core::application::{AppError, Hedy, NewConversation};
use hedy_core::domain::{AgentEvent, LimitLevel, Task, TaskId, TaskMode, TaskStatus};
use hedy_core::i18n::{self, Language};
use hedy_core::tr;

use crate::cli::{AskArgs, Cli};
use crate::style::Style;

/// Exit code after Ctrl+C, as shells use (128 + SIGINT).
const INTERRUPTED: u8 = 130;
/// Exit code when there is no question: the usage was printed instead.
const USAGE: u8 = 2;

/// What the main thread waits on: the task's events, its status, Ctrl+C.
enum Signal {
    Event(AgentEvent),
    /// A task's new status.
    Updated(TaskId, TaskStatus),
    Interrupt,
}

/// Forwards the task's events to the main thread, which prints them.
struct Forward(mpsc::Sender<Signal>);

impl TaskEvents for Forward {
    fn event(&self, _: &TaskId, event: &AgentEvent) {
        let _ = self.0.send(Signal::Event(event.clone()));
    }

    fn task_updated(&self, task: &Task) {
        let _ = self.0.send(Signal::Updated(task.id.clone(), task.status));
    }
}

/// How the turn ended.
enum Outcome {
    Answered,
    Failed,
    Interrupted,
}

pub fn ask(hedy: &Hedy, args: AskArgs) -> Result<ExitCode> {
    let Some(question) = read_question(&args.question)? else {
        let mut command = Cli::full_command();
        command.build();
        let ask = command
            .find_subcommand_mut("ask")
            .expect("ask is a subcommand");
        ask.print_help()?;
        return Ok(ExitCode::from(USAGE));
    };

    let (tx, rx) = mpsc::channel();
    let hedy = hedy.clone().with_task_events(Arc::new(Forward(tx.clone())));
    ctrlc::set_handler(move || {
        let _ = tx.send(Signal::Interrupt);
    })
    .context("Ctrl+C")?;

    let err = Style::for_stderr();
    let (task, latest) = match args.follow.as_deref() {
        None => {
            let mode = if args.research {
                TaskMode::Research
            } else {
                TaskMode::Ask
            };
            let task = hedy.start_conversation(NewConversation {
                prompt: question,
                mode,
                account: args.account,
                model: args.model,
                images: Vec::new(),
            })?;
            (task, true)
        }
        Some(id) => {
            let latest = hedy.conversations()?.into_iter().next();
            let task = match id {
                "" => latest.clone().ok_or_else(|| {
                    anyhow!(tr!(
                        "todavía no hay conversaciones; empieza una con `hedy ask \"…\"`",
                        "there are no conversations yet; start one with `hedy ask \"…\"`"
                    ))
                })?,
                id => hedy.task(&TaskId::new(id))?,
            };
            if !task.conversation {
                return Err(AppError::NotAConversation(task.id).into());
            }
            hedy.send_message(&task.id, &question)?;
            let is_latest = latest.is_some_and(|l| l.id == task.id);
            (task, is_latest)
        }
    };
    print_header(&hedy, &err, &task);

    let mut turn = Turn::new(&hedy, &task, args.json);
    let outcome = loop {
        let Ok(signal) = rx.recv() else {
            break Outcome::Failed;
        };
        match signal {
            Signal::Interrupt => {
                hedy.stop_task(&task.id)?;
                break Outcome::Interrupted;
            }
            Signal::Updated(id, status) if id == task.id => match status {
                TaskStatus::Running | TaskStatus::WaitingApproval => {}
                TaskStatus::Failed => break Outcome::Failed,
                _ => break Outcome::Answered,
            },
            Signal::Updated(..) => {}
            Signal::Event(event) => turn.show(&err, event),
        }
    };
    // The CLI is about to exit: its agent goes with it. Nothing else changes
    // (an answered turn is already in review, a failed one stays failed).
    let _ = hedy.stop_task(&task.id);
    // The account may have changed name, the model may come from the agent.
    let task = hedy.task(&task.id).unwrap_or(task);

    let code = match outcome {
        Outcome::Interrupted => {
            eprintln!(
                "{} {}",
                err.warn("■"),
                err.muted(tr!(
                    "Detenida. La conversación sigue en Hedy → Conversaciones.",
                    "Stopped. The conversation is still in Hedy → Conversations."
                ))
            );
            return Ok(ExitCode::from(INTERRUPTED));
        }
        Outcome::Answered => {
            if args.json {
                let json = serde_json::json!({
                    "id": task.id.as_str(),
                    "title": task.title,
                    "account": task.account.as_str(),
                    "answer": turn.answer(),
                });
                write_out(format!("{}\n", serde_json::to_string_pretty(&json)?).as_bytes())?;
            }
            if let Some(resets_at) = turn.limit {
                eprintln!(
                    "{} {}",
                    err.warn("!"),
                    limit_message(&hedy, &task, resets_at)
                );
            }
            ExitCode::SUCCESS
        }
        Outcome::Failed => {
            if let Some(resets_at) = turn.limit {
                eprintln!(
                    "{} {}",
                    err.error("✗"),
                    limit_message(&hedy, &task, resets_at)
                );
            } else if turn.error_shown {
                // The agent's error is already on screen.
            } else if let Some(reason) = turn.failure.filter(|r| !r.trim().is_empty()) {
                eprintln!("{} {}", err.error("✗"), reason.trim());
            } else {
                eprintln!(
                    "{} {}",
                    err.error("✗"),
                    tr!("No se pudo responder.", "It could not answer.")
                );
            }
            ExitCode::FAILURE
        }
    };
    print_footer(&hedy, &err, &task, latest);
    Ok(code)
}

/// The question from the arguments, or from stdin with `-` or a pipe.
/// `None`: there is none and stdin is a terminal.
fn read_question(words: &[String]) -> Result<Option<String>> {
    let stdin = std::io::stdin();
    let from_stdin = match words {
        [only] => only == "-",
        [] => !stdin.is_terminal(),
        _ => false,
    };
    if from_stdin {
        let mut text = String::new();
        stdin.lock().read_to_string(&mut text)?;
        return Ok(Some(text.trim().to_owned()));
    }
    Ok((!words.is_empty()).then(|| words.join(" ")))
}

/// `▶ ● Personal · Claude Opus`, on stderr.
fn print_header(hedy: &Hedy, s: &Style, task: &Task) {
    let Ok(account) = hedy.account(task.account.as_str()) else {
        return;
    };
    eprintln!(
        "{} {} {} {}",
        s.accent("▶"),
        s.chip(account.color),
        s.bold(&account.name),
        s.muted(format!(
            "· {}",
            account.provider.agent_label(task.model.as_deref())
        ))
    );
}

/// Where the conversation is and how to follow up, dimmed on stderr.
fn print_footer(hedy: &Hedy, s: &Style, task: &Task, latest: bool) {
    let mut parts = Vec::new();
    let account = hedy.account(task.account.as_str()).ok();
    if let Some(account) = &account {
        parts.push(account.name.clone());
    }
    if let Some(model) = task.model.as_deref().filter(|m| !m.is_empty()) {
        parts.push(model.to_owned());
    }
    parts.push(tr!(
        "Abierta también en Hedy → Conversaciones",
        "Also in Hedy → Conversations"
    ));
    let chip = account.map(|a| format!("{} ", s.chip(a.color)));
    eprintln!();
    eprintln!(
        "  {}{}",
        chip.unwrap_or_default(),
        s.muted(parts.join(" · "))
    );
    let follow = if latest {
        "hedy ask -c \"…\"".to_owned()
    } else {
        format!("hedy ask --continue={} \"…\"", task.id)
    };
    eprintln!(
        "  {} {}",
        s.muted(tr!("Para seguir:", "To follow up:")),
        s.command(follow)
    );
}

/// The limit was reached: which account, when it renews, and that Hedy does
/// not switch accounts (rule 3).
fn limit_message(hedy: &Hedy, task: &Task, resets_at: Option<i64>) -> String {
    let name = hedy
        .account(task.account.as_str())
        .map(|a| a.name)
        .unwrap_or_else(|_| task.account.to_string());
    let reset = resets_at
        .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
        .map(|at| {
            let at = at.with_timezone(&chrono::Local);
            match i18n::language() {
                Language::Spanish => {
                    let when = at.format("%d/%m %H:%M");
                    format!(" (se renueva el {when})")
                }
                Language::English => {
                    let when = at.format("%Y-%m-%d %H:%M");
                    format!(" (it renews on {when})")
                }
            }
        })
        .unwrap_or_default();
    tr!(
        "{name} llegó a su límite{reset}. Hedy no cambia de cuenta solo: espera a que se renueve, pregunta con otra cuenta (`hedy ask -a <cuenta> …`) o sigue en Hedy → Conversaciones.",
        "{name} hit its limit{reset}. Hedy does not switch accounts on its own: wait for it to renew, ask with another account (`hedy ask -a <account> …`) or go on in Hedy → Conversations."
    )
}

/// What the turn said and how it went.
struct Turn<'a> {
    hedy: &'a Hedy,
    id: TaskId,
    /// Print the answer as JSON at the end instead of as it arrives.
    json: bool,
    messages: Vec<String>,
    /// The limit was reached, and when it renews.
    limit: Option<Option<i64>>,
    /// Why the turn failed, when the agent said so.
    failure: Option<String>,
    /// An error was printed while the turn ran.
    error_shown: bool,
}

impl<'a> Turn<'a> {
    fn new(hedy: &'a Hedy, task: &Task, json: bool) -> Self {
        Self {
            hedy,
            id: task.id.clone(),
            json,
            messages: Vec::new(),
            limit: None,
            failure: None,
            error_shown: false,
        }
    }

    fn answer(&self) -> String {
        self.messages.join("\n\n")
    }

    fn show(&mut self, s: &Style, event: AgentEvent) {
        match event {
            AgentEvent::Message { text } => {
                if !self.json {
                    let gap = if self.messages.is_empty() { "" } else { "\n" };
                    let _ = write_out(format!("{gap}{text}\n").as_bytes());
                }
                self.messages.push(text);
            }
            AgentEvent::ToolUsed { tool, summary } => {
                let what = if summary.trim().is_empty() {
                    tool
                } else {
                    summary
                };
                eprintln!("  {}", s.muted(format!("· {what}")));
            }
            AgentEvent::CommandRun { command, .. } => {
                eprintln!("  {}", s.muted(format!("$ {command}")));
            }
            AgentEvent::PermissionRequest { summary, .. } => {
                let reason = tr!(
                    "Esta pregunta se hizo desde la terminal, donde Hedy no puede pedir permiso: responde sin esta acción.",
                    "This question was asked from the terminal, where Hedy cannot ask for permission: answer without this action."
                );
                let declined = self
                    .hedy
                    .decline_open_requests(&self.id, &reason)
                    .unwrap_or_default();
                if !declined.is_empty() {
                    eprintln!(
                        "{} {}",
                        s.warn("!"),
                        tr!(
                            "Pidió permiso para «{summary}» y Hedy lo rechazó: en la terminal no se aprueban acciones (ábrela en Hedy para eso).",
                            "It asked for permission to \"{summary}\" and Hedy declined: actions are not approved in the terminal (open it in Hedy for that)."
                        )
                    );
                }
            }
            AgentEvent::Plan {
                request_id: Some(request_id),
                ..
            } => {
                if self
                    .hedy
                    .answer_plan(&self.id, &request_id, false, None)
                    .is_ok()
                {
                    eprintln!(
                        "{} {}",
                        s.warn("!"),
                        tr!(
                            "Propuso un plan y Hedy lo rechazó: en la terminal no se aprueban planes.",
                            "It proposed a plan and Hedy declined it: plans are not approved in the terminal."
                        )
                    );
                }
            }
            AgentEvent::LimitWarning {
                level: LimitLevel::Reached,
                resets_at,
                ..
            } => self.limit = Some(resets_at),
            AgentEvent::LimitWarning {
                level: LimitLevel::Warning,
                percent,
                ..
            } => {
                let percent = percent.map(|p| format!(" ({p:.0} %)")).unwrap_or_default();
                eprintln!(
                    "{} {}",
                    s.warn("!"),
                    tr!(
                        "La cuenta está cerca de su límite{percent}.",
                        "The account is close to its limit{percent}."
                    )
                );
            }
            AgentEvent::Error { message } => {
                eprintln!("{} {}", s.error("✗"), message.trim());
                self.error_shown = true;
            }
            AgentEvent::Finished {
                ok: false, summary, ..
            } => self.failure = Some(summary),
            _ => {}
        }
    }
}

/// Writes to stdout now, treating a closed pipe (`hedy ask … | head`) as success.
fn write_out(bytes: &[u8]) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        Err(e) if e.kind() != ErrorKind::BrokenPipe => Err(e.into()),
        _ => Ok(()),
    }
}
