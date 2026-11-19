//! `hedy commit`: a message for what is staged, suggested by the folder's
//! account, then committed with the user's own git. Never stages, never
//! pushes.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use anyhow::Result;
use hedy_core::application::{Hedy, StagedSuggestion};
use hedy_core::tr;

use crate::spinner;
use crate::style::Style;

/// What `hedy commit` was asked for.
#[derive(Debug, Clone, Copy, Default)]
pub struct CommitOptions<'a> {
    pub account: Option<&'a str>,
    pub model: Option<&'a str>,
    /// Only print the message (for `git commit -m "$(hedy commit --print)"`).
    pub print: bool,
    /// Commit without asking.
    pub yes: bool,
}

/// Notes on stderr, so stdout carries only the message.
fn print_notes(s: &Style, suggestion: &StagedSuggestion) {
    let a = &suggestion.account;
    eprintln!(
        "{} {} {}",
        s.chip(a.color),
        s.bold(&a.name),
        s.muted(tr!(
            "· sugerido con esta cuenta (gasta unos pocos tokens)",
            "· suggested with this account (spends a few tokens)"
        ))
    );
    if !suggestion.skipped.is_empty() {
        let list = suggestion
            .skipped
            .iter()
            .map(|p| crate::team::clean(p))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "{} {}",
            s.warn("⚠"),
            tr!(
                "No se envió el contenido de archivos privados (solo su nombre): {list}",
                "Private files' contents were not sent (only their names): {list}"
            )
        );
    }
    if suggestion.truncated {
        eprintln!(
            "{}",
            s.muted(tr!(
                "Los cambios eran muy largos: la IA vio solo el comienzo.",
                "The changes were very long: the AI only saw the start."
            ))
        );
    }
}

enum Choice {
    Confirm,
    Edit,
    Regenerate,
    Cancel,
}

fn ask(s: &Style) -> Choice {
    loop {
        eprint!(
            "{} ",
            s.muted(tr!(
                "[Enter] confirmar · e editar · r regenerar · q cancelar",
                "[Enter] confirm · e edit · r regenerate · q cancel"
            ))
        );
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => return Choice::Cancel,
            Ok(_) => {}
        }
        match line.trim().to_lowercase().as_str() {
            "" => return Choice::Confirm,
            "e" => return Choice::Edit,
            "r" => return Choice::Regenerate,
            "q" => return Choice::Cancel,
            _ => {}
        }
    }
}

fn committed(s: &Style) {
    eprintln!(
        "{} {}",
        s.ok("✔"),
        tr!(
            "Commit hecho con tu git. Hedy no hace push.",
            "Committed with your git. Hedy does not push."
        )
    );
}

pub fn commit(hedy: &Hedy, dir: &Path, options: CommitOptions) -> Result<()> {
    let s = Style::for_stderr();
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    loop {
        let suggestion = spinner::while_running(
            &tr!(
                "Escribiendo un mensaje para lo preparado…",
                "Writing a message for what you staged…"
            ),
            || hedy.suggest_staged_commit(dir, options.account, options.model),
        )?;
        print_notes(&s, &suggestion);
        let message = &suggestion.message;
        if options.yes {
            hedy.commit_staged(dir, message, false)?;
            committed(&s);
            return Ok(());
        }
        if options.print || !interactive {
            println!("{message}");
            return Ok(());
        }
        let out = Style::for_stdout();
        println!("\n  {}\n", out.bold(message));
        match ask(&s) {
            Choice::Confirm => {
                hedy.commit_staged(dir, message, false)?;
                committed(&s);
                return Ok(());
            }
            Choice::Edit => {
                hedy.commit_staged(dir, message, true)?;
                committed(&s);
                return Ok(());
            }
            Choice::Regenerate => continue,
            Choice::Cancel => {
                eprintln!(
                    "{}",
                    s.muted(tr!(
                        "Cancelado: no se hizo el commit.",
                        "Cancelled: nothing was committed."
                    ))
                );
                return Ok(());
            }
        }
    }
}
