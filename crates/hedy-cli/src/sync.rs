//! `hedy sync`: personal sync from the terminal (docs/decisions.md §34).
//! The same use cases as the app; this module only asks and prints.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use hedy_core::application::ports::SyncConflict;
use hedy_core::application::{Hedy, SyncReport};
use hedy_core::domain::sync::{MemoryHome, Version, fold_word, item_preview};
use hedy_core::tr;

use crate::commands::age_label;
use crate::style::Style;

/// What `hedy sync project` sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomeChoice {
    Personal,
    Local,
    Shared(String),
    /// Follow the repo's owner again.
    Follow,
}

/// What `hedy sync owner` records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerChoice {
    Mine,
    Shared,
    Forget,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn read_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

fn print_report(s: &Style, report: &SyncReport) {
    let SyncReport {
        sent,
        received,
        conflicts,
    } = report;
    println!(
        "{} {}",
        s.ok("✓"),
        tr!(
            "Sincronizado · {sent} enviados, {received} recibidos",
            "Synced · {sent} sent, {received} received"
        )
    );
    if *conflicts > 0 {
        println!(
            "{} {}",
            s.warn("!"),
            tr!(
                "{conflicts} conflicto(s) para revisar: `hedy sync conflicts`",
                "{conflicts} conflict(s) to look at: `hedy sync conflicts`"
            )
        );
    }
}

/// `hedy sync`: where it syncs, the PCs and the conflicts.
pub fn status(hedy: &Hedy) -> Result<()> {
    let s = Style::for_stdout();
    let Some(status) = hedy.sync_status()? else {
        println!(
            "{}",
            tr!(
                "La sincronización no está configurada en esta PC.",
                "Sync is not set up on this PC."
            )
        );
        println!(
            "{}",
            s.muted(tr!(
                "Crea un repo git privado y vacío, y luego `hedy sync setup <dirección>`; si ya sincronizas en otra PC, `hedy sync join <dirección>`.",
                "Create an empty private git repo, then `hedy sync setup <url>`; if another PC already syncs, `hedy sync join <url>`."
            ))
        );
        return Ok(());
    };
    let last = status
        .last_sync
        .map_or_else(|| tr!("nunca", "never"), |at| age_label(now() - at));
    println!(
        "\n  {} {}  {}",
        s.ok("●"),
        s.bold(tr!("Sincronización", "Sync")),
        s.muted(tr!("última: {last}", "last: {last}"))
    );
    println!("  {}", s.muted(&status.url));
    if let Some(error) = &status.last_error {
        println!(
            "  {} {}",
            s.warn("!"),
            tr!("la última falló: {error}", "the last one failed: {error}")
        );
    }
    println!("\n{}", s.section(&tr!("PCs", "PCs")));
    for pc in &status.devices {
        // Each PC signs its own name: shown without terminal controls.
        let name: String = pc.name.chars().filter(|c| !c.is_control()).collect();
        let name = if name.trim().is_empty() {
            pc.id.as_str().to_owned()
        } else {
            name
        };
        let mark = if pc.this_pc {
            s.muted(tr!(" · esta PC", " · this PC"))
        } else {
            String::new()
        };
        let seen = pc
            .last_sync
            .map_or_else(String::new, |at| age_label(now() - at));
        // The id tells apart two PCs with one name (it is the name when
        // a PC has none yet).
        let id = if name == pc.id.as_str() {
            ""
        } else {
            pc.id.as_str()
        };
        println!("  {name}{mark}  {}  {}", s.muted(seen), s.muted(id));
    }
    if status.conflicts > 0 {
        let n = status.conflicts;
        println!(
            "\n{} {}",
            s.warn("!"),
            tr!(
                "{n} conflicto(s): `hedy sync conflicts`",
                "{n} conflict(s): `hedy sync conflicts`"
            )
        );
    }
    println!();
    Ok(())
}

/// `hedy sync setup <url>`: shows the recovery words, and in a terminal asks
/// for one of them back, so they are not lost.
pub fn setup(hedy: &Hedy, url: &str, name: Option<&str>) -> Result<()> {
    let s = Style::for_stdout();
    let name = name.map_or_else(|| hedy.default_device_name(), str::to_owned);
    let words = hedy.sync_setup(url, &name)?;
    println!(
        "\n{} {}\n",
        s.ok("✓"),
        tr!("Sincronización configurada.", "Sync is set up.")
    );
    println!(
        "{}",
        s.bold(tr!(
            "Tu clave de recuperación (12 palabras):",
            "Your recovery key (12 words):"
        ))
    );
    let list: Vec<&str> = words.split_whitespace().collect();
    for (i, chunk) in list.chunks(4).enumerate() {
        let line: Vec<String> = chunk
            .iter()
            .enumerate()
            .map(|(j, w)| format!("{:>2}. {:<10}", i * 4 + j + 1, w))
            .collect();
        println!("  {}", line.join(" "));
    }
    println!(
        "\n{}",
        s.warn(tr!(
            "Guárdala en tu gestor de contraseñas. La necesitas para sumar otra PC, y nadie puede recuperarla por ti, ni Hedy.",
            "Keep it in your password manager. You need it to add another PC, and nobody can recover it for you, not even Hedy."
        ))
    );
    if interactive() {
        let n = 7;
        loop {
            let typed = read_line(&tr!(
                "Para confirmar, escribe la palabra n.º {n}: ",
                "To confirm, type word #{n}: "
            ))?;
            if fold_word(&typed) == fold_word(list[n - 1]) {
                break;
            }
            println!(
                "{}",
                s.warn(tr!(
                    "No coincide; revisa la lista de arriba.",
                    "That does not match; check the list above."
                ))
            );
        }
        println!("{} {}", s.ok("✓"), tr!("Listo.", "Done."));
    }
    Ok(())
}

/// `hedy sync join <url>`: asks for the 12 words (or reads them from stdin).
pub fn join(hedy: &Hedy, url: &str, name: Option<&str>) -> Result<()> {
    let s = Style::for_stdout();
    let name = name.map_or_else(|| hedy.default_device_name(), str::to_owned);
    // Typed in a terminal, the words never echo (even with stdout piped).
    let words = if std::io::stdin().is_terminal() {
        rpassword::prompt_password(tr!(
            "Tu clave de recuperación (12 palabras, no se muestra): ",
            "Your recovery key (12 words, hidden): "
        ))?
    } else {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        line
    };
    if words.trim().is_empty() {
        bail!(
            "{}",
            tr!("faltan las 12 palabras", "the 12 words are missing")
        );
    }
    let report = hedy.sync_join(url, &words, &name)?;
    println!(
        "{} {}",
        s.ok("✓"),
        tr!(
            "Esta PC ya sincroniza ({name}).",
            "This PC syncs now ({name})."
        )
    );
    print_report(&s, &report);
    Ok(())
}

pub fn now_sync(hedy: &Hedy) -> Result<()> {
    let report = hedy.sync_now()?;
    print_report(&Style::for_stdout(), &report);
    Ok(())
}

fn conflict_line(conflict: &SyncConflict) -> String {
    let preview = |version: &Version| -> String {
        match item_preview(conflict.kind, version.body.as_deref()) {
            Some(text) => {
                let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                format!("«{}»", text.chars().take(70).collect::<String>())
            }
            None => tr!("(borrada)", "(deleted)"),
        }
    };
    format!("{} → {}", preview(&conflict.kept), preview(&conflict.other))
}

pub fn conflicts(hedy: &Hedy) -> Result<()> {
    let s = Style::for_stdout();
    let conflicts = hedy.sync_conflicts()?;
    if conflicts.is_empty() {
        println!("{}", tr!("No hay conflictos.", "There are no conflicts."));
        return Ok(());
    }
    println!(
        "\n{}",
        s.section(&tr!(
            "Conflictos · en uso → la otra versión",
            "Conflicts · in use → the other version"
        ))
    );
    for conflict in &conflicts {
        println!("  {}  {}", s.bold(conflict.id), conflict_line(conflict));
    }
    println!(
        "\n{}",
        s.muted(tr!(
            "Quédate con la que está en uso: `hedy sync resolve <id> --keep`; usa la otra: `hedy sync resolve <id> --other`.",
            "Keep the one in use: `hedy sync resolve <id> --keep`; use the other: `hedy sync resolve <id> --other`."
        ))
    );
    Ok(())
}

pub fn resolve(hedy: &Hedy, id: i64, use_other: bool) -> Result<()> {
    hedy.resolve_sync_conflict(id, use_other)?;
    println!(
        "{} {}",
        Style::for_stdout().ok("✓"),
        tr!(
            "Resuelto; se envía con la próxima sincronización.",
            "Resolved; it goes out with the next sync."
        )
    );
    Ok(())
}

/// `hedy sync owner`: the answers about repo owners, or one answer.
pub fn owner(hedy: &Hedy, owner: Option<&str>, choice: Option<OwnerChoice>) -> Result<()> {
    let s = Style::for_stdout();
    if let Some(owner) = owner
        && !owner.contains('/')
    {
        bail!(
            "{}",
            tr!(
                "«{owner}» no dice en qué servidor está; escríbelo como servidor/dueño, p. ej. github.com/{owner}",
                "\"{owner}\" does not say its server; write it as server/owner, e.g. github.com/{owner}"
            )
        );
    }
    match (owner, choice) {
        (Some(owner), Some(OwnerChoice::Mine)) => hedy.answer_owner(owner, true)?,
        (Some(owner), Some(OwnerChoice::Shared)) => hedy.answer_owner(owner, false)?,
        (Some(owner), Some(OwnerChoice::Forget)) => hedy.forget_owner(owner)?,
        _ => {}
    }
    let owners = hedy.repo_owners()?;
    if owners.mine.is_empty() && owners.shared.is_empty() {
        println!(
            "{}",
            tr!(
                "Todavía no respondiste por ningún dueño de repos.",
                "You have not answered about any repo owner yet."
            )
        );
        return Ok(());
    }
    for (title, list) in [
        (
            tr!("Tuyos · se sincronizan", "Yours · synced"),
            &owners.mine,
        ),
        (
            tr!(
                "Compartidos · no salen de esta PC",
                "Shared · stay on this PC"
            ),
            &owners.shared,
        ),
    ] {
        if !list.is_empty() {
            println!("\n{}", s.section(&title));
            for owner in list {
                println!("  {owner}");
            }
        }
    }
    println!(
        "\n{}",
        s.muted(tr!(
            "Las respuestas valen solo en esta PC.",
            "Answers apply on this PC only."
        ))
    );
    Ok(())
}

fn home_label(home: Option<&MemoryHome>) -> String {
    match home {
        Some(MemoryHome::Personal) => tr!("Personal · se sincroniza", "Personal · synced"),
        Some(MemoryHome::Shared(owner)) => tr!(
            "Compartida · {owner} · no sale de esta PC",
            "Shared · {owner} · stays on this PC"
        ),
        Some(MemoryHome::Local) | None => tr!("Solo en esta PC", "On this PC only"),
    }
}

/// `hedy sync project`: where the current project's memory lives, or a new
/// choice for it.
pub fn project(hedy: &Hedy, dir: &Path, choice: Option<HomeChoice>) -> Result<()> {
    let s = Style::for_stdout();
    let Some(project) = hedy.project_for(dir)? else {
        bail!(
            "{}",
            tr!(
                "esta carpeta no es un proyecto de Hedy; usa `hedy use <cuenta>` primero",
                "this folder is not a Hedy project; use `hedy use <account>` first"
            )
        );
    };
    if let Some(choice) = choice {
        let home = match choice {
            HomeChoice::Personal => Some(MemoryHome::Personal),
            HomeChoice::Local => Some(MemoryHome::Local),
            HomeChoice::Shared(owner) => Some(MemoryHome::Shared(owner)),
            HomeChoice::Follow => None,
        };
        hedy.set_memory_home(&project.path, home.as_ref())?;
    }
    let status = hedy.memory_home_status(&project.path)?;
    let name = s.bold(&project.name);
    println!(
        "{}: {}",
        tr!("Memoria de {name}", "{name} memory"),
        home_label(status.home.as_ref())
    );
    if let Some(owner) = &status.ask {
        println!(
            "{} {}",
            s.warn("?"),
            tr!(
                "¿La memoria de {owner} es solo tuya? `hedy sync owner {owner} --mine` o `--shared`",
                "Is {owner}'s memory yours alone? `hedy sync owner {owner} --mine` or `--shared`"
            )
        );
    } else if !status.chosen {
        println!(
            "{}",
            s.muted(tr!(
                "Según el dueño de su repo; cámbialo con `hedy sync project --personal|--local`.",
                "From its repo's owner; change it with `hedy sync project --personal|--local`."
            ))
        );
    }
    Ok(())
}

/// `hedy sync remove <pc>`: asks first in a terminal (`--yes` skips it).
pub fn remove(hedy: &Hedy, who: &str, yes: bool) -> Result<()> {
    if !yes {
        if !interactive() {
            bail!(
                "{}",
                tr!(
                    "para quitarla sin preguntar, usa `hedy sync remove {who} --yes`",
                    "to remove it without asking, use `hedy sync remove {who} --yes`"
                )
            );
        }
        let answer = read_line(&tr!(
            "¿Quitar {who}? No podrá leer lo que guardes desde ahora; lo que ya tiene sigue en esa PC. Si la perdiste o te la robaron, quita también su acceso a git. [s/N] ",
            "Remove {who}? It cannot read what you keep from now on; what it has stays on that PC. If it was lost or stolen, also revoke its git access. [y/N] "
        ))?;
        if !matches!(
            answer.to_lowercase().as_str(),
            "s" | "si" | "sí" | "y" | "yes"
        ) {
            return Ok(());
        }
    }
    let report = hedy.sync_remove_device(who)?;
    println!(
        "{} {}",
        Style::for_stdout().ok("✓"),
        tr!(
            "{who} ya no puede leer lo nuevo. Tus otras PCs siguen igual.",
            "{who} can no longer read what is new. Your other PCs carry on."
        )
    );
    print_report(&Style::for_stdout(), &report);
    Ok(())
}

/// `hedy sync leave`: asks first in a terminal (`--yes` skips it).
pub fn leave(hedy: &Hedy, yes: bool) -> Result<()> {
    if !yes {
        if !interactive() {
            bail!(
                "{}",
                tr!(
                    "para dejar de sincronizar sin preguntar, usa `hedy sync leave --yes`",
                    "to stop syncing without asking, use `hedy sync leave --yes`"
                )
            );
        }
        let answer = read_line(&tr!(
            "¿Dejar de sincronizar en esta PC? Para volver necesitarás tus 12 palabras. [s/N] ",
            "Stop syncing on this PC? To come back you will need your 12 words. [y/N] "
        ))?;
        if !matches!(
            answer.to_lowercase().as_str(),
            "s" | "si" | "sí" | "y" | "yes"
        ) {
            return Ok(());
        }
    }
    let s = Style::for_stdout();
    if !hedy.sync_leave()? {
        println!(
            "{}",
            tr!(
                "La sincronización no estaba configurada en esta PC; no queda nada de ella.",
                "Sync was not set up on this PC; nothing of it is left."
            )
        );
        return Ok(());
    }
    println!(
        "{} {}",
        s.ok("✓"),
        tr!(
            "Esta PC dejó de sincronizar. Tus notas siguen aquí, y tus otras PCs siguen igual.",
            "This PC stopped syncing. Your notes stay here, and your other PCs carry on."
        )
    );
    println!(
        "{}",
        s.muted(tr!(
            "Tus otras PCs aún la ven en la lista: quítala desde una de ellas (`hedy sync remove`).",
            "Your other PCs still list it: remove it from one of them (`hedy sync remove`)."
        ))
    );
    Ok(())
}
