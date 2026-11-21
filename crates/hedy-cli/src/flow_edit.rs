//! Writing flows and roles from the terminal (docs/decisions.md §48):
//! `hedy flow new|edit` and `hedy roles`. Editing opens the text in the
//! user's editor (`$VISUAL`, `$EDITOR`, else `vi`), checks it on save and,
//! when it does not hold, offers to open it again with the changes kept.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use hedy_core::application::{FlowScope, Hedy};
use hedy_core::tr;

use crate::commands::refuse_in_agent_session;
use crate::style::Style;
use crate::sync::{interactive, read_line};
use crate::team::clean;

/// The flow `hedy flow new` copies when none is given.
const DEFAULT_FROM: &str = "fix-loop";
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
    // `code --wait`: the editor and its own arguments.
    let mut words = editor.split_whitespace();
    let program = words.next().unwrap_or("vi");
    let draft = Draft::new(text)?;
    let status = Command::new(program)
        .args(words)
        .arg(&draft.0)
        .status()
        .with_context(|| tr!("no se pudo abrir {program}", "could not open {program}"))?;
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
    let place = if global { None } else { path.as_deref() };
    let saved = hedy.save_flow_as(place, None, flow)?;
    println!(
        "{} {} {}",
        s.ok("✓"),
        tr!("Flujo creado:", "Flow created:"),
        s.bold(clean(&saved.flow.name))
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
    print!("{}", hedy.role_text(path.as_deref(), id)?);
    Ok(())
}

pub fn role_new(hedy: &Hedy, dir: &Path, id: &str, from: Option<&str>, global: bool) -> Result<()> {
    refuse_in_agent_session()?;
    let path = project_path(hedy, dir)?;
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
    edit_until_saved(text, true, |edited| {
        let saved = hedy.save_role_text(path.as_deref(), edited, global)?;
        let name = clean(&saved.role.name);
        Ok(tr!("Rol creado: «{name}».", "Role created: «{name}»."))
    })
}

pub fn role_edit(hedy: &Hedy, dir: &Path, id: &str, global: bool) -> Result<()> {
    refuse_in_agent_session()?;
    let path = project_path(hedy, dir)?;
    let text = hedy.role_text(path.as_deref(), id)?;
    edit_until_saved(text, false, |edited| {
        let saved = hedy.save_role_text(path.as_deref(), edited, global)?;
        let name = clean(&saved.role.name);
        Ok(tr!("Guardado «{name}».", "Saved «{name}»."))
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
