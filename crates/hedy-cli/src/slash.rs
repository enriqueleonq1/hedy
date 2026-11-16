//! `hedy commands`: the slash commands (and skills) an account's CLI offers,
//! as it reported them the last time it answered in Hedy. Hedy never reads
//! the account folder for them (rule 1), so before a first answer there is
//! nothing to show.

use std::path::Path;

use anyhow::Result;
use hedy_core::application::Hedy;
use hedy_core::domain::command::reports_commands;
use hedy_core::domain::{Account, AgentCommand, CommandSource, KnownCommands};
use hedy_core::tr;

use crate::commands::age_label;
use crate::mascot::{self, Mood};
use crate::style::{Style, pad};

/// A description longer than this is cut, with "…".
const DESCRIPTION_WIDTH: usize = 72;
/// The column where descriptions start.
const NAME_WIDTH: usize = 28;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn commands(hedy: &Hedy, dir: &Path, account: Option<&str>, json: bool) -> Result<()> {
    let (account, known) = hedy.commands_in(dir, account)?;
    if json {
        let value = serde_json::json!({
            "account": account.id.as_str(),
            "name": account.name,
            "seenAt": known.as_ref().map(|k| k.seen_at),
            "commands": known.as_ref().map(|k| k.commands.clone()).unwrap_or_default(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    let s = Style::for_stdout();
    match known {
        Some(known) if !known.commands.is_empty() => print_list(&s, &account, &known),
        _ => print_none(&s, &account),
    }
    Ok(())
}

fn group_title(source: CommandSource) -> String {
    match source {
        CommandSource::Plugin => tr!("Plugins", "Plugins"),
        CommandSource::Skill => tr!("Habilidades", "Skills"),
        CommandSource::Custom => tr!("Tus comandos", "Your commands"),
        CommandSource::Builtin => tr!("Incluidos en la CLI", "Built into the CLI"),
        CommandSource::Unknown => tr!("Comandos", "Commands"),
    }
}

/// One line: up to `width` characters, with "…" when cut.
fn one_line(text: &str, width: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() <= width && !text.trim().contains('\n') {
        return line.to_owned();
    }
    let cut: String = line.chars().take(width.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

fn print_list(s: &Style, account: &Account, known: &KnownCommands) {
    let age = age_label(now_secs() - known.seen_at);
    let who = &account.name;
    println!();
    println!(
        "  {} {}  {}",
        s.chip(account.color),
        s.bold(tr!("Comandos de {who}", "{who} commands")),
        s.muted(tr!("vistos {age}", "seen {age}"))
    );
    let order = [
        CommandSource::Plugin,
        CommandSource::Skill,
        CommandSource::Custom,
        CommandSource::Builtin,
        CommandSource::Unknown,
    ];
    for source in order {
        let group: Vec<&AgentCommand> = known
            .commands
            .iter()
            .filter(|c| c.source == source)
            .collect();
        if group.is_empty() {
            continue;
        }
        println!("\n{}", s.section(&group_title(source)));
        for command in group {
            let name = match &command.argument_hint {
                Some(hint) => format!("/{} {}", command.name, hint),
                None => format!("/{}", command.name),
            };
            let shown = one_line(&name, NAME_WIDTH * 2);
            let width = shown.chars().count();
            let description = command
                .description
                .as_deref()
                .map(|d| one_line(d, DESCRIPTION_WIDTH))
                .unwrap_or_default();
            println!(
                "    {}  {}",
                pad(&s.command(&shown), width, NAME_WIDTH),
                s.muted(description)
            );
        }
    }
    println!();
    println!(
        "  {}",
        s.muted(tr!(
            "Escríbelos al empezar un mensaje: hedy ask \"/nombre …\" o en el chat de la app.",
            "Type them at the start of a message: hedy ask \"/name …\" or in the app's chat."
        ))
    );
}

fn print_none(s: &Style, account: &Account) {
    let name = s.bold(&account.name);
    let mut text = vec![];
    if reports_commands(account) {
        text.push(tr!(
            "Todavía no se conocen los comandos de {name}.",
            "{name}'s commands are not known yet."
        ));
        text.push(s.muted(tr!(
            "Aparecen después de su primera respuesta en Hedy (hedy ask o la app); Hedy no lee la carpeta de la cuenta.",
            "They show up after its first answer in Hedy (hedy ask or the app); Hedy does not read the account's folder."
        )));
    } else {
        text.push(tr!(
            "{name} no le pasa sus comandos a Hedy.",
            "{name} does not tell Hedy its commands."
        ));
        text.push(s.muted(tr!(
            "Codex y las claves de API no tienen comandos con «/» fuera de su propia terminal; usa hedy run.",
            "Codex and API keys have no \"/\" commands outside their own terminal; use hedy run."
        )));
    }
    mascot::print_beside(s, Mood::Sleepy, &text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_descriptions_are_cut_to_one_line() {
        assert_eq!(one_line("Short", 10), "Short");
        assert_eq!(one_line("A much longer text", 10), "A much lo…");
        assert_eq!(one_line("First\nsecond", 20), "First…");
    }
}
