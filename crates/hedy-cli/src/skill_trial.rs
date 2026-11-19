//! `hedy skills try <name>`: a skill tried on real tasks, each run twice in
//! throwaway copies (with and without it), judged by what the runs did and
//! by the same check Hedy runs in each copy. In a terminal, each run's
//! permission requests are asked here, one at a time; elsewhere they are
//! declined.

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use anyhow::{Context, Result};
use hedy_core::application::Hedy;
use hedy_core::application::ports::TaskEvents;
use hedy_core::domain::skill_check::Verdict;
use hedy_core::domain::skill_trial::{TrialOutcome, TrialResult};
use hedy_core::domain::{AgentEvent, Decision, Task, TaskId, TaskStatus};
use hedy_core::tr;

use crate::commands::{project_here, refuse_in_agent_session};
use crate::skill_transfer::Reviewer;
use crate::spinner;
use crate::style::Style;
use crate::sync::{confirmed, interactive};
use crate::team::clean;

/// How often the wait looks at the trial, in case a run ended elsewhere
/// (the app stopped or discarded it).
const POLL: Duration = Duration::from_secs(1);
/// Longest tool input shown for a tool Hedy cannot summarize.
const INPUT_SHOWN: usize = 400;

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

/// A permission request waiting for the user.
struct Ask {
    task: TaskId,
    request_id: String,
    text: String,
}

pub fn try_skill(
    hedy: &Hedy,
    dir: &Path,
    name: &str,
    tasks: &[String],
    check: Option<&str>,
    yes: bool,
    reviewer: &Reviewer,
) -> Result<()> {
    refuse_in_agent_session()?;
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let skill = hedy.skill_named(&project.path, name)?;
    // Tasks an AI proposed from someone else's skill are read before
    // anything runs.
    if tasks.is_empty() && yes {
        anyhow::bail!(
            "{}",
            tr!(
                "`--yes` solo va con tus propias tareas (`--task \"…\"`): las que propone la IA las revisas tú antes de gastar",
                "`--yes` only goes with your own tasks (`--task \"…\"`): you read the ones the AI proposes before spending"
            )
        );
    }
    let tasks = match tasks {
        [] => spinner::while_running(
            &tr!(
                "La IA propone tareas de prueba…",
                "The AI proposes test tasks…"
            ),
            || hedy.propose_trial_tasks(&project.path, skill.id, reviewer.account, None),
        )?,
        given => given.to_vec(),
    };
    if tasks.is_empty() {
        anyhow::bail!(
            "{}",
            tr!(
                "la IA no propuso tareas: dale una con `--task \"…\"`",
                "the AI proposed no tasks: give one with `--task \"…\"`"
            )
        );
    }
    let check = match check.map(str::trim).filter(|c| !c.is_empty()) {
        Some(check) => Some(check.to_owned()),
        None => hedy.trial_check(&project.path)?,
    };
    let skill_name = clean(&skill.name);
    println!();
    println!(
        "  {}",
        s.bold(tr!("Probar {skill_name}", "Try {skill_name}"))
    );
    println!(
        "  {}",
        s.muted(tr!(
            "Cada tarea dos veces, en copias aparte: con la habilidad y sin ella. Tu carpeta no cambia; lo que permitas se ejecuta en esas copias, que comparten la configuración de git del proyecto, y se borran al final.",
            "Each task twice, in separate copies: with the skill and without it. Your folder does not change; what you allow runs in those copies, which share the project's git settings, and they are deleted at the end."
        ))
    );
    for task in &tasks {
        println!("    · {}", clean(task));
    }
    match &check {
        Some(check) => {
            let check = clean(check);
            println!(
                "  {}",
                s.muted(tr!(
                    "Al terminar cada una, Hedy corre `{check}` en su copia: es lo que decide si acierta.",
                    "When each one ends, Hedy runs `{check}` in its copy: that decides whether it passes."
                ))
            );
        }
        None => println!(
            "  {}",
            s.warn(tr!(
                "No encontré cómo se corren los tests: «acierta» solo querrá decir que terminó sin error. Usa `--check \"…\"` para darle un comando.",
                "I found no way to run the tests: «pass» will only mean it finished without an error. Use `--check \"…\"` to give a command."
            ))
        ),
    }
    let runs = tasks.len() * 2;
    let question = tr!(
        "¿Empezar? Son {runs} tareas de verdad y gastan tokens de la cuenta del proyecto. [s/N] ",
        "Start? These are {runs} real tasks and they spend the project account's tokens. [y/N] "
    );
    let again = format!("hedy skills try {} --task \"…\" --yes", skill.name);
    if !confirmed(yes, &question, &again)? {
        println!("{}", tr!("No se probó nada.", "Nothing was tried."));
        return Ok(());
    }

    let (tx, rx) = mpsc::channel();
    let hedy = hedy.clone().with_task_events(Arc::new(Forward(tx.clone())));
    let interrupt = tx.clone();
    // Ctrl+C, a closed terminal or `kill`: the trial is cancelled.
    ctrlc::set_handler(move || {
        let _ = interrupt.send(Signal::Interrupt);
    })
    .context("Ctrl+C")?;
    let ask = interactive();
    let trial = hedy.start_skill_trial(
        &project.path,
        skill.id,
        &tasks,
        reviewer.model,
        check.as_deref(),
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
    let ids: Vec<TaskId> = trial.tasks.iter().map(|t| t.id.clone()).collect();
    let label = |id: &TaskId| {
        let at = ids.iter().position(|i| i == id).unwrap_or(0);
        let side = match at % 2 {
            0 => tr!("con", "with"),
            _ => tr!("sin", "without"),
        };
        format!("{} {}", at / 2 + 1, side)
    };
    // Paths inside a run's copy, shown from the copy's root.
    let copy = tr!("<copia>/", "<copy>/");
    let roots: Vec<String> = trial
        .tasks
        .iter()
        .filter_map(|t| t.workspace.as_ref())
        .map(|ws| format!("{}/", ws.worktree.display()))
        .collect();
    let shown = |text: &str| {
        let text = roots
            .iter()
            .fold(text.to_owned(), |text, root| text.replace(root, &copy));
        clean(
            &text
                .replace("\r\n", "\n")
                .replace(['\n', '\r'], " ⏎ ")
                .replace('\t', "  "),
        )
    };

    let mut queue: VecDeque<Ask> = VecDeque::new();
    let mut asking: Option<Ask> = None;
    loop {
        if asking.is_none() {
            while let Some(next) = queue.pop_front() {
                if hedy.is_permission_pending(&next.task, &next.request_id) {
                    print!("{}", next.text);
                    let _ = std::io::stdout().flush();
                    asking = Some(next);
                    break;
                }
            }
        }
        match rx.recv_timeout(POLL) {
            Ok(Signal::Interrupt) => {
                if asking.is_some() {
                    println!();
                }
                hedy.cancel_skill_trial(&project.path, &trial.id);
                println!(
                    "{} {}",
                    s.warn("■"),
                    tr!(
                        "Prueba cancelada: no se guardó ningún resultado.",
                        "Trial cancelled: no result was kept."
                    )
                );
                return Ok(());
            }
            Ok(Signal::Line(line)) => {
                if let Some(question) = asking.take() {
                    let answer = line.unwrap_or_default().trim().to_lowercase();
                    let allow = matches!(answer.as_str(), "s" | "si" | "sí" | "y" | "yes");
                    let decision = if allow {
                        Decision::Allow
                    } else {
                        Decision::Deny
                    };
                    // Already answered elsewhere, or its run ended: nothing to do.
                    let _ = hedy.answer_permission(&question.task, &question.request_id, decision);
                }
            }
            Ok(Signal::Event(
                id,
                AgentEvent::PermissionRequest {
                    request_id,
                    tool,
                    summary,
                    input,
                },
            )) if ids.contains(&id) => {
                // A request a rule already answered is not asked.
                if hedy.is_permission_pending(&id, &request_id) {
                    if ask {
                        let mut what = shown(&summary);
                        if summary.trim().is_empty() || summary == tool {
                            let input: String =
                                input.to_string().chars().take(INPUT_SHOWN).collect();
                            what = format!("{} {}", shown(&tool), shown(&input));
                        }
                        let run = label(&id);
                        queue.push_back(Ask {
                            task: id,
                            request_id,
                            text: tr!(
                                "  {run} pide: {what} ¿Permitir? [s/N] ",
                                "  {run} asks: {what} Allow? [y/N] "
                            ),
                        });
                    } else {
                        let _ = hedy.answer_permission(&id, &request_id, Decision::Deny);
                    }
                }
            }
            Ok(Signal::Updated(id, status)) if ids.contains(&id) => {
                if matches!(status, TaskStatus::Review | TaskStatus::Failed) {
                    let run = label(&id);
                    if asking.take().is_some() {
                        println!();
                    }
                    println!(
                        "  {} {}",
                        s.muted("·"),
                        tr!("terminó {run}", "{run} finished")
                    );
                }
            }
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if hedy.trial_settled(&trial.id, &ids) {
            break;
        }
    }
    if asking.is_some() {
        println!();
    }
    match hedy.take_trial_outcome(&trial.id)? {
        Some(TrialOutcome::Judged(result)) => {
            println!("{} {}", s.muted("→"), verdict(&s, &result));
            println!("  {}", result.summary());
            println!(
                "  {}",
                s.muted(tr!(
                    "Con pocas tareas es una pista, no una prueba: puedes repetirla.",
                    "With a few tasks it is a hint, not a proof: you can run it again."
                ))
            );
        }
        Some(TrialOutcome::Void(reason)) => {
            println!("{} {}", s.warn("■"), reason.message());
        }
        None => println!(
            "{}",
            tr!(
                "La prueba terminó sin resultado.",
                "The trial ended without a result."
            )
        ),
    }
    Ok(())
}

/// A trial's verdict, in its color.
pub fn verdict(s: &Style, result: &TrialResult) -> String {
    match result.verdict {
        Verdict::Useful => s.ok(tr!("Útil", "Useful")),
        Verdict::Doubtful => s.warn(tr!("Dudosa", "Doubtful")),
        Verdict::Harmful => s.error(tr!("Contraproducente", "Harmful")),
        Verdict::Untested => s.muted(tr!("Sin probar", "Untested")),
    }
}
