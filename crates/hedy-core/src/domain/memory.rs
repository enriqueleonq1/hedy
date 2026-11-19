//! Shared memory: notes every AI of a project reads (spec §4, "Memoria").
//! New notes from an AI are proposals until the user approves them (rule 6).

use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Decision,
    Convention,
    Rule,
    Preference,
}

impl Category {
    pub const ALL: [Category; 4] = [
        Self::Decision,
        Self::Convention,
        Self::Rule,
        Self::Preference,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Convention => "convention",
            Self::Rule => "rule",
            Self::Preference => "preference",
        }
    }
}

impl FromStr for Category {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            // Agents may answer in Spanish.
            .or(match s.as_str() {
                "decisión" | "decision" => Some(Self::Decision),
                "convención" | "convencion" => Some(Self::Convention),
                "regla" => Some(Self::Rule),
                "preferencia" => Some(Self::Preference),
                _ => None,
            })
            .ok_or(DomainError::UnknownCategory(s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteStatus {
    /// Suggested by an AI; invisible to agents until approved.
    Proposed,
    Approved,
}

/// Author of notes written by the user (agents use their label, "Claude Opus").
pub const USER_AUTHOR: &str = "user";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    /// `None`: a global note, for every (non-protected) project.
    pub project: Option<PathBuf>,
    pub text: String,
    pub category: Category,
    pub author: String,
    pub status: NoteStatus,
    pub created_at: i64,
    pub uses: u32,
    pub last_used: Option<i64>,
    /// Stable subject, such as `arquitectura/pagos`. A proposal under the
    /// topic of an approved note is a change to that note.
    pub topic: Option<String>,
    /// When an approved note was last replaced by a newer version.
    pub updated_at: Option<i64>,
}

/// Longest topic, in characters.
const TOPIC_MAX: usize = 80;

pub(crate) fn ascii_fold(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        c => c,
    }
}

/// A topic as a lowercase slug path: `Arquitectura / Pagos` → `arquitectura/pagos`.
/// Accents are dropped and anything else becomes a dash. `None` when `raw`
/// is blank; an error when nothing usable is left or it is too long.
pub fn normalize_topic(raw: &str) -> Result<Option<String>, DomainError> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let slug: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .map(ascii_fold)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '/' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let segments: Vec<String> = slug
        .split('/')
        .map(|segment| {
            segment
                .split('-')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("-")
        })
        .filter(|segment| !segment.is_empty())
        .collect();
    let topic = segments.join("/");
    if topic.is_empty() || topic.chars().count() > TOPIC_MAX {
        return Err(DomainError::InvalidTopic(raw.trim().to_owned()));
    }
    Ok(Some(topic))
}

/// The first line of a note, cut to `max` characters; `true` when something
/// was left out (a later line or the end of the first one).
pub fn preview(text: &str, max: usize) -> (String, bool) {
    let text = text.trim();
    let first = text.lines().next().unwrap_or("").trim();
    let more_lines = first.len() < text.len();
    if first.chars().count() <= max {
        return (first.to_owned(), more_lines);
    }
    let cut: String = first.chars().take(max.saturating_sub(1)).collect();
    (format!("{}…", cut.trim_end()), true)
}

/// Notes unused for this long are listed "para revisar".
const STALE_AFTER_SECS: i64 = 60 * 24 * 60 * 60;

pub fn is_stale(note: &Note, now: i64) -> bool {
    note.status == NoteStatus::Approved
        && now - note.last_used.unwrap_or(note.created_at) > STALE_AFTER_SECS
}

const NEGATIONS: &[&str] = &[
    "no",
    "nunca",
    "jamás",
    "evitar",
    "evita",
    "sin",
    "prohibido",
    "never",
    "not",
    "avoid",
];
const STOPWORDS: &[&str] = &[
    "el", "la", "los", "las", "un", "una", "de", "del", "en", "y", "o", "a", "al", "se", "que",
    "con", "por", "para", "es", "son", "su", "sus", "lo", "the", "a", "of", "to", "and", "in",
    "is",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '.')
        .map(|w| w.trim_matches('.').to_owned())
        .filter(|w| !w.is_empty())
        .collect()
}

fn content_words(text: &str) -> HashSet<String> {
    words(text)
        .into_iter()
        .filter(|w| !STOPWORDS.contains(&w.as_str()) && !NEGATIONS.contains(&w.as_str()))
        .collect()
}

fn negated(text: &str) -> bool {
    words(text).iter().any(|w| NEGATIONS.contains(&w.as_str()))
}

fn has_number(words: &HashSet<String>) -> bool {
    words.iter().any(|w| w.chars().any(|c| c.is_ascii_digit()))
}

/// Whether two notes look like they say opposite things about the same subject.
///
/// A local heuristic, no AI involved: the notes share most of their words and
/// either one is negated and the other not ("usar X" / "no usar X"), or they
/// differ only in a number ("24 h antes" / "2 h antes").
fn contradicts(a: &str, b: &str) -> bool {
    let (wa, wb) = (content_words(a), content_words(b));
    let common = wa.intersection(&wb).count();
    let union = wa.union(&wb).count();
    if union == 0 || common < 2 {
        return false;
    }
    let similar = common as f64 / union as f64 >= 0.5;
    let negation = negated(a) != negated(b);
    let only_numbers_differ = wa
        .symmetric_difference(&wb)
        .all(|w| w.chars().any(|c| c.is_ascii_digit()))
        && has_number(&wa)
        && has_number(&wb)
        && wa != wb;
    similar && (negation || only_numbers_differ)
}

/// Words that carry a note's meaning, folded so that small differences
/// (accents, a plural "s") do not matter: what similar notes are compared by,
/// and what the full-text index is searched with.
pub fn key_words(text: &str) -> Vec<String> {
    let mut out: HashSet<String> = HashSet::new();
    for word in content_words(text) {
        let mut word: String = word.chars().map(ascii_fold).collect();
        if word.chars().count() > 3 && word.ends_with('s') {
            word.pop();
        }
        if word.chars().count() >= 2 {
            out.insert(word);
        }
    }
    let mut out: Vec<String> = out.into_iter().collect();
    out.sort();
    out
}

/// Whether two notes are about the same thing, by their key words: they
/// share at least two, and at least half of the shorter note's. Only a
/// suggestion for the user ("parecida a"), never a claim that they disagree.
pub fn similar(a: &str, b: &str) -> bool {
    let (wa, wb) = (key_words(a), key_words(b));
    // Like the full-text search, a word matches its longer forms
    // ("guarda" / "guardan").
    let matches = |x: &String, y: &String| {
        x == y
            || (x.len().min(y.len()) >= 4
                && (x.starts_with(y.as_str()) || y.starts_with(x.as_str())))
    };
    let common = wa
        .iter()
        .filter(|x| wb.iter().any(|y| matches(x, y)))
        .count();
    let shorter = wa.len().min(wb.len());
    common >= 2 && common * 2 >= shorter
}

/// Pairs of notes (ids, lower first) that contradict each other, skipping
/// pairs the user already said are both valid.
pub fn contradictions(notes: &[Note], dismissed: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut pairs = Vec::new();
    for (i, a) in notes.iter().enumerate() {
        for b in &notes[i + 1..] {
            let pair = (a.id.min(b.id), a.id.max(b.id));
            if !dismissed.contains(&pair) && contradicts(&a.text, &b.text) {
                pairs.push(pair);
            }
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: i64, text: &str, last_used: Option<i64>) -> Note {
        Note {
            id,
            project: None,
            text: text.into(),
            category: Category::Decision,
            author: USER_AUTHOR.into(),
            status: NoteStatus::Approved,
            created_at: 0,
            uses: 0,
            last_used,
            topic: None,
            updated_at: None,
        }
    }

    #[test]
    fn detects_contradictions() {
        assert!(contradicts(
            "Los recordatorios se envían 24 h antes de la cita.",
            "Los recordatorios se envían 2 h antes de la cita."
        ));
        assert!(contradicts(
            "Usar Twilio para WhatsApp",
            "No usar Twilio para WhatsApp"
        ));
        assert!(!contradicts(
            "Tests con pytest",
            "Los precios se guardan en USD"
        ));
        assert!(!contradicts("Precios en USD", "Precios en USD"));
        assert!(!contradicts(
            "Las citas usan la zona horaria del salón",
            "Los recordatorios se envían por WhatsApp"
        ));
    }

    #[test]
    fn lists_pairs_except_dismissed() {
        let notes = [
            note(3, "Deploy los viernes", None),
            note(1, "Nunca deploy los viernes", None),
            note(2, "Precios en USD", None),
        ];
        assert_eq!(contradictions(&notes, &[]), vec![(1, 3)]);
        assert!(contradictions(&notes, &[(1, 3)]).is_empty());
    }

    #[test]
    fn stale_after_sixty_days_without_use() {
        let day = 24 * 60 * 60;
        assert!(is_stale(&note(1, "x", None), 61 * day));
        assert!(!is_stale(&note(1, "x", Some(50 * day)), 61 * day));
        let mut proposed = note(1, "x", None);
        proposed.status = NoteStatus::Proposed;
        assert!(!is_stale(&proposed, 100 * day));
    }

    #[test]
    fn topics_are_slug_paths() {
        let topic = |raw: &str| normalize_topic(raw).unwrap();
        assert_eq!(
            topic("arquitectura/pagos").as_deref(),
            Some("arquitectura/pagos")
        );
        assert_eq!(
            topic(" Arquitectura / Pagos con Tarjeta ").as_deref(),
            Some("arquitectura/pagos-con-tarjeta")
        );
        assert_eq!(
            topic("Decisión_técnica//Envíos").as_deref(),
            Some("decision-tecnica/envios")
        );
        assert_eq!(topic("   "), None);
        assert!(matches!(
            normalize_topic("¿?/¡!"),
            Err(DomainError::InvalidTopic(_))
        ));
        assert!(normalize_topic(&"a".repeat(81)).is_err());
    }

    #[test]
    fn previews_cut_the_first_line() {
        assert_eq!(
            preview("Precios en USD", 40),
            ("Precios en USD".into(), false)
        );
        assert_eq!(
            preview("Precios en USD\nY en Bs", 40),
            ("Precios en USD".into(), true)
        );
        assert_eq!(preview("uno dos tres cuatro", 9), ("uno dos…".into(), true));
    }

    #[test]
    fn similar_notes_share_their_key_words() {
        assert!(similar(
            "Los precios se guardan en USD",
            "Los precios se guardan en USD y se muestran también en Bs a tasa BCV"
        ));
        assert!(similar(
            "Los recordatorios se envían por WhatsApp",
            "El recordatorio se envía por WhatsApp y SMS"
        ));
        assert!(similar(
            "El precio se guarda siempre en dólares USD",
            "Los precios se guardan en USD y se muestran también en Bs"
        ));
        assert!(!similar(
            "Tests con pytest",
            "Los precios se guardan en USD"
        ));
        assert!(!similar(
            "Los formularios validan con zod",
            "Las citas usan la zona horaria del salón"
        ));
        assert_eq!(key_words("Los Precios en USD"), ["precio", "usd"]);
    }

    #[test]
    fn categories_in_both_languages() {
        assert_eq!("Decisión".parse::<Category>().unwrap(), Category::Decision);
        assert_eq!(
            "convention".parse::<Category>().unwrap(),
            Category::Convention
        );
        assert!("otra".parse::<Category>().is_err());
    }
}
