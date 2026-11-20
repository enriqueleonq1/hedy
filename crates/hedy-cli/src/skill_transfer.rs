//! `hedy skills import` and `hedy skills export`: skills in and out of Hedy
//! as `SKILL.md` files. An import shows what goes in first and asks.

use std::path::Path;

use anyhow::Result;
use hedy_core::application::{Hedy, ImportCandidate, SkillReview};
use hedy_core::domain::skill_check::{Finding, SkillText, Verdict, Weight, predicted};
use hedy_core::tr;

use crate::commands::{hint, print_skill_files, project_here, refuse_in_agent_session, short_path};
use crate::spinner;
use crate::style::Style;
use crate::sync::confirmed;
use crate::team::clean;

/// Characters of each description shown in the preview.
const DESCRIPTION_SHOWN: usize = 100;
/// Lines of each body shown in the preview.
const BODY_LINES_SHOWN: usize = 6;
/// Most skills the AI reviews on its own in one import: past that, only
/// with `--review` (each one is a request).
const AUTO_REVIEWS: usize = 3;

/// Who reviews with AI, and whether.
pub struct Reviewer<'a> {
    /// `None`: on its own for a few skills; `Some(false)`: never.
    pub wanted: Option<bool>,
    pub account: Option<&'a str>,
    pub model: Option<&'a str>,
}

pub fn import(
    hedy: &Hedy,
    dir: &Path,
    source: &Path,
    global: bool,
    replace: bool,
    yes: bool,
    reviewer: &Reviewer,
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
    let importable = |c: &ImportCandidate| c.problem.is_none() && (c.replaces.is_none() || replace);
    let to_review = preview.iter().filter(|c| importable(c)).count();
    // On its own only where the user is looking, before deciding: never in
    // a script, nor before a question that cannot be asked.
    let review = reviewer
        .wanted
        .unwrap_or(crate::sync::interactive() && to_review <= AUTO_REVIEWS);
    for candidate in preview {
        print_candidate(&s, candidate, replace);
        let ai = (review && importable(candidate)).then(|| {
            ai_review(
                hedy,
                &project.path,
                &candidate.text(),
                &candidate.findings,
                reviewer,
            )
        });
        print_analysis(&s, &candidate.findings, ai.as_ref());
    }
    if !review && to_review > 0 && reviewer.wanted.is_none() {
        let line = tr!(
            "La IA no las revisó contra tus notas (una petición por habilidad): añade `--review` para que lo haga.",
            "The AI did not review them against your notes (one request per skill): add `--review` to have it do so."
        );
        println!("  {}", hint(&s, &line));
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
    let flags = [
        (global, " --global"),
        (replace, " --replace"),
        (reviewer.wanted == Some(true), " --review"),
        (reviewer.wanted == Some(false), " --no-review"),
    ];
    for (on, flag) in flags {
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
        let size = match lines {
            1 => tr!(
                "1 línea · {chars} caracteres:",
                "1 line · {chars} characters:"
            ),
            n => tr!(
                "{n} líneas · {chars} caracteres:",
                "{n} lines · {chars} characters:"
            ),
        };
        println!("    {}", s.muted(size));
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

/// Level 2, or why it could not run (the import goes on without it).
fn ai_review(
    hedy: &Hedy,
    project: &Path,
    skill: &SkillText,
    measured: &[Finding],
    reviewer: &Reviewer,
) -> Result<SkillReview, String> {
    let name = clean(skill.name);
    spinner::while_running(
        &tr!("La IA revisa {name}…", "The AI reviews {name}…"),
        || hedy.review_skill(project, skill, measured, reviewer.account, reviewer.model),
    )
    .map_err(|e| clean(&e.to_string()))
}

/// What Hedy measured, what the AI saw and the verdict both suggest.
fn print_analysis(s: &Style, measured: &[Finding], ai: Option<&Result<SkillReview, String>>) {
    if measured.is_empty() && ai.is_none() {
        return;
    }
    let mark = |weight: Weight| match weight {
        Weight::Info => s.muted("·"),
        Weight::Good => s.ok("✓"),
        Weight::Warning => s.warn("!"),
        Weight::Bad => s.error("✕"),
    };
    let mut all = measured.to_vec();
    println!(
        "    {}",
        s.muted(tr!("Lo que mide Hedy", "What Hedy measures"))
    );
    for finding in measured {
        println!(
            "      {} {}",
            mark(finding.weight()),
            clean(&finding.message())
        );
    }
    match ai {
        Some(Ok(review)) => {
            let who = review
                .account
                .as_ref()
                .map(|a| clean(&a.name))
                .unwrap_or_default();
            let secs = review.duration_ms.div_ceil(1000);
            let notes = match review.notes {
                1 => tr!("1 nota tuya", "1 of your notes"),
                n => tr!("{n} notas tuyas", "{n} of your notes"),
            };
            println!(
                "    {}",
                s.muted(tr!(
                    "Lo que opina la IA · {who} · {secs} s · {notes}, nada del código",
                    "What the AI thinks · {who} · {secs} s · {notes}, none of the code"
                ))
            );
            if review.unread {
                println!(
                    "      {} {}",
                    s.warn("!"),
                    tr!(
                        "No se pudo leer su respuesta.",
                        "Its answer could not be read."
                    )
                );
            } else if review.findings.is_empty() {
                println!(
                    "      {} {}",
                    s.ok("✓"),
                    tr!("Nada que señalar.", "Nothing to point out.")
                );
            }
            for finding in &review.findings {
                println!(
                    "      {} {}",
                    mark(finding.weight()),
                    clean(&finding.message())
                );
            }
            all.extend(review.findings.iter().cloned());
        }
        Some(Err(why)) => println!(
            "    {} {}",
            s.warn("!"),
            tr!(
                "La IA no pudo revisarla: {why}",
                "The AI could not review it: {why}"
            )
        ),
        None => {}
    }
    let verdict = match predicted(&all) {
        // Nothing found against it is not proof that it helps.
        Verdict::Useful => s.muted(tr!(
            "Nada en contra (sin probar)",
            "Nothing against it (untested)"
        )),
        Verdict::Doubtful => s.warn(tr!("Dudosa", "Doubtful")),
        Verdict::Harmful => s.error(tr!("Probablemente contraproducente", "Probably harmful")),
        Verdict::Untested => s.muted(tr!("Sin probar", "Untested")),
    };
    println!("    → {verdict}");
}

/// `hedy skills check <name>`: the analysis of an approved skill.
pub fn check(hedy: &Hedy, dir: &Path, name: &str, reviewer: &Reviewer) -> Result<()> {
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let skill = hedy.skill_named(&project.path, name)?;
    let (findings, _) = hedy.measure_approved_skill(&project.path, &skill)?;
    let text = SkillText {
        name: &skill.name,
        description: &skill.description,
        body: &skill.body,
        extra_files: &[],
    };
    let ai = (reviewer.wanted != Some(false))
        .then(|| ai_review(hedy, &project.path, &text, &findings, reviewer));
    println!();
    println!("  {}", s.bold(clean(&skill.name)));
    println!("    {}", shortened(&clean(&skill.description)));
    print_analysis(&s, &findings, ai.as_ref());
    println!();
    Ok(())
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
    let mark = if n > 0 { s.ok("✔") } else { s.warn("!") };
    println!(
        "{mark} {}",
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
