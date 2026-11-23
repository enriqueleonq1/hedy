//! Writing flows and roles from the terminal (docs/decisions.md §48):
//! `hedy flow new|edit|import` and `hedy roles`. Editing opens the text in the
//! user's editor (`$VISUAL`, `$EDITOR`, else `vi`), checks it on save and,
//! when it does not hold, offers to open it again with the changes kept.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use hedy_core::application::{FlowScope, Hedy};
use hedy_core::domain::flow::Trigger;
use hedy_core::tr;

use crate::commands::refuse_in_agent_session;
use crate::style::Style;
use crate::sync::{interactive, read_line};
use crate::team::clean;

/// The flow `hedy flow new` copies when none is given.
const DEFAULT_FROM: &str = "essential";
/// The role `hedy roles new` copies when none is given.
const DEFAULT_ROLE: &str = "reviewer";

fn project_path(hedy: &Hedy, dir: &Path) -> Result<Option<PathBuf>> {
    Ok(hedy.project_for(dir)?.map(|p| p.path))
}

/// A private file for the editor, removed when dropped.
struct Draft(PathBuf);

impl Draft {
    fn new(text: &str) -> Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let path = std::env::temp_dir().join(format!("hedy-{}-{nanos}.yaml", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options
            .open(&path)
            .with_context(|| path.display().to_string())?;
        file.write_all(text.as_bytes())?;
        Ok(Self(path))
    }
}

impl Drop for Draft {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Opens `text` in the user's editor and returns what was saved.
fn edit_in_editor(text: &str) -> Result<String> {
    if !interactive() {
        bail!(
            "{}",
            tr!(
                "editar necesita una terminal: corre el comando tú",
                "editing needs a terminal: run the command yourself"
            )
        );
    }
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .ok()
        .filter(|e| !e.trim().is_empty())
        .unwrap_or_else(|| "vi".to_owned());
    // As git does: the shell reads `$EDITOR` (`code --wait`, a quoted path).
    let draft = Draft::new(text)?;
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$@\""))
        .arg("editor")
        .arg(&draft.0)
        .status()
        .with_context(|| tr!("no se pudo abrir {editor}", "could not open {editor}"))?;
    if !status.success() {
        bail!(
            "{}",
            tr!(
                "el editor terminó con un error; no se guardó nada",
                "the editor ended with an error; nothing was saved"
            )
        );
    }
    Ok(std::fs::read_to_string(&draft.0)?)
}

/// Edits `text` until `save` takes it or the user gives up. Unchanged text
/// saves nothing, unless `new` (a copy being created).
fn edit_until_saved(
    text: String,
    new: bool,
    mut save: impl FnMut(&str) -> Result<String>,
) -> Result<()> {
    let s = Style::for_stdout();
    let mut current = text.clone();
    loop {
        let edited = edit_in_editor(&current)?;
        if edited == text && !new {
            println!("{}", s.muted(tr!("Sin cambios.", "No changes.")));
            return Ok(());
        }
        match save(&edited) {
            Ok(done) => {
                println!("{} {done}", s.ok("✓"));
                return Ok(());
            }
            Err(e) => {
                println!("{} {}", s.warn("!"), clean(&e.to_string()));
                let again = read_line(&tr!(
                    "¿Volver a abrirlo con tus cambios? [S/n] ",
                    "Open it again with your changes? [Y/n] "
                ))?;
                if matches!(again.trim().to_lowercase().as_str(), "n" | "no") {
                    println!(
                        "{}",
                        s.muted(tr!("No se guardó nada.", "Nothing was saved."))
                    );
                    return Ok(());
                }
                current = edited;
            }
        }
    }
}

/// `hedy flow import <file>`: a flow written as text (what `hedy flow show`
/// prints) saved as the user's. It starts by hand, as any copy does: a file
/// from elsewhere never runs agents after each task until the user says so.
pub fn import_flow(
    hedy: &Hedy,
    dir: &Path,
    file: &Path,
    global: bool,
    replace: bool,
) -> Result<()> {
    refuse_in_agent_session()?;
    let s = Style::for_stdout();
    // A relative path is the user's, from the folder the command speaks of.
    let file = dir.join(file);
    let text =
        std::fs::read_to_string(&file).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    let mut flow = hedy_core::domain::flow_text::from_text(&text)
        .map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    let path = project_path(hedy, dir)?;
    let target = if global { None } else { path.as_deref() };
    let scope = if target.is_some() {
        FlowScope::Project
    } else {
        FlowScope::Global
    };
    let automatic = flow.triggers.iter().any(|t| *t != Trigger::Manual);
    flow.triggers = vec![Trigger::Manual];
    let name = clean(&flow.name);
    let same = hedy
        .flows(target)?
        .into_iter()
        .find(|e| e.scope == scope && e.flow.name.to_lowercase() == flow.name.to_lowercase());
    if same.is_some() && !replace {
        bail!(tr!(
            "Ya tienes un flujo «{name}»: usa --replace para cambiarlo",
            "You already have a flow «{name}»: use --replace to change it"
        ));
    }
    let saved = hedy.save_flow_as(target, same.as_ref().map(|e| e.key.as_str()), flow)?;
    println!(
        "{} {} {} {}",
        s.ok("✓"),
        if same.is_some() {
            tr!("Flujo cambiado:", "Flow changed:")
        } else {
            tr!("Flujo importado:", "Flow imported:")
        },
        s.bold(clean(&saved.flow.name)),
        s.muted(format!("· {}", place(global, &path)))
    );
    if automatic {
        println!(
            "{}",
            s.muted(tr!(
                "Empieza a mano: si quieres que siga al final de cada tarea, actívalo en la app o con `hedy flow edit`.",
                "It starts by hand: to have it follow each task's end, turn that on in the app or with `hedy flow edit`."
            ))
        );
    }
    Ok(())
}

pub fn new_flow(
    hedy: &Hedy,
    dir: &Path,
    name: &str,
    from: Option<&str>,
    global: bool,
) -> Result<()> {
    refuse_in_agent_session()?;
    let s = Style::for_stdout();
    let path = project_path(hedy, dir)?;
    let source = hedy.flow(path.as_deref(), from.unwrap_or(DEFAULT_FROM))?;
    let mut flow = source.flow;
    flow.name = name.trim().to_owned();
    let target = if global { None } else { path.as_deref() };
    let saved = hedy.save_flow_as(target, None, flow)?;
    println!(
        "{} {} {} {}",
        s.ok("✓"),
        tr!("Flujo creado:", "Flow created:"),
        s.bold(clean(&saved.flow.name)),
        s.muted(format!("· {}", place(global, &path)))
    );
    let name = clean(&saved.flow.name);
    println!(
        "{}",
        s.muted(tr!(
            "Edítalo con `hedy flow edit \"{name}\"` o en la app, pestaña Flujos.",
            "Edit it with `hedy flow edit \"{name}\"` or in the app, Flows tab."
        ))
    );
    Ok(())
}

pub fn edit_flow(hedy: &Hedy, dir: &Path, name: &str) -> Result<()> {
    refuse_in_agent_session()?;
    let path = project_path(hedy, dir)?;
    let entry = hedy.flow(path.as_deref(), name)?;
    if entry.scope == FlowScope::Hedy {
        let key = &entry.key;
        bail!(
            "{}",
            tr!(
                "es una plantilla de Hedy: crea tu copia con `hedy flow new <nombre> --from {key}`",
                "it is one of Hedy's templates: make your copy with `hedy flow new <name> --from {key}`"
            )
        );
    }
    let text = hedy.flow_text(path.as_deref(), &entry.key)?;
    edit_until_saved(text, false, |edited| {
        let saved = hedy.save_flow_text_as(path.as_deref(), &entry.key, edited)?;
        let name = clean(&saved.flow.name);
        Ok(tr!("Guardado «{name}».", "Saved «{name}»."))
    })
}

pub fn roles_list(hedy: &Hedy, dir: &Path) -> Result<()> {
    let s = Style::for_stdout();
    let path = project_path(hedy, dir)?;
    let roles = hedy.roles(path.as_deref())?;
    println!();
    for (scope, title) in [
        (FlowScope::Hedy, tr!("Roles de Hedy", "Hedy's roles")),
        (FlowScope::Project, tr!("Del proyecto", "This project's")),
        (
            FlowScope::Global,
            tr!("Para todos los proyectos", "For every project"),
        ),
        (FlowScope::Team, tr!("Del equipo", "The team's")),
    ] {
        let here: Vec<_> = roles.iter().filter(|r| r.scope == scope).collect();
        if here.is_empty() {
            continue;
        }
        println!("{}", s.section(&title));
        for entry in here {
            let role = &entry.role;
            let mut traits = vec![if role.can.write {
                tr!("escribe", "writes")
            } else {
                tr!("solo lee", "read-only")
            }];
            if role.can.run {
                traits.push(tr!("corre comandos", "runs commands"));
            }
            if role.delivers == hedy_core::domain::flow::Delivers::Verdict {
                traits.push(tr!("da un veredicto", "gives a verdict"));
            }
            println!(
                "  {}  {} {}",
                s.bold(clean(&role.name)),
                s.muted(format!("{} ·", role.id)),
                s.muted(traits.join(", "))
            );
            if !role.summary.is_empty() {
                println!("    {}", clean(&role.summary));
            }
        }
        println!();
    }
    Ok(())
}

pub fn role_show(hedy: &Hedy, dir: &Path, id: &str) -> Result<()> {
    let path = project_path(hedy, dir)?;
    for line in hedy.role_text(path.as_deref(), id)?.lines() {
        println!("{}", clean(line));
    }
    Ok(())
}

/// Where a role or flow is saved, said after saving it.
fn place(global: bool, path: &Option<PathBuf>) -> String {
    if global || path.is_none() {
        tr!("para todos los proyectos", "for every project")
    } else {
        tr!("para este proyecto", "for this project")
    }
}

/// The id a role's text names, as written.
fn id_in(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("id:"))
        .map(str::trim)
}

pub fn role_new(hedy: &Hedy, dir: &Path, id: &str, from: Option<&str>, global: bool) -> Result<()> {
    refuse_in_agent_session()?;
    let path = project_path(hedy, dir)?;
    // A new role never replaces one there is (Hedy's included): that is `edit`.
    if hedy.roles(path.as_deref())?.iter().any(|r| r.role.id == id) {
        bail!(
            "{}",
            tr!(
                "ya hay un rol «{id}»: cámbialo con `hedy roles edit {id}`",
                "there is already a role «{id}»: change it with `hedy roles edit {id}`"
            )
        );
    }
    let base = hedy.role_text(path.as_deref(), from.unwrap_or(DEFAULT_ROLE))?;
    // The copy under its new id; the rest is the user's to change.
    let text: String = base
        .lines()
        .map(|line| {
            if line.starts_with("id:") {
                format!("id: {id}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    // A new role is saved even when the copy is kept as it was.
    let taken: Vec<String> = hedy
        .roles(path.as_deref())?
        .into_iter()
        .map(|r| r.role.id)
        .collect();
    let here = place(global, &path);
    edit_until_saved(text, true, |edited| {
        if let Some(other) = id_in(edited).filter(|i| taken.iter().any(|t| t == i)) {
            bail!(
                "{}",
                tr!(
                    "ya hay un rol «{other}»",
                    "there is already a role «{other}»"
                )
            );
        }
        let saved = hedy.save_role_text(path.as_deref(), edited, global)?;
        let name = clean(&saved.role.name);
        Ok(tr!(
            "Rol creado: «{name}», {here}.",
            "Role created: «{name}», {here}."
        ))
    })
}

pub fn role_edit(hedy: &Hedy, dir: &Path, id: &str, global: bool) -> Result<()> {
    refuse_in_agent_session()?;
    let path = project_path(hedy, dir)?;
    let text = hedy.role_text(path.as_deref(), id)?;
    let others: Vec<String> = hedy
        .roles(path.as_deref())?
        .into_iter()
        .map(|r| r.role.id)
        .filter(|other| other != id)
        .collect();
    let here = place(global, &path);
    edit_until_saved(text, false, |edited| {
        // Changing the id onto another role would replace it unseen.
        if let Some(other) = id_in(edited).filter(|i| others.iter().any(|o| o == i)) {
            bail!(
                "{}",
                tr!(
                    "ya hay un rol «{other}»",
                    "there is already a role «{other}»"
                )
            );
        }
        let saved = hedy.save_role_text(path.as_deref(), edited, global)?;
        let name = clean(&saved.role.name);
        Ok(tr!("Guardado «{name}», {here}.", "Saved «{name}», {here}."))
    })
}

pub fn role_remove(hedy: &Hedy, dir: &Path, id: &str) -> Result<()> {
    refuse_in_agent_session()?;
    let s = Style::for_stdout();
    let path = project_path(hedy, dir)?;
    hedy.delete_role(path.as_deref(), id)?;
    println!(
        "{} {}",
        s.ok("✓"),
        tr!("Rol quitado: {id}", "Role removed: {id}")
    );
    Ok(())
}

/// `hedy flow default [name] [--clear]`: the flow this project's «Cambiar
/// código» tasks start with (docs/decisions.md §56), shown, set or cleared.
pub fn default_flow(hedy: &Hedy, dir: &Path, name: Option<&str>, clear: bool) -> Result<()> {
    let s = Style::for_stdout();
    let Some(path) = project_path(hedy, dir)? else {
        bail!(tr!(
            "Esta carpeta no es un proyecto de Hedy",
            "This folder is not a Hedy project"
        ));
    };
    if name.is_some() || clear {
        refuse_in_agent_session()?;
        hedy.set_default_flow(&path, name)?;
    }
    match hedy.default_flow(&path)? {
        Some(entry) => println!(
            "{} {} {}",
            s.ok("✓"),
            tr!(
                "Las tareas de «Cambiar código» empiezan con el flujo",
                "«Change code» tasks start with the flow"
            ),
            s.bold(clean(&entry.flow.name)),
        ),
        None => println!(
            "{} {}",
            s.ok("✓"),
            tr!(
                "Las tareas de «Cambiar código» las hace una sola IA (sin flujo)",
                "«Change code» tasks are done by one AI (no flow)"
            ),
        ),
    }
    Ok(())
}

pub fn share_flow(hedy: &Hedy, dir: &Path, name: &str) -> Result<()> {
    refuse_in_agent_session()?;
    let project = crate::commands::project_here(hedy, dir)?;
    let shared = hedy.team_share_flow(&project.path, name)?;
    say_shared(hedy, &project.path, &shared)
}

pub fn unshare_flow(hedy: &Hedy, dir: &Path, name: &str) -> Result<()> {
    refuse_in_agent_session()?;
    let project = crate::commands::project_here(hedy, dir)?;
    hedy.team_unshare_flow(&project.path, name)?;
    let s = Style::for_stdout();
    let name = clean(name);
    println!(
        "{} {}",
        s.ok("✓"),
        tr!(
            "«{name}» ya no se comparte.",
            "«{name}» is no longer shared."
        )
    );
    Ok(())
}

pub fn share_role(hedy: &Hedy, dir: &Path, id: &str) -> Result<()> {
    refuse_in_agent_session()?;
    let project = crate::commands::project_here(hedy, dir)?;
    let shared = hedy.team_share_role(&project.path, id)?;
    say_shared(hedy, &project.path, &shared)
}

pub fn unshare_role(hedy: &Hedy, dir: &Path, id: &str) -> Result<()> {
    refuse_in_agent_session()?;
    let project = crate::commands::project_here(hedy, dir)?;
    hedy.team_unshare_role(&project.path, id)?;
    let s = Style::for_stdout();
    println!(
        "{} {}",
        s.ok("✓"),
        tr!(
            "El rol {id} ya no se comparte.",
            "Role {id} is no longer shared."
        )
    );
    Ok(())
}

fn say_shared(hedy: &Hedy, project: &Path, name: &str) -> Result<()> {
    let s = Style::for_stdout();
    let team = hedy
        .flow_sharing(project)?
        .map(|t| clean(&t.name))
        .unwrap_or_default();
    let name = clean(name);
    println!(
        "{} {}",
        s.ok("✓"),
        tr!(
            "«{name}» queda compartido con {team}: llega a los demás con `hedy team now`.",
            "«{name}» is shared with {team}: it reaches the others with `hedy team now`."
        )
    );
    Ok(())
}

/// The user allows a team flow's test command on this PC (never an agent).
pub fn allow_command(hedy: &Hedy, command: &str) -> Result<()> {
    refuse_in_agent_session()?;
    hedy.approve_flow_command(command)?;
    let s = Style::for_stdout();
    let command = clean(command);
    println!(
        "{} {}",
        s.ok("✓"),
        tr!(
            "Permitido en este PC: {command}",
            "Allowed on this PC: {command}"
        )
    );
    Ok(())
}
