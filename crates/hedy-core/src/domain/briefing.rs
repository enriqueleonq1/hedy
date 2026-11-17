//! The `briefing` experiment (docs/decisions.md §33): a few lines Hedy adds
//! to the first prompt of a project task, built without AI, so the agent
//! starts knowing where to look instead of exploring:
//! - the files that mention what the request names (identifiers, file names);
//! - the approved notes that match the request;
//! - how the project runs its tests.
//!
//! It stays short (a few hundred tokens at most): it only points, it never
//! pastes code.

use crate::tr;

/// Names taken from a request, at most.
pub const MAX_TERMS: usize = 6;
/// Files listed, at most.
pub const MAX_FILES: usize = 8;
/// Notes inlined, at most.
pub const MAX_NOTES: usize = 3;
/// Characters of each note.
const NOTE_CHARS: usize = 200;

/// What Hedy found for a request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Briefing {
    /// Files that mention the request's names, most matches first: (path,
    /// names it mentions, matching lines).
    pub files: Vec<(String, Vec<String>, usize)>,
    /// Approved notes that match the request, best first.
    pub notes: Vec<String>,
    pub test_command: Option<String>,
}

impl Briefing {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.notes.is_empty() && self.test_command.is_none()
    }

    /// The text appended to the prompt; empty when there is nothing to say.
    pub fn render(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut lines = vec![tr!(
            "Contexto que preparó Hedy (búsqueda local; puede estar incompleto):",
            "Context prepared by Hedy (local search; it may be incomplete):"
        )];
        if !self.files.is_empty() {
            lines.push(tr!(
                "- Archivos que mencionan lo que pides:",
                "- Files that mention what you ask about:"
            ));
            for (path, names, _) in &self.files {
                lines.push(format!("  - {path} ({})", names.join(", ")));
            }
        }
        if !self.notes.is_empty() {
            lines.push(tr!(
                "- Notas de la memoria del proyecto:",
                "- Notes from the project's memory:"
            ));
            for note in &self.notes {
                lines.push(format!("  - {}", clip(note, NOTE_CHARS)));
            }
        }
        if let Some(command) = &self.test_command {
            lines.push(tr!("- Tests: `{command}`", "- Tests: `{command}`"));
        }
        lines.join("\n")
    }
}

fn clip(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

/// The names a request mentions: identifiers (`business_days`, `formatPrice`,
/// `Product.price_cents`) and file paths (`shop/catalog.py`), in order, at
/// most [`MAX_TERMS`]. Plain words are left out: they match everywhere.
pub fn search_terms(prompt: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for raw in prompt.split(|c: char| c.is_whitespace() || "`'\"«»()[]{},;:¿?¡!".contains(c)) {
        let word = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if word.chars().count() < 3 || !word.is_ascii() {
            continue;
        }
        let snake = word.contains('_');
        let path = word.contains('/') || has_extension(word);
        let camel = word.chars().next().is_some_and(char::is_lowercase)
            && word.chars().any(char::is_uppercase);
        if (snake || path || camel) && !terms.iter().any(|t| t == word) {
            terms.push(word.to_owned());
        }
        if terms.len() == MAX_TERMS {
            break;
        }
    }
    terms
}

/// `catalog.py`, `README.md`: a dot followed by a short extension.
fn has_extension(word: &str) -> bool {
    word.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=4).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}

/// How the project runs its tests, guessed from its files.
pub fn test_command(files: &[String]) -> Option<String> {
    let has = |name: &str| files.iter().any(|f| f == name);
    let any = |pred: &dyn Fn(&str) -> bool| files.iter().any(|f| pred(f));
    let command = if has("Cargo.toml") {
        "cargo test"
    } else if has("package.json") {
        "npm test"
    } else if has("go.mod") {
        "go test ./..."
    } else if has("pytest.ini") || any(&|f| f.ends_with("conftest.py")) {
        "python3 -m pytest -q"
    } else if any(&|f| {
        let name = f.rsplit('/').next().unwrap_or(f);
        name.starts_with("test_") && name.ends_with(".py")
    }) {
        "python3 -m unittest discover -q"
    } else {
        return None;
    };
    Some(command.to_owned())
}

/// Adds up search hits per file: `hits` are (term, file) pairs, one per
/// matching line. Most lines first, then by path; at most [`MAX_FILES`].
pub fn rank_files(hits: &[(String, String)]) -> Vec<(String, Vec<String>, usize)> {
    let mut files: Vec<(String, Vec<String>, usize)> = Vec::new();
    for (term, path) in hits {
        match files.iter_mut().find(|(p, _, _)| p == path) {
            Some((_, terms, count)) => {
                *count += 1;
                if !terms.contains(term) {
                    terms.push(term.clone());
                }
            }
            None => files.push((path.clone(), vec![term.clone()], 1)),
        }
    }
    // Files that mention more of the names first, then more lines.
    files.sort_by(|a, b| {
        b.1.len()
            .cmp(&a.1.len())
            .then(b.2.cmp(&a.2))
            .then(a.0.cmp(&b.0))
    });
    files.truncate(MAX_FILES);
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    #[test]
    fn takes_identifiers_and_paths_not_plain_words() {
        assert_eq!(
            search_terms(
                "business_days en calendar_utils.py da un día menos: del lunes 3 al viernes 7."
            ),
            ["business_days", "calendar_utils.py"]
        );
        assert_eq!(
            search_terms("Agrega en shop/catalog.py una función format_price(product)."),
            ["shop/catalog.py", "format_price"]
        );
        assert_eq!(
            search_terms("Renombra `customerPhone` y customer_phone; otra vez customer_phone"),
            ["customerPhone", "customer_phone"]
        );
        assert!(search_terms("Arregla el total del carrito, por favor.").is_empty());
        assert!(search_terms("versión 1.2.0 y 3.14").is_empty());
        let many = "a_1 b_2 c_3 d_4 e_5 f_6 g_7 h_8";
        assert_eq!(search_terms(many).len(), MAX_TERMS);
    }

    #[test]
    fn guesses_the_test_command() {
        let files = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            test_command(&files(&["Cargo.toml", "src/lib.rs"])).as_deref(),
            Some("cargo test")
        );
        assert_eq!(
            test_command(&files(&["shop/a.py", "tests/test_shop.py"])).as_deref(),
            Some("python3 -m unittest discover -q")
        );
        assert_eq!(
            test_command(&files(&["tests/conftest.py", "tests/test_a.py"])).as_deref(),
            Some("python3 -m pytest -q")
        );
        assert_eq!(test_command(&files(&["README.md"])), None);
    }

    #[test]
    fn ranks_files_by_names_then_lines() {
        let hit = |t: &str, p: &str| (t.to_owned(), p.to_owned());
        let ranked = rank_files(&[
            hit("a_b", "x.py"),
            hit("a_b", "x.py"),
            hit("a_b", "x.py"),
            hit("a_b", "y.py"),
            hit("c_d", "y.py"),
        ]);
        assert_eq!(
            ranked[0],
            ("y.py".into(), vec!["a_b".into(), "c_d".into()], 2)
        );
        assert_eq!(ranked[1], ("x.py".into(), vec!["a_b".into()], 3));
    }

    #[test]
    fn renders_a_short_briefing_or_nothing() {
        assert_eq!(Briefing::default().render(), "");
        let briefing = Briefing {
            files: vec![("shop/catalog.py".into(), vec!["format_price".into()], 2)],
            notes: vec!["Los precios se muestran como «USD 12.50».".into()],
            test_command: Some("python3 -m unittest discover -q".into()),
        };
        let text = with_language(Language::Spanish, || briefing.render());
        assert!(text.starts_with("Contexto que preparó Hedy"));
        assert!(text.contains("  - shop/catalog.py (format_price)"));
        assert!(text.contains("  - Los precios se muestran como «USD 12.50»."));
        assert!(text.ends_with("- Tests: `python3 -m unittest discover -q`"));
        assert_eq!(clip(&"x".repeat(300), 10), format!("{}…", "x".repeat(10)));
    }
}
