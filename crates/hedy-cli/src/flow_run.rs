//! `hedy flow run <flow> "<request>"`: a flow started and followed in the
//! terminal (docs/decisions.md §44). Each role's start, each check and the
//! end are printed; approvals, plans and permission requests are asked here,
//! one at a time. Without a terminal, permission requests are declined and an
//! approval leaves the task waiting for the app.

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use anyhow::{Context, Result};
use hedy_core::application::ports::TaskEvents;
use hedy_core::application::{Hedy, NewTask};
use hedy_core::domain::{AgentEvent, Decision, Task, TaskId, TaskMode, TaskStatus};
use hedy_core::tr;

use crate::commands::{project_here, refuse_in_agent_session};
use crate::style::Style;
use crate::sync::interactive;
use crate::team::clean;

/// How often the wait looks at the task, in case it ended elsewhere.
const POLL: Duration = Duration::from_secs(1);
/// Longest text of a role's work shown before an approval.
const SHOWN: usize = 2_000;

enum Signal {
    Event(TaskId, AgentEvent),
    Updated(TaskId, TaskStatus),
    /// A line typed by the user (`None`: stdin closed).
    Line(Option<String>),
    Interrupt,
}

struct Forward(mpsc::Sender<Signal>);

impl TaskEvents for Forward {
    fn event(&self, id: &TaskId, event: &AgentEvent) {
        let _ = self.0.send(Signal::Event(id.clone(), event.clone()));
    }

    fn task_updated(&self, task: &Task) {
        let _ = self.0.send(Signal::Updated(task.id.clone(), task.status));
    }
}

/// What the user is being asked.
enum Question {
    Permission(String),
    Plan(String),
    Approval,
}

fn yes(line: &str) -> bool {
    matches!(
        line.trim().to_lowercase().as_str(),
        "s" | "si" | "sí" | "y" | "yes"
    )
}

pub fn run(
    hedy: &Hedy,
    dir: &Path,
    flow: &str,
    request: &str,
    model: Option<String>,
) -> Result<()> {
    refuse_in_agent_session()?;
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let ask = interactive();
    let (tx, rx) = mpsc::channel();
    let hedy = hedy.clone().with_task_events(Arc::new(Forward(tx.clone())));
    let interrupt = tx.clone();
    ctrlc::set_handler(move || {
        let _ = interrupt.send(Signal::Interrupt);
    })
    .context("Ctrl+C")?;
    let task = hedy.start_flow(
        NewTask {
            images: Vec::new(),
            project: project.path.clone(),
            prompt: request.to_owned(),
            mode: TaskMode::Project,
            model,
            plan_first: false,
            permissions: None,
        },
        flow,
    )?;
    if ask {
        // Answers are read on their own thread, so Ctrl+C works mid-question.
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                if tx.send(Signal::Line(line.ok())).is_err() {
                    return;
                }
            }
            let _ = tx.send(Signal::Line(None));
        });
    }
    let name = task
        .flow
        .as_ref()
        .map(|r| r.flow.name.clone())
        .unwrap_or_default();
    println!("{} {}", s.bold(&name), s.muted(format!("· {}", task.title)));
    let id = task.id.clone();
    let mut asking: Option<Question> = None;
    let question = |text: String| {
        print!("{text} ");
        let _ = std::io::stdout().flush();
    };
    loop {
        match rx.recv_timeout(POLL) {
            Ok(Signal::Interrupt) => {
                let _ = hedy.stop_task(&id);
                let _ = writeln!(
                    std::io::stdout(),
                    "\n{} {}",
                    s.warn("■"),
                    tr!(
                        "Detenido: la tarea queda para revisar en la app.",
                        "Stopped: the task stays for review in the app."
                    )
                );
                return Ok(());
            }
            Ok(Signal::Line(line)) => {
                let line = line.unwrap_or_default();
                match asking.take() {
                    Some(Question::Permission(request_id)) => {
                        let decision = if yes(&line) {
                            Decision::Allow
                        } else {
                            Decision::Deny
                        };
                        let _ = hedy.answer_permission(&id, &request_id, decision);
                    }
                    Some(Question::Plan(request_id)) => {
                        let feedback = (!yes(&line)).then_some(line.trim());
                        let _ = hedy.answer_plan(&id, &request_id, yes(&line), feedback);
                    }
                    Some(Question::Approval) => {
                        // Anything but yes: what to change (or a plain no).
                        let feedback = line.trim();
                        let feedback =
                            (!matches!(feedback.to_lowercase().as_str(), "" | "n" | "no"))
                                .then_some(feedback);
                        if let Err(e) = hedy.answer_flow_approval(&id, yes(&line), feedback) {
                            println!("{} {e}", s.warn("!"));
                            asking = Some(Question::Approval);
                        }
                    }
                    None => {}
                }
            }
            Ok(Signal::Event(event_id, event)) if event_id == id => match event {
                AgentEvent::PhaseStarted { agent, .. } => {
                    let role = hedy
                        .task(&id)
                        .ok()
                        .and_then(|t| Some(t.flow?.current_role()?.name.clone()))
                        .unwrap_or_default();
                    println!("{} {} {}", s.ok("▶"), s.bold(&role), s.muted(agent));
                }
                AgentEvent::FlowCheck { what, passed, .. } => {
                    let (mark, word) = if passed {
                        (s.ok("✓"), tr!("pasa", "passes"))
                    } else {
                        (s.warn("✗"), tr!("no pasa", "does not pass"))
                    };
                    println!("{mark} {} {}", clean(&what), s.muted(word));
                }
                AgentEvent::FlowApproval { .. } => {
                    if let Some(last) = hedy.task_board(&id).ok().and_then(|b| b.last().cloned()) {
                        let text: String = last.text.chars().take(SHOWN).collect();
                        println!("\n{}\n{}\n", s.bold(clean(&last.role)), clean(&text));
                    }
                    if !ask {
                        println!(
                            "{}",
                            tr!(
                                "Espera tu aprobación: respóndela en la app.",
                                "It waits for your approval: answer it in the app."
                            )
                        );
                        return Ok(());
                    }
                    question(tr!(
                        "¿Lo apruebas? [s / escribe qué cambiar]",
                        "Do you approve it? [y / write what to change]"
                    ));
                    asking = Some(Question::Approval);
                }
                AgentEvent::Plan {
                    text,
                    request_id: Some(request_id),
                } => {
                    println!("\n{}\n", clean(&text));
                    if ask {
                        question(tr!(
                            "¿Apruebas el plan? [s / escribe qué cambiar]",
                            "Approve the plan? [y / write what to change]"
                        ));
                        asking = Some(Question::Plan(request_id));
                    } else {
                        let _ = hedy.answer_plan(&id, &request_id, false, None);
                    }
                }
                AgentEvent::PermissionRequest {
                    request_id,
                    summary,
                    ..
                } if hedy.is_permission_pending(&id, &request_id) => {
                    if ask {
                        let what = clean(&summary);
                        question(tr!(
                            "  Pide: {what} ¿Permitir? [s/N]",
                            "  Asks: {what} Allow? [y/N]"
                        ));
                        asking = Some(Question::Permission(request_id));
                    } else {
                        let _ = hedy.answer_permission(&id, &request_id, Decision::Deny);
                    }
                }
                AgentEvent::Error { message } => {
                    println!("{} {}", s.warn("!"), clean(&message));
                }
                _ => {}
            },
            Ok(Signal::Updated(updated, status)) if updated == id => {
                if matches!(status, TaskStatus::Review | TaskStatus::Failed) && asking.is_none() {
                    let done = match status {
                        TaskStatus::Review => tr!(
                            "Listo: revisa los cambios en la app, o con `hedy tasks`.",
                            "Done: review the changes in the app, or with `hedy tasks`."
                        ),
                        _ => tr!(
                            "Se detuvo con un error: míralo en la app.",
                            "It stopped with an error: see it in the app."
                        ),
                    };
                    println!("{} {done}", s.ok("■"));
                    return Ok(());
                }
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if hedy.task(&id).is_ok_and(|t| t.status.is_closed()) {
                    return Ok(());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}
