//! Skills in and out of Hedy as plain `SKILL.md` files (the Agent Skills
//! format Claude Code, Codex and the other CLIs read): YAML frontmatter with
//! `name` and `description`, then the steps in Markdown. Hedy keeps only
//! those three; other keys (`allowed-tools`, `hooks`, `model`…) are dropped
//! and reported.

use super::Skill;
use super::skill::{MANAGED_MARKER, yaml_string};

/// What a `SKILL.md` says, before any check.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedSkill {
    pub name: Option<String>,
    pub description: Option<String>,
    pub body: String,
    /// Frontmatter keys Hedy does not keep, in file order.
    pub dropped_keys: Vec<String>,
}

/// Reads a `SKILL.md`. Without frontmatter the whole text is the body.
pub fn parse_skill_md(text: &str) -> ParsedSkill {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut lines = text.lines();
    let Some(front) = frontmatter(&text) else {
        return ParsedSkill {
            body: clean_body(&text),
            ..ParsedSkill::default()
        };
    };
    let mut parsed = ParsedSkill::default();
    for (key, value) in front.entries {
        match key.as_str() {
            "name" => parsed.name = value,
            "description" => parsed.description = value,
            _ if !parsed.dropped_keys.contains(&key) => parsed.dropped_keys.push(key),
            _ => {}
        }
    }
    lines.nth(front.end);
    parsed.body = clean_body(&lines.collect::<Vec<_>>().join("\n"));
    parsed
}

/// The `SKILL.md` Hedy exports: the same as any CLI reads, without Hedy's
/// marker, so the copy is the user's own.
pub fn portable_skill_file(skill: &Skill) -> String {
    format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}\n",
        skill.name,
        yaml_string(&skill.description),
        skill.body.trim()
    )
}

struct Frontmatter {
    entries: Vec<(String, Option<String>)>,
    /// Index of the closing `---` line.
    end: usize,
}

fn frontmatter(text: &str) -> Option<Frontmatter> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first()?.trim_end() != "---" {
        return None;
    }
    let end = lines
        .iter()
        .skip(1)
        .position(|l| matches!(l.trim_end(), "---" | "..."))?
        + 1;
    let mut entries = Vec::new();
    let mut i = 1;
    while i < end {
        let line = lines[i];
        i += 1;
        let top = !line.starts_with([' ', '\t']) && !line.trim_start().starts_with('#');
        let Some((key, rest)) = line.split_once(':').filter(|_| top) else {
            continue;
        };
        // The lines that belong to this key: indented or blank.
        let start = i;
        while i < end && (lines[i].starts_with([' ', '\t']) || lines[i].trim().is_empty()) {
            i += 1;
        }
        let value = scalar(rest.trim(), &lines[start..i]);
        let key = key.trim().trim_matches(['"', '\'']);
        entries.push((key.to_owned(), value));
    }
    Some(Frontmatter { entries, end })
}

/// A YAML scalar: plain, quoted or a block (`|`, `>`). `None` for a nested
/// map or list, `null`, `~` or nothing at all.
fn scalar(first: &str, more: &[&str]) -> Option<String> {
    let mut more: Vec<&str> = more.iter().map(|l| l.trim()).collect();
    // A quoted value may start on the next line.
    if first.is_empty() {
        while more.first().is_some_and(|l| l.is_empty()) {
            more.remove(0);
        }
        if let Some(next) = more.first().filter(|l| l.starts_with(['"', '\''])) {
            return scalar(next, &more[1..]);
        }
    }
    let value = if first.starts_with(['|', '>']) {
        more.join("\n")
    } else if let Some(quoted) = first.strip_prefix('"') {
        // A line ending in `\` goes on without a space.
        let mut joined = quoted.to_owned();
        for line in more {
            match joined.strip_suffix('\\') {
                Some(open) if !open.ends_with('\\') => joined.truncate(open.len()),
                _ => joined.push(' '),
            }
            joined.push_str(line);
        }
        double_quoted(&joined)
    } else if let Some(quoted) = first.strip_prefix('\'') {
        let joined = std::iter::once(quoted)
            .chain(more)
            .collect::<Vec<_>>()
            .join(" ");
        let end = closing_single_quote(&joined);
        joined[..end].replace("''", "'")
    } else if first.is_empty() {
        let nested = more
            .iter()
            .any(|l| l.starts_with("- ") || l.contains(": ") || l.ends_with(':'));
        if nested {
            return None;
        }
        plain(&more.join(" "))?
    } else {
        plain(
            &std::iter::once(first)
                .chain(more)
                .collect::<Vec<_>>()
                .join(" "),
        )?
    };
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// A plain scalar without its comment; `None` for `null` or `~`.
fn plain(text: &str) -> Option<String> {
    let text = match text.find([' ', '\t']).and_then(|_| {
        text.char_indices()
            .find(|(i, c)| *c == '#' && text[..*i].ends_with([' ', '\t']))
    }) {
        Some((comment, _)) => &text[..comment],
        None => text,
    };
    let text = text.trim();
    (!matches!(text, "null" | "Null" | "NULL" | "~")).then(|| text.to_owned())
}

/// A double-quoted scalar's text up to its closing quote, escapes decoded.
fn double_quoted(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some(hex @ ('x' | 'u' | 'U')) => {
                    let digits = match hex {
                        'x' => 2,
                        'u' => 4,
                        _ => 8,
                    };
                    let code: String = chars.by_ref().take(digits).collect();
                    if let Some(c) = u32::from_str_radix(&code, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                Some(other) => out.push(other),
                None => break,
            },
            c => out.push(c),
        }
    }
    out
}

/// Where a single-quoted scalar ends (`''` is a quote inside it).
fn closing_single_quote(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            return i;
        }
        i += 1;
    }
    text.len()
}

/// The body without the lines Hedy adds to the files it writes (a file an
/// older Hedy wrote can be imported back as it was approved).
fn clean_body(body: &str) -> String {
    let mut lines: Vec<&str> = body.lines().collect();
    while let Some(first) = lines.first() {
        let line = first.trim();
        let hedys = line == MANAGED_MARKER
            || (line.starts_with("<!-- Approved in Hedy.") && line.ends_with("-->"));
        if line.is_empty() || hedys {
            lines.remove(0);
        } else {
            break;
        }
    }
    lines.join("\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SkillStatus;
    use crate::domain::skill::skill_file;

    #[test]
    fn reads_name_description_and_body_and_reports_other_keys() {
        let parsed = parse_skill_md(
            "---\nname: pdf-forms\ndescription: Fill PDF forms. Use when the user has a PDF.\nallowed-tools: Read, Bash\nmetadata:\n  version: 2\n---\n\n# PDF forms\n\n1. Run `python fill.py`\n",
        );
        assert_eq!(parsed.name.as_deref(), Some("pdf-forms"));
        assert_eq!(
            parsed.description.as_deref(),
            Some("Fill PDF forms. Use when the user has a PDF.")
        );
        assert_eq!(parsed.body, "# PDF forms\n\n1. Run `python fill.py`");
        assert_eq!(parsed.dropped_keys, ["allowed-tools", "metadata"]);
    }

    #[test]
    fn reads_quoted_folded_and_multi_line_values() {
        let parsed = parse_skill_md(
            "---\r\nname: \"release\"\r\ndescription: >-\r\n  Cut a release.\r\n  Use before tagging.\r\n---\r\nSteps\r\n",
        );
        assert_eq!(parsed.name.as_deref(), Some("release"));
        assert_eq!(
            parsed.description.as_deref(),
            Some("Cut a release.\nUse before tagging.")
        );
        assert_eq!(parsed.body, "Steps");

        let parsed = parse_skill_md(
            "---\nname: 'it''s'\ndescription: \"Say \\\"hi\\\"\" # greeting\n---\nx",
        );
        assert_eq!(parsed.name.as_deref(), Some("it's"));
        assert_eq!(parsed.description.as_deref(), Some("Say \"hi\""));

        let parsed = parse_skill_md("---\ndescription: Lint the\n  code first\t# always\n---\nx");
        assert_eq!(parsed.description.as_deref(), Some("Lint the code first"));

        // As official skills and YAML dumpers write them.
        let parsed = parse_skill_md(
            "---\n\"name\": caf\ndescription:\n  \"Solve \\\"hard\\\" problems,\n  caf\\u00e9 and \\xE9\\\n  t\"\nlicense: ~\n---\nx",
        );
        assert_eq!(parsed.name.as_deref(), Some("caf"));
        assert_eq!(
            parsed.description.as_deref(),
            Some("Solve \"hard\" problems, café and ét")
        );
        assert_eq!(
            parse_skill_md("---\ndescription: null\n---\nx").description,
            None
        );
        assert_eq!(
            parse_skill_md("---\ndescription: Use C# here\n---\nx")
                .description
                .as_deref(),
            Some("Use C# here")
        );
    }

    #[test]
    fn without_frontmatter_everything_is_the_body() {
        let parsed = parse_skill_md("# Deploy\n\n1. make deploy\n");
        assert_eq!(parsed.name, None);
        assert_eq!(parsed.body, "# Deploy\n\n1. make deploy");
        // An unclosed frontmatter is not one.
        assert_eq!(parse_skill_md("---\nname: x\n").name, None);
    }

    #[test]
    fn a_file_hedy_wrote_comes_back_as_it_was_approved_and_exports_without_the_marker() {
        let skill = Skill {
            id: 1,
            project: None,
            name: "run-e2e-tests".into(),
            description: "Run the \"e2e\" tests.".into(),
            body: "1. npm test".into(),
            status: SkillStatus::Approved,
            author: "user".into(),
            created_at: 0,
            updated_at: None,
            source_task: None,
        };
        let back = parse_skill_md(&skill_file(&skill));
        assert_eq!(back.name.as_deref(), Some("run-e2e-tests"));
        assert_eq!(back.description.as_deref(), Some("Run the \"e2e\" tests."));
        assert_eq!(back.body, "1. npm test");

        let exported = portable_skill_file(&skill);
        assert!(!exported.contains(MANAGED_MARKER));
        let again = parse_skill_md(&exported);
        assert_eq!(
            (again.name, again.description, again.body),
            (Some(skill.name), Some(skill.description), skill.body)
        );
    }
}
