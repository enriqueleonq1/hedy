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
use crate::team::{clean, clean_text, pick};

pub fn memory_ask(hedy: &Hedy, dir: &Path, args: MemoryAsk) -> Result<()> {
    let question = args.question.join(" ");
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
        println!("{}", serde_json::to_string_pretty(&as_json(&answer))?);
        return Ok(());
    }
    print_answer(&answer);
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
            "  {}\n",
            s.muted(&tr!(
                "Cuando lo sepas, anótalo: tus IAs también lo sabrán.",
                "When you know it, write it down: your AIs will know it too."
            ))
        );
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
    let used = answer.sources.len();
    if let Some(account) = &answer.account {
        let name = clean(&account.name);
        println!(
            "\n  {}\n",
            s.muted(&tr!(
                "Respondió {name} con {used} notas de tu memoria.",
                "{name} answered from {used} notes of your memory."
            ))
        );
    }
}

/// Where a note lives and who wrote it, for the line under it.
fn place(source: &Source) -> String {
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
            let project = clean(source.project.as_deref().unwrap_or_default());
            parts.push(tr!(
                "Equipo {team} · proyecto {project}",
                "Team {team} · project {project}"
            ));
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
