//! `hedy team`: team memory from the terminal (docs/decisions.md §35). The
//! same use cases as the app; this module only asks and prints.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Result, bail};
use hedy_core::application::{Hedy, TeamPerson, TeamStatus, TeamSuggestion, TeamWriteOutcome};
use hedy_core::domain::Category;
use hedy_core::domain::sync::{DeviceId, MemoryHome, fold_word};
use hedy_core::domain::team::{PersonId, Role};
use hedy_core::tr;

use crate::commands::age_label;
use crate::style::Style;
use crate::sync::{confirmed, interactive, now, print_report, read_line};

/// The team a command is about: the one asked for (`--team`), else the
/// current project's, else the only one.
pub fn pick(hedy: &Hedy, dir: &Path, team: Option<&str>) -> Result<String> {
    if let Some(team) = team {
        return Ok(team.to_owned());
    }
    if let Ok(Some(project)) = hedy.project_for(dir)
        && let Ok(status) = hedy.memory_home_status(&project.path)
        && let Some(MemoryHome::Team(id)) = status.home
    {
        return Ok(id);
    }
    let teams = hedy.teams()?;
    match teams.as_slice() {
        [only] => Ok(only.id.clone()),
        [] => bail!(
            "{}",
            tr!(
                "esta PC no está en ningún equipo: `hedy team create` o `hedy team join`",
                "this PC is in no team: `hedy team create` or `hedy team join`"
            )
        ),
        _ => {
            let names: Vec<String> = teams
                .iter()
                .map(|t| {
                    if t.name.is_empty() {
                        t.id.clone()
                    } else {
                        t.name.clone()
                    }
                })
                .collect();
            let names = names.join(", ");
            bail!(
                "{}",
                tr!(
                    "esta PC está en varios equipos ({names}): elige uno con --team <nombre>",
                    "this PC is in several teams ({names}): pick one with --team <name>"
                )
            )
        }
    }
}

fn role_label(role: Role) -> String {
    match role {
        Role::Admin => tr!("admin", "admin"),
        Role::Member => tr!("miembro", "member"),
    }
}

/// `hedy team`: the team's people, invites and what waits for review; with
/// several teams and none chosen, the list of teams.
pub fn status(hedy: &Hedy, dir: &Path, team: Option<&str>) -> Result<()> {
    let s = Style::for_stdout();
    let teams = hedy.teams()?;
    if teams.is_empty() {
        println!(
            "{}",
            tr!(
                "Esta PC no está en ningún equipo.",
                "This PC is in no team."
            )
        );
        println!(
            "{}",
            s.muted(tr!(
                "Crea un repo git privado y vacío para el equipo y luego `hedy team create <dirección> --name <equipo>`; si te invitaron, `hedy team join <dirección>`.",
                "Create an empty private git repo for the team, then `hedy team create <url> --name <team>`; if you were invited, `hedy team join <url>`."
            ))
        );
        return Ok(());
    }
    let id = match pick(hedy, dir, team) {
        Ok(id) => id,
        Err(_) => {
            println!("\n{}", s.section(&tr!("Equipos", "Teams")));
            for t in &teams {
                let role = t.role.map(role_label).unwrap_or_default();
                println!("  {}  {}", s.bold(&t.name), s.muted(role));
            }
            println!(
                "\n{}",
                s.muted(tr!(
                    "Elige uno con --team <nombre>.",
                    "Pick one with --team <name>."
                ))
            );
            return Ok(());
        }
    };
    if let Some(waiting) = teams.iter().find(|t| t.id == id && t.waiting) {
        let url = &waiting.url;
        println!(
            "{} {}",
            s.warn("…"),
            tr!(
                "Esperando a que el equipo te deje entrar ({url}). `hedy team now` vuelve a mirar.",
                "Waiting for the team to let you in ({url}). `hedy team now` looks again."
            )
        );
        return Ok(());
    }
    print_status(&s, &hedy.team_status(&id)?);
    Ok(())
}

fn print_status(s: &Style, status: &TeamStatus) {
    let t = &status.summary;
    let last = t
        .last_sync
        .map_or_else(|| tr!("nunca", "never"), |at| age_label(now() - at));
    let role = t.role.map(role_label).unwrap_or_default();
    println!(
        "\n  {} {}  {}  {}",
        s.ok("●"),
        s.bold(&t.name),
        s.muted(role),
        s.muted(tr!("última: {last}", "last: {last}"))
    );
    println!("  {}", s.muted(&t.url));
    if t.leaving {
        println!(
            "  {} {}",
            s.warn("…"),
            tr!(
                "saliendo: esperando a que otro admin te saque",
                "leaving: waiting for another admin to take you out"
            )
        );
    }
    if let Some(error) = &t.last_error {
        println!(
            "  {} {}",
            s.warn("!"),
            tr!("la última falló: {error}", "the last one failed: {error}")
        );
    }
    println!("\n{}", s.section(&tr!("Personas", "People")));
    for person in &status.people {
        print_person(s, person);
    }
    if !status.invites.is_empty() {
        println!("\n{}", s.section(&tr!("Invitaciones", "Invites")));
        for invite in &status.invites {
            let left = ((invite.expires_at - now()) / 3600).max(0);
            let role = role_label(invite.role);
            let who = if invite.own_pc {
                tr!("otra PC tuya", "another PC of yours")
            } else {
                tr!("persona nueva, {role}", "new person, {role}")
            };
            let state = if invite.answered {
                tr!(
                    "respondida: entra con la próxima sincronización",
                    "answered: it gets in on the next sync"
                )
            } else {
                tr!("vence en {left} h", "expires in {left} h")
            };
            println!(
                "  {}  {}  {}",
                s.bold(&invite.code),
                s.muted(who),
                s.muted(state)
            );
        }
    }
    for number in &status.failed_invites {
        println!(
            "\n{} {}",
            s.warn("!"),
            tr!(
                "La invitación {number} no funcionó: alguien escribió un código que no coincidía, y ya no sirve. Crea otra.",
                "Invite {number} did not work: someone typed a code that did not match, and it is no longer valid. Make another."
            )
        );
    }
    if status.suggestions > 0 {
        let n = status.suggestions;
        println!(
            "\n{} {}",
            s.warn("!"),
            tr!(
                "{n} sugerencia(s): `hedy team review`",
                "{n} suggestion(s): `hedy team review`"
            )
        );
    }
    println!();
}

fn print_person(s: &Style, person: &TeamPerson) {
    let me = if person.me {
        s.muted(tr!(" · tú", " · you"))
    } else {
        String::new()
    };
    let pcs: Vec<String> = person
        .devices
        .iter()
        .map(|pc| {
            let name = if pc.name.trim().is_empty() {
                pc.id.as_str().to_owned()
            } else {
                pc.name.clone()
            };
            match pc.this_pc {
                true => format!("{name} {}", tr!("(esta PC)", "(this PC)")),
                false => name,
            }
        })
        .collect();
    println!(
        "  {}{me}  {}  {}",
        s.bold(&person.name),
        s.muted(role_label(person.role)),
        s.muted(pcs.join(", "))
    );
}

/// Shows the team's 12 words and, in a terminal, asks for one back.
fn show_words(s: &Style, words: &str) -> Result<()> {
    println!(
        "{}",
        s.bold(tr!(
            "La clave de recuperación del equipo (12 palabras):",
            "The team's recovery key (12 words):"
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
            "Guárdala en el gestor de contraseñas del equipo. Sin ella, un equipo cuyos admins pierden sus PCs queda cerrado para siempre.",
            "Keep it in the team's password manager. Without it, a team whose admins lose their PCs is locked forever."
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
    }
    Ok(())
}

/// The name a person goes by: the one given, else asked, else the user's.
fn my_name(given: Option<&str>) -> Result<String> {
    if let Some(name) = given.map(str::trim).filter(|n| !n.is_empty()) {
        return Ok(name.to_owned());
    }
    if interactive() {
        let name = read_line(&tr!("Tu nombre en el equipo: ", "Your name in the team: "))?;
        if !name.trim().is_empty() {
            return Ok(name);
        }
    }
    bail!(
        "{}",
        tr!(
            "falta tu nombre en el equipo: usa --me <nombre>",
            "your name in the team is missing: use --me <name>"
        )
    )
}

/// `hedy team create <url> --name <team>`.
pub fn create(
    hedy: &Hedy,
    url: &str,
    team_name: &str,
    me: Option<&str>,
    pc: Option<&str>,
) -> Result<()> {
    let s = Style::for_stdout();
    let me = my_name(me)?;
    let pc = pc.map_or_else(|| hedy.default_device_name(), str::to_owned);
    let (_, words) = hedy.team_create(url, team_name, &me, &pc)?;
    println!(
        "\n{} {}\n",
        s.ok("✓"),
        tr!(
            "Equipo «{team_name}» creado; eres su admin.",
            "Team \"{team_name}\" created; you are its admin."
        )
    );
    show_words(&s, &words)?;
    println!(
        "\n{}",
        s.muted(tr!(
            "Conecta un proyecto con `hedy team link` (en su carpeta) e invita con `hedy team invite`.",
            "Connect a project with `hedy team link` (in its folder) and invite with `hedy team invite`."
        ))
    );
    Ok(())
}

/// `hedy team invite`: a code that works once, for 48 hours.
pub fn invite(hedy: &Hedy, team: &str, admin: bool, mine: bool) -> Result<()> {
    let s = Style::for_stdout();
    let invite = if mine {
        hedy.team_invite_pc(team)?
    } else {
        let role = if admin { Role::Admin } else { Role::Member };
        hedy.team_invite(team, role)?
    };
    let url = hedy.team_status(team)?.summary.url;
    println!(
        "\n{} {}\n",
        s.ok("✓"),
        tr!(
            "Código de invitación (sirve una vez, por 48 horas):",
            "Invite code (it works once, for 48 hours):"
        )
    );
    println!("  {}\n", s.bold(&invite.code));
    println!(
        "{}",
        tr!(
            "Quien entra escribe: hedy team join {url}",
            "Whoever joins types: hedy team join {url}"
        )
    );
    println!(
        "{}",
        s.muted(tr!(
            "Pásale el código por otro canal que la dirección. Dale también acceso al repo en tu servidor git: Hedy no puede.",
            "Send the code through another channel than the address. Also give them access to the repo on your git server: Hedy cannot."
        ))
    );
    Ok(())
}

/// `hedy team invite --cancel <n>`.
pub fn cancel_invite(hedy: &Hedy, team: &str, number: u32) -> Result<()> {
    hedy.team_cancel_invite(team, number)?;
    println!(
        "{} {}",
        Style::for_stdout().ok("✓"),
        tr!(
            "Invitación {number} cancelada.",
            "Invite {number} cancelled."
        )
    );
    Ok(())
}

/// `hedy team join <url> [code]`: the code is asked when missing (not
/// echoed in a terminal).
pub fn join(
    hedy: &Hedy,
    url: &str,
    code: Option<&str>,
    me: Option<&str>,
    pc: Option<&str>,
) -> Result<()> {
    let s = Style::for_stdout();
    let code = match code {
        Some(code) => code.to_owned(),
        None if std::io::stdin().is_terminal() => rpassword::prompt_password(tr!(
            "Código de invitación (no se muestra): ",
            "Invite code (hidden): "
        ))?,
        None => read_line("")?,
    };
    let me = my_name(me)?;
    let pc = pc.map_or_else(|| hedy.default_device_name(), str::to_owned);
    let join = hedy.team_join(url, &code, &me, &pc)?;
    if join.joined {
        println!(
            "{} {}",
            s.ok("✓"),
            tr!("Ya estás en el equipo.", "You are in the team.")
        );
    } else {
        println!(
            "{} {}",
            s.ok("✓"),
            tr!(
                "Pedido enviado. Entras cuando la PC que te invitó sincronice; `hedy team now` vuelve a mirar.",
                "Request sent. You get in once the PC that invited you syncs; `hedy team now` looks again."
            )
        );
    }
    Ok(())
}

/// `hedy team now`: one sync of the team (or a look, while joining).
pub fn now_sync(hedy: &Hedy, team: &str) -> Result<()> {
    let s = Style::for_stdout();
    let waiting = hedy.teams()?.iter().any(|t| t.id == team && t.waiting);
    let report = hedy.team_sync(team)?;
    let still = hedy.teams()?.iter().any(|t| t.id == team && t.waiting);
    match (waiting, still) {
        (true, true) => println!(
            "{} {}",
            s.warn("…"),
            tr!("Todavía esperando a entrar.", "Still waiting to get in.")
        ),
        (true, false) => println!(
            "{} {}",
            s.ok("✓"),
            tr!("Ya estás en el equipo.", "You are in the team.")
        ),
        _ => print_report(&s, &report),
    }
    if report.refused > 0 {
        let n = report.refused;
        println!(
            "{} {}",
            s.warn("!"),
            tr!(
                "{n} cambio(s) de otras PCs rechazados: alguien escribió lo que su rol no permite.",
                "{n} change(s) from other PCs refused: someone wrote what their role does not allow."
            )
        );
    }
    Ok(())
}

/// `hedy team link`: the current project's memory becomes the team's.
pub fn link(hedy: &Hedy, dir: &Path, team: &str) -> Result<()> {
    let Some(project) = hedy.project_for(dir)? else {
        bail!(
            "{}",
            tr!(
                "esta carpeta no es un proyecto de Hedy; usa `hedy use <cuenta>` primero",
                "this folder is not a Hedy project; use `hedy use <account>` first"
            )
        );
    };
    hedy.team_link_project(team, &project.path)?;
    let name = &project.name;
    println!(
        "{} {}",
        Style::for_stdout().ok("✓"),
        tr!(
            "La memoria de {name} ahora es del equipo: sus agentes leen la capa Equipo, y sus notas de equipo van al repo del equipo (nunca a tu sincronización personal).",
            "{name}'s memory is now the team's: its agents read the team layer, and its team notes go to the team's repo (never to your personal sync)."
        )
    );
    Ok(())
}

/// `hedy team notes`: the team layer first, then each project's.
pub fn notes(hedy: &Hedy, team: &str) -> Result<()> {
    let s = Style::for_stdout();
    let notes = hedy.team_notes(team)?;
    if notes.is_empty() {
        println!(
            "{}",
            tr!(
                "El equipo no tiene notas todavía.",
                "The team has no notes yet."
            )
        );
        return Ok(());
    }
    let mut layer = None;
    for note in &notes {
        if layer != Some(note.team_layer) {
            layer = Some(note.team_layer);
            let title = if note.team_layer {
                tr!(
                    "Capa Equipo (siempre en el contexto)",
                    "Team layer (always in context)"
                )
            } else {
                tr!("Proyectos", "Projects")
            };
            println!("\n{}", s.section(&title));
        }
        let project = note
            .project
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| format!(" · {}", n.to_string_lossy()))
            .unwrap_or_default();
        println!(
            "  {} {}",
            note.text,
            s.muted(format!(
                "— {}{project} · {}",
                note.author,
                note.category.as_str()
            ))
        );
    }
    println!();
    Ok(())
}

fn parse_category(category: &str) -> Result<Category> {
    category.parse::<Category>().map_err(|_| {
        let valid: Vec<&str> = Category::ALL.iter().map(|c| c.as_str()).collect();
        let valid = valid.join(", ");
        anyhow::anyhow!(
            "{}",
            tr!(
                "categoría desconocida: usa una de {valid}",
                "unknown category: use one of {valid}"
            )
        )
    })
}

fn print_outcome(outcome: &TeamWriteOutcome) {
    let s = Style::for_stdout();
    match outcome {
        TeamWriteOutcome::Published(_) => println!(
            "{} {}",
            s.ok("✓"),
            tr!(
                "Publicada con tu nombre; sale con la próxima sincronización.",
                "Published under your name; it goes out with the next sync."
            )
        ),
        TeamWriteOutcome::Suggested(_) => println!(
            "{} {}",
            s.ok("✓"),
            tr!(
                "Enviada como sugerencia: quien puede cambiarla la revisa.",
                "Sent as a suggestion: whoever may change it reviews it."
            )
        ),
    }
}

/// `hedy team add <text>`: a note of the current project, or (`--everyone`)
/// of the team layer.
pub fn add(
    hedy: &Hedy,
    dir: &Path,
    team: &str,
    text: &str,
    everyone: bool,
    category: &str,
) -> Result<()> {
    let category = parse_category(category)?;
    let project = match everyone {
        true => None,
        false => Some(hedy.project_for(dir)?.map(|p| p.path).ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                tr!(
                    "esta carpeta no es un proyecto de Hedy; para la capa Equipo usa --everyone",
                    "this folder is not a Hedy project; for the team layer use --everyone"
                )
            )
        })?),
    };
    let outcome = hedy.team_add_note(team, project.as_deref(), text, category, None)?;
    print_outcome(&outcome);
    Ok(())
}

/// `hedy team publish <id>`: moves a note of this PC (`hedy memory` lists
/// them) to the team.
pub fn publish(hedy: &Hedy, team: &str, id: i64, everyone: bool) -> Result<()> {
    let outcome = hedy.team_publish_note(team, id, everyone)?;
    print_outcome(&outcome);
    Ok(())
}

/// A suggestion's short name: the end of its id.
fn short(uid: &str) -> &str {
    &uid[uid.len().saturating_sub(6)..]
}

fn one_line(text: &str) -> String {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = text.chars().take(90).collect();
    if cut.chars().count() < text.chars().count() {
        format!("«{cut}…»")
    } else {
        format!("«{cut}»")
    }
}

fn print_suggestion(s: &Style, suggestion: &TeamSuggestion) {
    let after = match &suggestion.shown {
        Some(text) => one_line(text),
        None => tr!("(quitar la nota)", "(remove the note)"),
    };
    let what = match &suggestion.before {
        Some(before) => format!("{} → {after}", one_line(before)),
        None => tr!("nueva: {after}", "new: {after}"),
    };
    let layer = if suggestion.team_layer {
        tr!("capa Equipo", "team layer")
    } else {
        tr!("proyecto", "project")
    };
    println!(
        "  {}  {what}  {}",
        s.bold(short(&suggestion.uid)),
        s.muted(format!("— {} · {layer}", suggestion.author))
    );
    if suggestion.hidden {
        println!(
            "      {} {}",
            s.warn("!"),
            tr!(
                "tiene caracteres ocultos (⟦U+…⟧): no se puede aceptar",
                "it has hidden characters (⟦U+…⟧): it cannot be accepted"
            )
        );
    }
}

/// `hedy team review`: the suggestions; with an id, takes it (`--accept`)
/// or turns it down (`--reject`).
pub fn review(hedy: &Hedy, team: &str, id: Option<&str>, accept: bool, reject: bool) -> Result<()> {
    let s = Style::for_stdout();
    let suggestions = hedy.team_suggestions(team)?;
    let Some(id) = id else {
        if suggestions.is_empty() {
            println!(
                "{}",
                tr!("No hay sugerencias.", "There are no suggestions.")
            );
            return Ok(());
        }
        let (mine, rest): (Vec<_>, Vec<_>) = suggestions.iter().partition(|s| s.can_review);
        if !mine.is_empty() {
            println!(
                "\n{}",
                s.section(&tr!("Para que revises", "For you to review"))
            );
            mine.iter().for_each(|x| print_suggestion(&s, x));
        }
        if !rest.is_empty() {
            println!("\n{}", s.section(&tr!("De otros", "For others")));
            rest.iter().for_each(|x| print_suggestion(&s, x));
        }
        println!(
            "\n{}",
            s.muted(tr!(
                "Acepta: `hedy team review <id> --accept`; rechaza: `hedy team review <id> --reject`.",
                "Accept: `hedy team review <id> --accept`; turn down: `hedy team review <id> --reject`."
            ))
        );
        return Ok(());
    };
    let found: Vec<&TeamSuggestion> = suggestions
        .iter()
        .filter(|x| x.uid == id || x.uid.ends_with(id))
        .collect();
    let suggestion = match found.as_slice() {
        [one] => *one,
        [] => bail!(
            "{}",
            tr!(
                "no hay una sugerencia «{id}»",
                "there is no suggestion \"{id}\""
            )
        ),
        _ => bail!(
            "{}",
            tr!(
                "varias sugerencias terminan en «{id}»: escribe más de su id",
                "several suggestions end in \"{id}\": type more of its id"
            )
        ),
    };
    match (accept, reject) {
        (true, _) => {
            hedy.team_accept_suggestion(team, &suggestion.uid)?;
            println!("{} {}", s.ok("✓"), tr!("Aceptada.", "Accepted."));
        }
        (_, true) => {
            hedy.team_reject_suggestion(team, &suggestion.uid)?;
            println!("{} {}", s.ok("✓"), tr!("Rechazada.", "Turned down."));
        }
        _ => print_suggestion(&s, suggestion),
    }
    Ok(())
}

/// A person, by their name or the start of their id.
fn find_person(status: &TeamStatus, who: &str) -> Result<PersonId> {
    let who = who.trim();
    let found: Vec<&TeamPerson> = status
        .people
        .iter()
        .filter(|p| {
            p.name.eq_ignore_ascii_case(who)
                || (who.len() >= 4 && p.person.as_str().starts_with(who))
        })
        .collect();
    match found.as_slice() {
        [one] => Ok(one.person.clone()),
        [] => bail!(
            "{}",
            tr!(
                "nadie del equipo es «{who}»",
                "nobody in the team is \"{who}\""
            )
        ),
        _ => bail!(
            "{}",
            tr!(
                "varias personas se llaman «{who}»: usa el comienzo de su id",
                "several people are called \"{who}\": use the start of their id"
            )
        ),
    }
}

/// A PC, by its name or its id.
fn find_pc(status: &TeamStatus, who: &str) -> Result<DeviceId> {
    let who = who.trim();
    let found: Vec<DeviceId> = status
        .people
        .iter()
        .flat_map(|p| &p.devices)
        .filter(|d| d.id.as_str() == who || d.name.trim() == who)
        .map(|d| d.id.clone())
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!(
            "{}",
            tr!(
                "el equipo no tiene la PC «{who}»",
                "the team has no PC \"{who}\""
            )
        ),
        _ => bail!(
            "{}",
            tr!(
                "varias PCs se llaman «{who}»: usa su id",
                "several PCs are called \"{who}\": use its id"
            )
        ),
    }
}

/// `hedy team role <person> --admin|--member`.
pub fn role(hedy: &Hedy, team: &str, who: &str, admin: bool) -> Result<()> {
    let person = find_person(&hedy.team_status(team)?, who)?;
    let role = if admin { Role::Admin } else { Role::Member };
    hedy.team_set_role(team, &person, role)?;
    let role = role_label(role);
    println!(
        "{} {}",
        Style::for_stdout().ok("✓"),
        tr!("{who} ahora es {role}.", "{who} is now {role}.")
    );
    Ok(())
}

/// `hedy team remove <person>` (admins), or `--pc` for one PC.
pub fn remove(hedy: &Hedy, team: &str, who: &str, pc: bool, yes: bool) -> Result<()> {
    let s = Style::for_stdout();
    let status = hedy.team_status(team)?;
    let question = if pc {
        tr!(
            "¿Quitar la PC {who}? No podrá leer lo que el equipo guarde desde ahora. [s/N] ",
            "Remove the PC {who}? It cannot read what the team keeps from now on. [y/N] "
        )
    } else {
        tr!(
            "¿Quitar a {who} del equipo? No podrá leer lo nuevo; sus notas se quedan con su nombre. [s/N] ",
            "Remove {who} from the team? They cannot read what is new; their notes stay under their name. [y/N] "
        )
    };
    if !confirmed(yes, &question, &format!("hedy team remove {who} --yes"))? {
        return Ok(());
    }
    if pc {
        hedy.team_remove_pc(team, &find_pc(&status, who)?)?;
        println!("{} {}", s.ok("✓"), tr!("PC quitada.", "PC removed."));
    } else {
        hedy.team_remove_person(team, &find_person(&status, who)?)?;
        println!(
            "{} {}",
            s.ok("✓"),
            tr!(
                "{who} ya no está en el equipo.",
                "{who} is no longer in the team."
            )
        );
    }
    let url = &status.summary.url;
    println!(
        "{}",
        s.warn(tr!(
            "Quítale también el acceso al repo en tu servidor git ({url}): Hedy no puede.",
            "Also remove their access to the repo on your git server ({url}): Hedy cannot."
        ))
    );
    Ok(())
}

/// `hedy team rename <name>` (the team), or `--person <who>` (a person).
pub fn rename(hedy: &Hedy, team: &str, name: &str, person: Option<&str>) -> Result<()> {
    match person {
        Some(who) => {
            let person = find_person(&hedy.team_status(team)?, who)?;
            hedy.team_rename_person(team, &person, name)?;
        }
        None => hedy.team_rename(team, name)?,
    }
    println!(
        "{} {}",
        Style::for_stdout().ok("✓"),
        tr!(
            "Hecho; sale con la próxima sincronización.",
            "Done; it goes out with the next sync."
        )
    );
    Ok(())
}

/// `hedy team leave`: asks first in a terminal (`--yes` skips it);
/// `--here` only forgets the team on this PC.
pub fn leave(hedy: &Hedy, team: &str, yes: bool, here: bool) -> Result<()> {
    let s = Style::for_stdout();
    if here {
        let question = tr!(
            "¿Olvidar el equipo en esta PC sin pedir salir? Sigue en el equipo hasta que un admin la quite. [s/N] ",
            "Forget the team on this PC without asking to leave? It stays in the team until an admin removes it. [y/N] "
        );
        if confirmed(yes, &question, "hedy team leave --here --yes")? {
            hedy.team_forget(team)?;
            println!(
                "{} {}",
                s.ok("✓"),
                tr!(
                    "Olvidado aquí. Pide a un admin que quite esta PC.",
                    "Forgotten here. Ask an admin to remove this PC."
                )
            );
        }
        return Ok(());
    }
    let question = tr!(
        "¿Salir del equipo en esta PC? Sus notas de equipo se borran aquí (siguen en el equipo); para volver, pide otra invitación. [s/N] ",
        "Leave the team on this PC? Its team notes go from here (they stay in the team); to come back, ask for another invite. [y/N] "
    );
    if !confirmed(yes, &question, "hedy team leave --yes")? {
        return Ok(());
    }
    hedy.team_leave(team)?;
    if hedy.teams()?.iter().any(|t| t.id == team && t.leaving) {
        println!(
            "{} {}",
            s.ok("✓"),
            tr!(
                "Pedido enviado: sales cuando otro admin sincronice (`hedy team now` lo comprueba).",
                "Request sent: you leave once another admin syncs (`hedy team now` checks it)."
            )
        );
    } else {
        println!(
            "{} {}",
            s.ok("✓"),
            tr!("Saliste del equipo.", "You left the team.")
        );
    }
    Ok(())
}
