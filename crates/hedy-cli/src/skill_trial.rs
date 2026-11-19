//! `hedy skills try <name>`: a skill tried on real tasks, each run twice in
//! throwaway copies (with and without it), judged by what the runs did. In a
//! terminal, each run's permission requests are asked here; elsewhere they
//! are declined.

use std::path::Path;
use std::sync::{Arc, mpsc};

use anyhow::{Context, Result};
use hedy_core::application::Hedy;
use hedy_core::application::ports::TaskEvents;
use hedy_core::domain::skill_check::Verdict;
use hedy_core::domain::skill_trial::TrialResult;
use hedy_core::domain::{AgentEvent, Decision, Task, TaskId, TaskStatus};
use hedy_core::tr;

use crate::commands::{project_here, refuse_in_agent_session};
use crate::skill_transfer::Reviewer;
use crate::spinner;
use crate::style::Style;
use crate::sync::{confirmed, interactive, read_line};
use crate::team::clean;

enum Signal {
    Event(TaskId, AgentEvent),
    Updated(TaskId, TaskStatus),
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

pub fn try_skill(
    hedy: &Hedy,
    dir: &Path,
    name: &str,
    tasks: &[String],
    yes: bool,
    reviewer: &Reviewer,
) -> Result<()> {
    refuse_in_agent_session()?;
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let skill = hedy.skill_named(&project.path, name)?;
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
    let skill_name = clean(&skill.name);
    println!();
    println!(
        "  {}",
        s.bold(tr!("Probar {skill_name}", "Try {skill_name}"))
    );
    println!(
        "  {}",
        s.muted(tr!(
            "Cada tarea dos veces, en copias aparte: con la habilidad y sin ella. Nada toca tu proyecto.",
            "Each task twice, in separate copies: with the skill and without it. Nothing touches your project."
        ))
    );
    for task in &tasks {
        println!("    · {}", clean(task));
    }
    let runs = tasks.len() * 2;
    let question = tr!(
        "¿Empezar? Son {runs} tareas de verdad y gastan tokens de la cuenta del proyecto. [s/N] ",
        "Start? These are {runs} real tasks and they spend the project account's tokens. [y/N] "
    );
    let again = format!("hedy skills try {} --yes", skill.name);
    if !confirmed(yes, &question, &again)? {
        println!("{}", tr!("No se probó nada.", "Nothing was tried."));
        return Ok(());
    }

    let (tx, rx) = mpsc::channel();
    let hedy = hedy.clone().with_task_events(Arc::new(Forward(tx.clone())));
    ctrlc::set_handler(move || {
        let _ = tx.send(Signal::Interrupt);
    })
    .context("Ctrl+C")?;
    let started = hedy.start_skill_trial(&project.path, skill.id, &tasks, reviewer.model)?;
    let ids: Vec<TaskId> = started.iter().map(|t| t.id.clone()).collect();
    // Paths inside a run's copy, shown from the copy's root.
    let roots: Vec<String> = started
        .iter()
        .filter_map(|t| t.workspace.as_ref())
        .map(|ws| format!("{}/", ws.worktree.display()))
        .collect();
    let short = |summary: &str| {
        roots
            .iter()
            .fold(summary.to_owned(), |text, root| text.replace(root, ""))
    };
    let label = |id: &TaskId| {
        let at = ids.iter().position(|i| i == id).unwrap_or(0);
        let side = match at % 2 {
            0 => tr!("con", "with"),
            _ => tr!("sin", "without"),
        };
        format!("{} {}", at / 2 + 1, side)
    };
    let ask = interactive();
    let mut left = ids.len();
    while left > 0 {
        let Ok(signal) = rx.recv() else { break };
        match signal {
            Signal::Interrupt => {
                hedy.cancel_skill_trial(&ids);
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
            Signal::Updated(id, status) if ids.contains(&id) => {
                if status == TaskStatus::Discarded {
                    left -= 1;
                } else if matches!(status, TaskStatus::Review | TaskStatus::Failed) {
                    let run = label(&id);
                    println!(
                        "  {} {}",
                        s.muted("·"),
                        tr!("terminó {run}", "{run} finished")
                    );
                }
            }
            Signal::Event(
                id,
                AgentEvent::PermissionRequest {
                    request_id,
                    summary,
                    ..
                },
            ) if ids.contains(&id) => {
                let allow = ask && {
                    let run = label(&id);
                    let what = clean(&short(&summary));
                    let answer = read_line(&tr!(
                        "  {run} pide: {what} ¿Permitir? [s/N] ",
                        "  {run} asks: {what} Allow? [y/N] "
                    ))?;
                    matches!(
                        answer.to_lowercase().as_str(),
                        "s" | "si" | "sí" | "y" | "yes"
                    )
                };
                let decision = if allow {
                    Decision::Allow
                } else {
                    Decision::Deny
                };
                hedy.answer_permission(&id, &request_id, decision)?;
            }
            _ => {}
        }
    }
    match hedy.skill_trial(skill.id)? {
        Some(result) => {
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
