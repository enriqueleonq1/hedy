//! `hedy skills import` and `hedy skills export`: skills in and out of Hedy
//! as `SKILL.md` files. An import shows what goes in first and asks.

use std::path::Path;

use anyhow::Result;
use hedy_core::application::{Hedy, ImportCandidate};
use hedy_core::tr;

use crate::commands::{hint, print_skill_files, project_here, refuse_in_agent_session, short_path};
use crate::style::Style;
use crate::sync::confirmed;
use crate::team::clean;

/// Characters of each description shown in the preview.
const DESCRIPTION_SHOWN: usize = 100;
/// Lines of each body shown in the preview.
const BODY_LINES_SHOWN: usize = 6;

pub fn import(
    hedy: &Hedy,
    dir: &Path,
    source: &Path,
    global: bool,
    replace: bool,
    yes: bool,
) -> Result<()> {
    refuse_in_agent_session()?;
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let path = dir.join(source);
    let found = hedy.preview_import(&project.path, &path, global)?;
    let preview = &found.candidates;
    println!();
    println!(
        "  {}  {}",
        tr!("Habilidades encontradas", "Skills found"),
        s.muted(short_path(&path))
    );
    println!();
    for candidate in preview {
        print_candidate(&s, candidate, replace);
    }
    if found.total > preview.len() {
        let (shown, total) = (preview.len(), found.total);
        let line = tr!(
            "Se muestran las primeras {shown} de {total}: importa el resto por carpetas.",
            "Showing the first {shown} of {total}: import the rest folder by folder."
        );
        println!("  {}", s.warn(line));
    }
    let ready = preview
        .iter()
        .filter(|c| c.problem.is_none() && (c.replaces.is_none() || replace))
        .count();
    println!();
    if ready == 0 {
        println!("  {}\n", tr!("Nada que importar.", "Nothing to import."));
        return Ok(());
    }
    let name = clean(&project.name);
    let question = tr!(
        "¿Importar {ready} a {name}? Tus IAs las usarán dentro de Hedy. [s/N] ",
        "Import {ready} into {name}? Your AIs will use them inside Hedy. [y/N] "
    );
    let mut again = format!(
        "hedy skills import {}",
        shell_word(&source.to_string_lossy())
    );
    for (on, flag) in [(global, " --global"), (replace, " --replace")] {
        if on {
            again.push_str(flag);
        }
    }
    again.push_str(" --yes");
    if !confirmed(yes, &question, &again)? {
        println!("{}", tr!("No se importó nada.", "Nothing was imported."));
        return Ok(());
    }
    let replacing: Vec<String> = match replace {
        true => preview.iter().map(|c| c.draft.name.clone()).collect(),
        false => Vec::new(),
    };
    let report = hedy.import_skills(&project.path, preview, &replacing)?;
    let (imported, updated) = (report.imported.len(), report.updated.len());
    let done = match (imported, updated) {
        (n, 0) => tr!("{n} importadas.", "{n} imported."),
        (0, n) => tr!("{n} actualizadas.", "{n} updated."),
        (n, m) => tr!(
            "{n} importadas y {m} actualizadas.",
            "{n} imported and {m} updated."
        ),
    };
    println!("{} {done}", s.ok("✔"));
    let skipped = report.skipped.len();
    if skipped > 0 {
        let line = tr!(
            "{skipped} no entraron (mira la lista de arriba).",
            "{skipped} did not go in (see the list above)."
        );
        println!("  {}", s.muted(line));
    }
    print_skill_files(&s, &report.files);
    Ok(())
}

/// One line per skill, then what stops it or what Hedy leaves out. The text
/// comes from someone else's file: cleaned for the terminal.
fn print_candidate(s: &Style, candidate: &ImportCandidate, replace: bool) {
    let name = clean(&candidate.draft.name);
    let (mark, note) = match (&candidate.problem, &candidate.replaces) {
        (Some(problem), _) => (s.warn("!"), Some(s.warn(clean(&problem.message())))),
        (None, Some(_)) if replace => (
            s.ok("↻"),
            Some(s.muted(tr!(
                "actualiza tu habilidad con ese nombre",
                "updates your skill with that name"
            ))),
        ),
        (None, Some(_)) => (
            s.warn("!"),
            Some(hint(
                s,
                &tr!(
                    "ya tienes una con ese nombre: se salta (usa `--replace` para actualizarla)",
                    "you already have one with that name: skipped (use `--replace` to update it)"
                ),
            )),
        ),
        (None, None) => (s.ok("+"), None),
    };
    println!("  {mark} {}", s.bold(&name));
    let description = clean(&candidate.draft.description);
    if !description.is_empty() {
        println!("    {}", shortened(&description));
    }
    if let Some(note) = note {
        println!("    {note}");
    }
    // What the AIs would read: its size and how it starts.
    let body = &candidate.draft.body;
    if candidate.problem.is_none() && !body.is_empty() {
        let (lines, chars) = (body.lines().count(), body.chars().count());
        println!(
            "    {}",
            s.muted(tr!(
                "{lines} líneas · {chars} caracteres:",
                "{lines} lines · {chars} characters:"
            ))
        );
        for line in body.lines().take(BODY_LINES_SHOWN) {
            println!("    {} {}", s.muted("│"), shortened(&clean(line)));
        }
        if lines > BODY_LINES_SHOWN {
            println!("    {} {}", s.muted("│"), s.muted("…"));
        }
    }
    if !candidate.dropped_keys.is_empty() {
        let keys = clean(&candidate.dropped_keys.join(", "));
        println!(
            "    {}",
            s.muted(tr!("Hedy no guarda: {keys}", "Hedy does not keep: {keys}"))
        );
    }
    if !candidate.extra_files.is_empty() {
        let files = clean(&candidate.extra_files.join(", "));
        println!(
            "    {}",
            s.muted(tr!(
                "no se importan (Hedy guarda solo el texto): {files}",
                "not imported (Hedy keeps only the text): {files}"
            ))
        );
    }
}

/// A path as the user would type it again in a shell.
fn shell_word(text: &str) -> String {
    let plain = text
        .chars()
        .all(|c| c.is_alphanumeric() || "/._-~".contains(c));
    match plain {
        true => text.to_owned(),
        false => format!("'{}'", clean(text).replace('\'', "'\\''")),
    }
}

fn shortened(text: &str) -> String {
    if text.chars().count() <= DESCRIPTION_SHOWN {
        return text.to_owned();
    }
    let cut: String = text.chars().take(DESCRIPTION_SHOWN - 1).collect();
    format!("{}…", cut.trim_end())
}

pub fn export(hedy: &Hedy, dir: &Path, names: &[String], to: &Path) -> Result<()> {
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let dest = dir.join(to);
    let report = hedy.export_skills(&project.path, names, &dest)?;
    if report.written.is_empty() && report.skipped.is_empty() {
        println!(
            "{}",
            tr!(
                "No hay habilidades aprobadas que exportar.",
                "There are no approved skills to export."
            )
        );
        return Ok(());
    }
    for path in &report.written {
        let path = short_path(path);
        println!("  {}", s.muted(tr!("escrita {path}", "wrote {path}")));
    }
    for path in &report.skipped {
        let path = short_path(path);
        println!(
            "  {} {}",
            s.warn("!"),
            tr!(
                "{path} ya existe: no se tocó",
                "{path} already exists: left alone"
            )
        );
    }
    let n = report.written.len();
    let folder = short_path(&dest);
    println!(
        "{} {}",
        s.ok("✔"),
        tr!("{n} exportadas a {folder}.", "{n} exported to {folder}.")
    );
    if n > 0 {
        let line = tr!(
            "Son archivos normales: cópialos a .claude/skills o .agents/skills para usarlos fuera de Hedy.",
            "They are plain files: copy them to .claude/skills or .agents/skills to use them outside Hedy."
        );
        println!("  {}", s.muted(line));
    }
    Ok(())
}
