//! `hedy memory ask`: a question answered only from the memory
//! (docs/decisions.md §36), each claim citing its note; the notes it cites
//! are listed under the answer, with where they live and who wrote them.

use std::path::Path;

use anyhow::Result;
use hedy_core::application::{AskScope, Hedy, MemoryAnswer};
use hedy_core::domain::ask::{Source, SourceLayer};
use hedy_core::tr;
use serde_json::json;

use crate::cli::MemoryAsk;
use crate::commands::age_label;
use crate::spinner;
use crate::style::Style;
use crate::sync::now;
use crate::team::{clean, clean_text, pick, safe_json};

pub fn memory_ask(hedy: &Hedy, dir: &Path, args: MemoryAsk) -> Result<()> {
    let question = args.question.join(" ");
    if args.code {
        return look_in_code(hedy, dir, &question, &args);
    }
    let scope = if args.all {
        AskScope::Mine
    } else if let Some(team) = args.team.as_deref() {
        AskScope::Team(pick(hedy, dir, Some(team))?)
    } else if let Some(project) = hedy.project_for(dir)? {
        AskScope::Project(project.path)
    } else {
        AskScope::Mine
    };
    let answer = spinner::while_running(
        &tr!("Buscando en tu memoria…", "Searching your memory…"),
        || {
            hedy.ask_memory(
                &question,
                &scope,
                args.account.as_deref(),
                args.model.as_deref(),
            )
        },
    )?;
    if args.json {
        println!("{}", safe_json(&as_json(&answer))?);
        return Ok(());
    }
    print_answer(&answer);
    if answer.text.is_none() && matches!(scope, AskScope::Project(_)) {
        let s = Style::for_stdout();
        println!(
            "  {}\n",
            s.muted(&tr!(
                "Búscalo en el código: hedy memory ask --code \"…\"",
                "Look for it in the code: hedy memory ask --code \"…\""
            ))
        );
    }
    Ok(())
}

/// `--code`: the project's files answer, and the note it proposes waits for
/// the user's approval.
fn look_in_code(hedy: &Hedy, dir: &Path, question: &str, args: &MemoryAsk) -> Result<()> {
    let Some(project) = hedy.project_for(dir)? else {
        anyhow::bail!(
            "{}",
            tr!(
                "esta carpeta no es un proyecto de Hedy; usa `hedy use <cuenta>` primero",
                "this folder is not a Hedy project; use `hedy use <account>` first"
            )
        );
    };
    let found = spinner::while_running(
        &tr!("Leyendo el proyecto…", "Reading the project…"),
        || {
            hedy.look_in_code(
                question,
                &project.path,
                args.account.as_deref(),
                args.model.as_deref(),
            )
        },
    )?;
    if args.json {
        let value = json!({
            "answer": found.text,
            "proposal": found.proposal.as_ref().map(|n| json!({ "id": n.id, "text": n.text })),
            "account": found.account.id.as_str(),
        });
        println!("{}", safe_json(&value)?);
        return Ok(());
    }
    let s = Style::for_stdout();
    println!();
    match &found.text {
        Some(text) => println!("  {}\n", clean_text(text)),
        None => println!(
            "  {}\n",
            tr!(
                "Tampoco el código del proyecto responde eso.",
                "The project's code does not answer that either."
            )
        ),
    }
    if let Some(note) = &found.proposal {
        let id = note.id;
        println!(
            "  {} {}",
            s.bold(&tr!("Propuesta #{id}:", "Proposal #{id}:")),
            clean_text(&note.text)
        );
        println!(
            "  {}\n",
            s.muted(&tr!(
                "Apruébala con `hedy memory approve {id}` para que tus IAs lo sepan.",
                "Approve it with `hedy memory approve {id}` so your AIs know it."
            ))
        );
    }
    Ok(())
}

fn as_json(answer: &MemoryAnswer) -> serde_json::Value {
    let sources: Vec<serde_json::Value> = answer
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut value = serde_json::to_value(s).unwrap_or_default();
            value["n"] = json!(i + 1);
            value["cited"] = json!(answer.cited.contains(&(i + 1)));
            value
        })
        .collect();
    json!({
        "answer": answer.text,
        "cited": answer.cited,
        "sources": sources,
        "account": answer.account.as_ref().map(|a| a.id.as_str()),
    })
}

fn print_answer(answer: &MemoryAnswer) {
    let s = Style::for_stdout();
    println!();
    let Some(text) = &answer.text else {
        println!(
            "  {}",
            tr!(
                "Nada escrito en tu memoria responde eso.",
                "Nothing written in your memory answers that."
            )
        );
        println!(
            "  {}",
            s.muted(&tr!(
                "Cuando lo sepas, anótalo: tus IAs también lo sabrán.",
                "When you know it, write it down: your AIs will know it too."
            ))
        );
        print_reader(&s, answer);
        return;
    };
    println!("  {}\n", clean_text(text));
    for n in &answer.cited {
        if let Some(source) = answer.sources.get(n - 1) {
            println!(
                "  {} {}",
                s.bold(format!("[{n}]")),
                clean_text(&source.text)
            );
            println!("      {}", s.muted(place(source)));
        }
    }
    print_reader(&s, answer);
}

/// Which account read how many notes (nothing, when none was sent).
fn print_reader(s: &Style, answer: &MemoryAnswer) {
    let Some(account) = &answer.account else {
        println!();
        return;
    };
    let name = clean(&account.name);
    let read = answer.sources.len();
    let line = if read == 1 {
        tr!(
            "{name} leyó 1 nota de tu memoria.",
            "{name} read 1 note of your memory."
        )
    } else {
        tr!(
            "{name} leyó {read} notas de tu memoria.",
            "{name} read {read} notes of your memory."
        )
    };
    println!("\n  {}\n", s.muted(&line));
}

/// Where a note lives and who wrote it, for the line under it.
pub(crate) fn place(source: &Source) -> String {
    let age = age_label(now() - source.at);
    let mut parts: Vec<String> = Vec::new();
    match source.layer {
        SourceLayer::Team => {
            let team = clean(source.team.as_deref().unwrap_or_default());
            parts.push(tr!(
                "Equipo {team} · capa del equipo",
                "Team {team} · team layer"
            ));
        }
        SourceLayer::Project => {
            let team = clean(source.team.as_deref().unwrap_or_default());
            match source.project.as_deref() {
                Some(project) => {
                    let project = clean(project);
                    parts.push(tr!(
                        "Equipo {team} · proyecto {project}",
                        "Team {team} · project {project}"
                    ));
                }
                None => parts.push(tr!(
                    "Equipo {team} · un proyecto que no está en esta PC",
                    "Team {team} · a project not on this PC"
                )),
            }
        }
        SourceLayer::Mine => match source.project.as_deref() {
            Some(project) => {
                let project = clean(project);
                parts.push(tr!(
                    "Tuya · proyecto {project}",
                    "Yours · project {project}"
                ));
            }
            None => parts.push(tr!("Tuya · todos los proyectos", "Yours · every project")),
        },
    }
    if let Some(author) = &source.author {
        parts.push(clean(author));
    }
    parts.push(age);
    parts.join(" · ")
}
