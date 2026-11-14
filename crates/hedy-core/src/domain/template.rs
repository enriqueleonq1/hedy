//! Prompt templates: reusable requests for the composer, global or for one
//! project. A template may have placeholders written `{name}` (letters,
//! digits, `_` or `-`, no spaces) that the user fills before sending:
//! "Escribe pruebas para {archivo}".

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::DomainError;
use crate::tr;

/// Longest template name.
pub const MAX_NAME: usize = 60;
/// Longest template text.
pub const MAX_TEXT: usize = 20_000;
/// Longest placeholder name.
const MAX_PLACEHOLDER: usize = 40;

/// A built-in example (`builtin:tests`) or a template saved in the database.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum TemplateId {
    Builtin(String),
    Saved(i64),
}

const BUILTIN_PREFIX: &str = "builtin:";

impl fmt::Display for TemplateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(key) => write!(f, "{BUILTIN_PREFIX}{key}"),
            Self::Saved(id) => write!(f, "{id}"),
        }
    }
}

impl FromStr for TemplateId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(key) = value.strip_prefix(BUILTIN_PREFIX)
            && builtin(key).is_some()
        {
            return Ok(Self::Builtin(key.to_owned()));
        }
        value
            .parse()
            .map(Self::Saved)
            .map_err(|_| DomainError::InvalidTemplate(value.to_owned()))
    }
}

impl From<TemplateId> for String {
    fn from(id: TemplateId) -> Self {
        id.to_string()
    }
}

impl TryFrom<String> for TemplateId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub id: TemplateId,
    /// `None`: every project.
    pub project: Option<PathBuf>,
    pub name: String,
    pub text: String,
    /// The built-in example this is (edited or not), so it can be restored.
    pub builtin: Option<String>,
}

impl PromptTemplate {
    pub fn placeholders(&self) -> Vec<String> {
        placeholders(&self.text)
    }
}

/// The built-in examples: generic requests, in the current language.
/// `(key, name, text)`.
pub fn builtins() -> Vec<(&'static str, String, String)> {
    vec![
        (
            "explain",
            tr!("Explicar código", "Explain code"),
            tr!(
                "Explica qué hace {{archivo}}, cómo encaja en el resto del proyecto y qué partes conviene tocar con cuidado.",
                "Explain what {{file}} does, how it fits in the rest of the project and which parts need care."
            ),
        ),
        (
            "tests",
            tr!("Escribir pruebas", "Write tests"),
            tr!(
                "Escribe pruebas para {{archivo}}. Cubre los casos normales y los bordes, y ejecútalas al terminar.",
                "Write tests for {{file}}. Cover the usual cases and the edge cases, and run them when you finish."
            ),
        ),
        (
            "review",
            tr!("Revisar el cambio", "Review the change"),
            tr!(
                "Revisa los cambios de esta tarea como lo haría un compañero: errores, casos sin cubrir y nombres poco claros. Propón arreglos concretos.",
                "Review the changes in this task as a teammate would: bugs, uncovered cases and unclear names. Suggest concrete fixes."
            ),
        ),
        (
            "fix",
            tr!("Corregir un error", "Fix an error"),
            tr!(
                "Aparece este error: {{error}}. Encuentra la causa, corrígela y explica qué pasaba.",
                "This error shows up: {{error}}. Find the cause, fix it and explain what was going on."
            ),
        ),
        (
            "summary",
            tr!("Resumir lo hecho", "Summarize the work"),
            tr!(
                "Resume lo que hiciste: qué cambiaste, por qué y qué queda pendiente.",
                "Summarize what you did: what you changed, why, and what is left to do."
            ),
        ),
    ]
}

/// The built-in example with this key: `(name, text)`.
pub fn builtin(key: &str) -> Option<(String, String)> {
    builtins()
        .into_iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, name, text)| (name, text))
}

fn is_placeholder_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// The placeholders of `text`, each once, in the order they first appear.
/// `{archivo}` is one; `{ }`, `{"a": 1}` or `{}` are plain text.
pub fn placeholders(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                let valid = !name.is_empty()
                    && name.chars().count() <= MAX_PLACEHOLDER
                    && name.chars().all(is_placeholder_char);
                if valid {
                    if !found.iter().any(|f| f == name) {
                        found.push(name.to_owned());
                    }
                    rest = &after[end + 1..];
                } else {
                    rest = after;
                }
            }
            None => break,
        }
    }
    found
}

/// A trimmed, valid template name.
pub fn validate_name(name: &str) -> Result<String, DomainError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME || name.contains(['\n', '\r']) {
        return Err(DomainError::InvalidTemplateName(name.to_owned()));
    }
    Ok(name.to_owned())
}

/// A valid template text (kept as written, only trimmed at the ends).
pub fn validate_text(text: &str) -> Result<String, DomainError> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_TEXT {
        return Err(DomainError::EmptyTemplate);
    }
    Ok(text.to_owned())
}

/// Names are compared without case: "Revisar" and "revisar" are the same.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    #[test]
    fn finds_each_placeholder_once_in_order() {
        assert_eq!(
            placeholders("Pruebas para {archivo} con {marco_de-tests}; {archivo} otra vez"),
            vec!["archivo", "marco_de-tests"]
        );
        assert_eq!(placeholders("Explica {descripción}"), vec!["descripción"]);
    }

    #[test]
    fn braces_that_are_not_placeholders_stay_text() {
        assert!(placeholders(r#"Devuelve {"a": 1} y { } y {} y {abierto"#).is_empty());
        assert_eq!(placeholders("{{x}}"), vec!["x"]);
        assert!(placeholders(&format!("{{{}}}", "a".repeat(41))).is_empty());
    }

    #[test]
    fn template_ids_round_trip_as_text() {
        let builtin: TemplateId = "builtin:tests".parse().unwrap();
        assert_eq!(builtin, TemplateId::Builtin("tests".into()));
        assert_eq!(builtin.to_string(), "builtin:tests");
        assert_eq!("12".parse::<TemplateId>().unwrap(), TemplateId::Saved(12));
        assert!("builtin:nada".parse::<TemplateId>().is_err());
        assert!("x".parse::<TemplateId>().is_err());
        let json = serde_json::to_string(&TemplateId::Saved(3)).unwrap();
        assert_eq!(json, "\"3\"");
    }

    #[test]
    fn builtins_exist_in_both_languages_with_the_same_placeholders() {
        let spanish = builtins();
        let english = with_language(Language::English, builtins);
        assert!((3..=5).contains(&spanish.len()));
        for ((key, es_name, es_text), (_, en_name, en_text)) in spanish.iter().zip(&english) {
            assert_ne!(es_name, en_name, "{key}");
            assert_eq!(
                placeholders(es_text).len(),
                placeholders(en_text).len(),
                "{key}"
            );
        }
        assert_eq!(placeholders(&spanish[1].2), vec!["archivo"]);
        assert_eq!(placeholders(&english[1].2), vec!["file"]);
    }

    #[test]
    fn names_and_texts_are_validated() {
        assert_eq!(validate_name("  Revisar  ").unwrap(), "Revisar");
        assert!(validate_name(" ").is_err());
        assert!(validate_name("a\nb").is_err());
        assert!(validate_name(&"x".repeat(61)).is_err());
        assert!(validate_text("  ").is_err());
        assert!(same_name("Revisar", " revisar"));
    }
}
