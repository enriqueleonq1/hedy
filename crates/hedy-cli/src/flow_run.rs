//! `hedy flow run <flow> "<request>"`: a flow started and followed in the
//! terminal (docs/decisions.md §44). Each role's start, each check and the
//! end are printed; approvals, plans and permission requests are asked here,
//! one at a time. Without a terminal, permission requests are declined and an
//! approval leaves the task waiting for the app.
//!
//! `hedy do "<request>"` follows a task with one AI the same way, with the
//! project's way of working (docs/decisions.md §60): its plan is approved with
//! Enter or corrected by writing, and Hedy's checks are printed.

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use anyhow::{Context, Result, bail};
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

/// Text shown as it was written, line by line, each line cleaned.
fn clean_lines(text: &str) -> String {
    text.lines().map(clean).collect::<Vec<_>>().join("\n")
}

fn yes(line: &str) -> bool {
    matches!(
        line.trim().to_lowercase().as_str(),
        "s" | "si" | "sí" | "y" | "yes"
    )
}

/// Hedy with its task events coming here, and Ctrl+C.
fn listen(hedy: &Hedy) -> Result<(Hedy, mpsc::Sender<Signal>, mpsc::Receiver<Signal>)> {
    let (tx, rx) = mpsc::channel();
    let hedy = hedy.clone().with_task_events(Arc::new(Forward(tx.clone())));
    let interrupt = tx.clone();
    ctrlc::set_handler(move || {
        let _ = interrupt.send(Signal::Interrupt);
    })
    .context("Ctrl+C")?;
    Ok((hedy, tx, rx))
}

/// `hedy do`: a task with one AI and the project's way of working; `plan`
/// overrides its plan for this task.
pub fn run_do(
    hedy: &Hedy,
    dir: &Path,
    request: &str,
    plan: Option<bool>,
    model: Option<String>,
) -> Result<()> {
    refuse_in_agent_session()?;
    let project = project_here(hedy, dir)?;
    let ask = interactive();
    let (hedy, tx, rx) = listen(hedy)?;
    let task = hedy.start_task(NewTask {
        images: Vec::new(),
        project: project.path.clone(),
        prompt: request.to_owned(),
        mode: TaskMode::Project,
        model,
        plan_first: plan.unwrap_or(project.working.plan),
        permissions: None,
    })?;
    // The project's way of working, and what this task changes of it.
    let mut name = project.working.name();
    match plan {
        Some(true) if !project.working.plan => {
            name.push_str(&tr!(" · esta vez con plan", " · this time with a plan"))
        }
        Some(false) if project.working.plan => {
            name.push_str(&tr!(" · esta vez sin plan", " · this time without a plan"))
        }
        _ => {}
    }
    follow(&hedy, task, &name, ask, true, tx, rx)
}

pub fn run(
    hedy: &Hedy,
    dir: &Path,
    flow: &str,
    request: &str,
    model: Option<String>,
) -> Result<()> {
    refuse_in_agent_session()?;
    let project = project_here(hedy, dir)?;
    let ask = interactive();
    let (hedy, tx, rx) = listen(hedy)?;
    // A team's test command runs here only once the user allows it.
    let entry = hedy.flow(Some(&project.path), flow)?;
    for command in &entry.unapproved {
        let shown = clean(command);
        if !ask {
            bail!(
                "{}",
                tr!(
                    "este flujo del equipo corre «{shown}»: permítelo con `hedy flow allow \"{shown}\"`",
                    "this team flow runs «{shown}»: allow it with `hedy flow allow \"{shown}\"`"
                )
            );
        }
        let answer = crate::sync::read_line(&tr!(
            "Este flujo del equipo corre en tu PC, aislado y sin red: {shown}\n¿Lo permites? [s/N] ",
            "This team flow runs on your PC, isolated and offline: {shown}\nDo you allow it? [y/N] "
        ))?;
        if !yes(&answer) {
            bail!("{}", tr!("no se ejecutó nada", "nothing was run"));
        }
        hedy.approve_flow_command(command)?;
    }
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
    let name = task
        .flow
        .as_ref()
        .map(|r| r.flow.name.clone())
        .unwrap_or_default();
    follow(&hedy, task, &name, ask, false, tx, rx)
}

/// Prints the task as it goes and asks its questions here until it ends.
/// `one_ai`: a task with one AI, whose plan Enter approves.
fn follow(
    hedy: &Hedy,
    task: Task,
    name: &str,
    ask: bool,
    one_ai: bool,
    tx: mpsc::Sender<Signal>,
    rx: mpsc::Receiver<Signal>,
) -> Result<()> {
    let s = Style::for_stdout();
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
    println!(
        "{} {}",
        s.bold(clean(name)),
        s.muted(format!("· {}", clean(&task.title)))
    );
    let id = task.id.clone();
    // One question at a time, in the order they came.
    let mut waiting: VecDeque<(Question, String)> = VecDeque::new();
    let mut asking: Option<Question> = None;
    // Whether the run stopped on something to read (no verdict, out of rounds…).
    let mut trouble = false;
    let still_open = |question: &Question| match question {
        Question::Permission(request_id) | Question::Plan(request_id) => {
            hedy.is_permission_pending(&id, request_id) || matches!(question, Question::Plan(_))
        }
        Question::Approval => hedy
            .task(&id)
            .is_ok_and(|t| t.status == TaskStatus::WaitingApproval),
    };
    let finish = |status: TaskStatus, trouble: bool| {
        let (mark, done) = match (status, trouble) {
            (TaskStatus::Review, false) => (
                s.ok("■"),
                tr!(
                    "Listo: revisa los cambios en la app, o con `hedy tasks`.",
                    "Done: review the changes in the app, or with `hedy tasks`."
                ),
            ),
            (TaskStatus::Review, true) => (
                s.warn("■"),
                tr!(
                    "Se detuvo antes de terminar: lee el aviso y revisa los cambios en la app.",
                    "It stopped before the end: read the notice and review the changes in the app."
                ),
            ),
            _ => (
                s.warn("■"),
                tr!(
                    "Se detuvo con un error: míralo en la app.",
                    "It stopped with an error: see it in the app."
                ),
            ),
        };
        println!("{mark} {done}");
    };
    loop {
        if asking.is_none() {
            while let Some((question, text)) = waiting.pop_front() {
                if still_open(&question) {
                    print!("{text} ");
                    let _ = std::io::stdout().flush();
                    asking = Some(question);
                    break;
                }
            }
        }
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
                        // With one AI, Enter approves and anything written corrects it.
                        let approve = yes(&line) || (one_ai && line.trim().is_empty());
                        let feedback = (!approve).then_some(line.trim());
                        let _ = hedy.answer_plan(&id, &request_id, approve, feedback);
                    }
                    Some(Question::Approval) => {
                        // Anything but yes: what to change (or a plain no).
                        let feedback = line.trim();
                        let feedback =
                            (!matches!(feedback.to_lowercase().as_str(), "" | "n" | "no"))
                                .then_some(feedback);
                        if let Err(e) = hedy.answer_flow_approval(&id, yes(&line), feedback) {
                            println!("{} {e}", s.warn("!"));
                        }
                    }
                    None => {}
                }
            }
            Ok(Signal::Event(event_id, event)) if event_id == id => match event {
                AgentEvent::PhaseStarted { agent, .. } if one_ai => {
                    println!("{} {}", s.ok("▶"), s.muted(clean(&agent)));
                }
                AgentEvent::CheckRan {
                    check,
                    command,
                    passed,
                    ..
                } => {
                    let mark = if passed { s.ok("✓") } else { s.warn("✗") };
                    println!("{mark} {:<6} {}", clean(&check), s.muted(clean(&command)));
                }
                AgentEvent::ChecksSkipped { reason, .. } => {
                    println!("{} {}", s.warn("!"), clean(&reason));
                }
                AgentEvent::ReviewRan { warnings, .. } => {
                    if warnings.is_empty() {
                        println!(
                            "{} {}",
                            s.ok("✓"),
                            tr!("revisión: no vio errores", "review: it saw no mistakes")
                        );
                    }
                    for w in warnings {
                        let at = w.line.map_or(w.path.clone(), |n| format!("{}:{n}", w.path));
                        println!("{} {} {}", s.warn("!"), clean(&at), clean(&w.message));
                    }
                }
                AgentEvent::PhaseStarted { agent, .. } => {
                    let role = hedy
                        .task(&id)
                        .ok()
                        .and_then(|t| Some(t.flow?.current_role()?.name.clone()))
                        .unwrap_or_default();
                    println!(
                        "{} {} {}",
                        s.ok("▶"),
                        s.bold(clean(&role)),
                        s.muted(clean(&agent))
                    );
                }
                AgentEvent::FlowCheck { what, passed, .. } => {
                    let (mark, word) = if passed {
                        (s.ok("✓"), tr!("pasa", "passes"))
                    } else {
                        (s.warn("✗"), tr!("no pasa", "does not pass"))
                    };
                    println!("{mark} {} {}", clean(&what), s.muted(word));
                }
                AgentEvent::FlowOutOfRounds { rounds, asked, .. } => {
                    println!(
                        "{} {}",
                        s.warn("↺"),
                        tr!(
                            "Se acabaron las vueltas ({rounds}). Lo último que se pidió:",
                            "Out of rounds ({rounds}). The last thing asked:"
                        )
                    );
                    let text: String = asked.chars().take(SHOWN).collect();
                    println!("{}\n", clean_lines(&text));
                }
                AgentEvent::FlowApproval { .. } => {
                    if let Some(last) = hedy.task_board(&id).ok().and_then(|b| b.last().cloned()) {
                        let text: String = last.text.chars().take(SHOWN).collect();
                        println!("\n{}\n{}\n", s.bold(clean(&last.role)), clean_lines(&text));
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
                    waiting.push_back((
                        Question::Approval,
                        tr!(
                            "¿Lo apruebas? [s / escribe qué cambiar]",
                            "Do you approve it? [y / write what to change]"
                        ),
                    ));
                }
                AgentEvent::Plan {
                    text,
                    request_id: Some(request_id),
                } => {
                    println!("\n{}\n", clean_lines(&text));
                    if ask {
                        let question = if one_ai {
                            tr!(
                                "Corrige el plan, o Enter para aprobarlo ›",
                                "Correct the plan, or Enter to approve it ›"
                            )
                        } else {
                            tr!(
                                "¿Apruebas el plan? [s / escribe qué cambiar]",
                                "Approve the plan? [y / write what to change]"
                            )
                        };
                        waiting.push_back((Question::Plan(request_id), question));
                    } else {
                        // With no one to ask here, the plan waits: never turned down for you.
                        println!(
                            "{}",
                            tr!(
                                "El plan espera tu OK: respóndelo en la app.",
                                "The plan waits for your OK: answer it in the app."
                            )
                        );
                        return Ok(());
                    }
                }
                AgentEvent::PermissionRequest {
                    request_id,
                    summary,
                    ..
                } if hedy.is_permission_pending(&id, &request_id) => {
                    if ask {
                        let what = clean(&summary);
                        waiting.push_back((
                            Question::Permission(request_id),
                            tr!(
                                "  Pide: {what} ¿Permitir? [s/N]",
                                "  Asks: {what} Allow? [y/N]"
                            ),
                        ));
                    } else {
                        let _ = hedy.answer_permission(&id, &request_id, Decision::Deny);
                    }
                }
                // Answered elsewhere (the app, a rule): no longer asked here.
                AgentEvent::PermissionResolved { request_id, .. } => {
                    if matches!(&asking, Some(Question::Permission(r) | Question::Plan(r)) if *r == request_id)
                    {
                        asking = None;
                        println!();
                    }
                }
                AgentEvent::Error { message } => {
                    trouble = true;
                    println!("{} {}", s.warn("!"), clean(&message));
                }
                _ => {}
            },
            Ok(Signal::Updated(updated, status)) if updated == id => match status {
                TaskStatus::Review | TaskStatus::Failed => {
                    finish(status, trouble);
                    return Ok(());
                }
                // An approval answered in the app.
                TaskStatus::Running if matches!(asking, Some(Question::Approval)) => {
                    asking = None;
                    println!();
                }
                _ => {}
            },
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let Ok(now) = hedy.task(&id) else {
                    return Ok(());
                };
                if now.status.is_closed() {
                    return Ok(());
                }
                if matches!(now.status, TaskStatus::Review | TaskStatus::Failed) {
                    finish(now.status, trouble);
                    return Ok(());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}
