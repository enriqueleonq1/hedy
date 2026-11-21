//! A role as text (docs/decisions.md §48): what `hedy roles show` prints and
//! `hedy roles edit` opens. The same strict subset of YAML as a flow's text:
//!
//! ```yaml
//! id: accessibility
//! name: Accessibility
//! summary: Checks the changes can be used with a keyboard and a reader
//! delivers: verdict
//! can: []
//! sees: all
//! plan_first: false
//! instructions: |
//!   You review accessibility. Read the changes...
//! ```
//!
//! `delivers`: `text` or `verdict`; `can`: any of `write`, `run`; `sees`:
//! `all` or a list of board sections; `skills` and `notes`: lists of names.

use super::flow::{Abilities, Carries, Delivers, RoleDef, Sees, valid_name, valid_role_id};
use super::flow_text::TextError;
use crate::tr;

/// Longest instructions accepted.
pub const MAX_INSTRUCTIONS: usize = 8_000;

fn list(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

pub fn to_text(role: &RoleDef) -> String {
    let mut out = String::new();
    out.push_str(&format!("id: {}\n", role.id));
    out.push_str(&format!("name: {}\n", role.name));
    if !role.summary.is_empty() {
        out.push_str(&format!("summary: {}\n", role.summary));
    }
    let delivers = match role.delivers {
        Delivers::Text => "text",
        Delivers::Verdict => "verdict",
    };
    out.push_str(&format!("delivers: {delivers}\n"));
    let mut can = Vec::new();
    if role.can.write {
        can.push("write".to_owned());
    }
    if role.can.run {
        can.push("run".to_owned());
    }
    out.push_str(&format!("can: {}\n", list(&can)));
    match &role.sees {
        Sees::All => out.push_str("sees: all\n"),
        Sees::Sections(sections) => out.push_str(&format!("sees: {}\n", list(sections))),
    }
    out.push_str(&format!("plan_first: {}\n", role.plan_first));
    if !role.carries.skills.is_empty() {
        out.push_str(&format!("skills: {}\n", list(&role.carries.skills)));
    }
    if !role.carries.notes.is_empty() {
        out.push_str(&format!("notes: {}\n", list(&role.carries.notes)));
    }
    out.push_str("instructions: |\n");
    for line in role.instructions.lines() {
        if line.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&format!("  {line}\n"));
        }
    }
    out
}

/// `[a, b]` → `["a", "b"]`.
fn parse_list(value: &str, line: usize) -> Result<Vec<String>, TextError> {
    let inner = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .ok_or_else(|| {
            TextError::at(
                line,
                tr!(
                    "se esperaba una lista entre corchetes: [a, b]",
                    "a list in brackets was expected: [a, b]"
                ),
            )
        })?;
    Ok(inner
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect())
}

pub fn from_text(text: &str) -> Result<RoleDef, TextError> {
    if text.len() > super::flow_text::MAX_TEXT {
        return Err(TextError::at(
            0,
            tr!("el texto es demasiado largo", "the text is too long"),
        ));
    }
    let mut id = None;
    let mut name = None;
    let mut summary = String::new();
    let mut delivers = Delivers::Text;
    let mut can = Abilities::default();
    let mut sees = Sees::All;
    let mut plan_first = false;
    let mut carries = Carries::default();
    let mut instructions: Option<Vec<String>> = None;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if let Some(body) = instructions.as_mut() {
            // Everything after `instructions: |`, two spaces in.
            if raw.trim().is_empty() {
                body.push(String::new());
            } else if let Some(rest) = raw.strip_prefix("  ") {
                body.push(rest.to_owned());
            } else {
                return Err(TextError::at(
                    line,
                    tr!(
                        "las instrucciones van al final, con dos espacios al principio de cada línea",
                        "the instructions go last, with two spaces at the start of each line"
                    ),
                ));
            }
            continue;
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            return Err(TextError::at(
                line,
                tr!("se esperaba «clave: valor»", "«key: value» was expected"),
            ));
        };
        let value = value.trim();
        match key.trim() {
            "id" => id = Some(value.to_owned()),
            "name" => name = Some(value.to_owned()),
            "summary" => value.clone_into(&mut summary),
            "delivers" => {
                delivers = match value {
                    "text" => Delivers::Text,
                    "verdict" => Delivers::Verdict,
                    _ => {
                        return Err(TextError::at(
                            line,
                            tr!(
                                "«delivers» es text o verdict",
                                "«delivers» is text or verdict"
                            ),
                        ));
                    }
                }
            }
            "can" => {
                can = Abilities::default();
                for ability in parse_list(value, line)? {
                    match ability.as_str() {
                        "write" => can.write = true,
                        "run" => can.run = true,
                        other => {
                            return Err(TextError::at(
                                line,
                                tr!(
                                    "«{other}» no es una capacidad: write o run",
                                    "«{other}» is not an ability: write or run"
                                ),
                            ));
                        }
                    }
                }
            }
            "sees" => {
                sees = if value == "all" {
                    Sees::All
                } else {
                    Sees::Sections(parse_list(value, line)?)
                }
            }
            "plan_first" => {
                plan_first = match value {
                    "true" => true,
                    "false" => false,
                    _ => {
                        return Err(TextError::at(
                            line,
                            tr!(
                                "«plan_first» es true o false",
                                "«plan_first» is true or false"
                            ),
                        ));
                    }
                }
            }
            "skills" => carries.skills = parse_list(value, line)?,
            "notes" => carries.notes = parse_list(value, line)?,
            "instructions" if value == "|" => instructions = Some(Vec::new()),
            other => {
                return Err(TextError::at(
                    line,
                    tr!(
                        "«{other}» no es una clave de un rol",
                        "«{other}» is not a role key"
                    ),
                ));
            }
        }
    }
    let missing = |what: &str| TextError::at(0, tr!("falta «{what}»", "«{what}» is missing"));
    let id = id.ok_or_else(|| missing("id"))?;
    if !valid_role_id(&id) {
        return Err(TextError::at(
            0,
            tr!(
                "«{id}» no sirve como id de un rol (minúsculas, números y guiones)",
                "«{id}» is not a valid role id (lowercase letters, digits and dashes)"
            ),
        ));
    }
    let name = name.ok_or_else(|| missing("name"))?;
    if !valid_name(&name) {
        return Err(TextError::at(
            0,
            tr!(
                "el rol necesita un nombre de una línea",
                "the role needs a one-line name"
            ),
        ));
    }
    let instructions = instructions
        .map(|lines| lines.join("\n").trim_end().to_owned())
        .filter(|i| !i.trim().is_empty())
        .ok_or_else(|| missing("instructions"))?;
    if instructions.chars().count() > MAX_INSTRUCTIONS {
        return Err(TextError::at(
            0,
            tr!(
                "las instrucciones pasan de {MAX_INSTRUCTIONS} caracteres",
                "the instructions are longer than {MAX_INSTRUCTIONS} characters"
            ),
        ));
    }
    if let Sees::Sections(sections) = &sees
        && let Some(bad) = sections.iter().find(|s| !super::flow::valid_section(s))
    {
        return Err(TextError::at(
            0,
            tr!(
                "«{bad}» no es una sección de la pizarra",
                "«{bad}» is not a board section"
            ),
        ));
    }
    Ok(RoleDef {
        id,
        name,
        summary,
        instructions,
        carries,
        delivers,
        can,
        sees,
        plan_first,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flow::builtin_roles;

    #[test]
    fn every_builtin_role_reads_back_as_written() {
        for role in builtin_roles() {
            assert_eq!(from_text(&to_text(&role)).unwrap(), role, "{}", role.id);
        }
    }

    #[test]
    fn a_role_written_by_hand() {
        let role = from_text(
            "id: accesibilidad\nname: Accesibilidad\ndelivers: verdict\ncan: []\nsees: [frame, programmer]\ninstructions: |\n  Revisa el teclado.\n\n  Y el lector.\n",
        )
        .unwrap();
        assert_eq!(role.instructions, "Revisa el teclado.\n\nY el lector.");
        assert_eq!(role.delivers, Delivers::Verdict);
        assert_eq!(
            role.sees,
            Sees::Sections(vec!["frame".into(), "programmer".into()])
        );
        assert!(!role.can.write);
    }

    #[test]
    fn what_is_refused() {
        for bad in [
            "name: X\ninstructions: |\n  y\n",
            "id: feedback\nname: X\ninstructions: |\n  y\n",
            "id: x\nname: X\n",
            "id: x\nname: X\ncan: [delete]\ninstructions: |\n  y\n",
            "id: x\nname: X\ninstructions: |\nno indent\n",
            "id: x\nname: X\ncolor: red\ninstructions: |\n  y\n",
        ] {
            assert!(from_text(bad).is_err(), "{bad}");
        }
    }
}
