//! Custom color themes for the app: imported from a VS Code theme, described
//! to an AI ("Describir con IA") or edited by hand. The UI derives every
//! other token from these colors and fixes contrast; the core only keeps
//! them well formed.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::DomainError;
use super::account::fold_accent;

/// Longest theme name (in characters).
pub const MAX_NAME: usize = 60;
/// Most colors a theme may hold.
pub const MAX_COLORS: usize = 160;
/// Longest color key.
const MAX_KEY: usize = 40;
/// Longest theme id.
pub const MAX_ID: usize = 40;
/// Longest description sent to the AI (in characters).
pub const MAX_DESCRIPTION: usize = 500;

/// The colors an AI answers with; the UI derives the rest from them.
pub const SEED_KEYS: [&str; 18] = [
    "bg", "panel", "panel2", "ink", "muted", "line", "accent", "action", "success", "warning",
    "danger", "info", "keyword", "string", "number", "function", "type", "comment",
];

/// The seeds a draft cannot do without.
pub const REQUIRED_SEEDS: [&str; 3] = ["bg", "ink", "accent"];

/// Below this relative luminance a background reads as dark (the point where
/// white and black text have the same contrast on it).
const DARK_LUMINANCE: f64 = 0.179;

/// Whether the theme is meant for dark or light mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeBase {
    Dark,
    Light,
}

impl ThemeBase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// The base a background color asks for.
    pub fn of_background(hex: &str) -> Option<Self> {
        relative_luminance(hex).map(|l| {
            if l < DARK_LUMINANCE {
                Self::Dark
            } else {
                Self::Light
            }
        })
    }
}

impl fmt::Display for ThemeBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ThemeBase {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            _ => Err(DomainError::InvalidThemeBase(s.to_owned())),
        }
    }
}

/// Where a custom theme came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeOrigin {
    /// Imported from a VS Code theme file.
    Vscode,
    /// Described to an AI.
    Described,
    /// Made or changed by hand.
    Edited,
}

/// A theme the user saved. Sent to the UI as is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub base: ThemeBase,
    /// Color key (`bg`, `ink`, `keyword`...) to `#RRGGBB` or `#RRGGBBAA`.
    pub colors: BTreeMap<String, String>,
    pub origin: ThemeOrigin,
}

impl CustomTheme {
    /// A valid theme, with its name trimmed and its colors normalized.
    pub fn new(
        id: impl Into<String>,
        name: &str,
        base: ThemeBase,
        colors: BTreeMap<String, String>,
        origin: ThemeOrigin,
    ) -> Result<Self, DomainError> {
        Self {
            id: id.into(),
            name: name.to_owned(),
            base,
            colors,
            origin,
        }
        .validate()
    }

    /// Checks every rule and normalizes what it can (trimmed name, uppercase
    /// `#RRGGBB[AA]` colors).
    pub fn validate(self) -> Result<Self, DomainError> {
        if !is_theme_id(&self.id) {
            return Err(DomainError::InvalidThemeId(self.id));
        }
        Ok(Self {
            name: theme_name(&self.name)?,
            colors: theme_colors(self.colors)?,
            ..self
        })
    }
}

/// A theme proposed by an AI from a description; the user reviews and saves
/// it (it is not stored).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeDraft {
    pub name: String,
    pub base: ThemeBase,
    /// Only [`SEED_KEYS`], normalized; always with [`REQUIRED_SEEDS`].
    pub colors: BTreeMap<String, String>,
}

/// A trimmed name of 1 to [`MAX_NAME`] characters.
pub fn theme_name(name: &str) -> Result<String, DomainError> {
    let name = name.trim();
    let count = name.chars().count();
    if count == 0 || count > MAX_NAME {
        return Err(DomainError::InvalidThemeName(name.to_owned()));
    }
    Ok(name.to_owned())
}

/// 1 to [`MAX_COLORS`] valid entries, normalized.
fn theme_colors(colors: BTreeMap<String, String>) -> Result<BTreeMap<String, String>, DomainError> {
    if colors.is_empty() || colors.len() > MAX_COLORS {
        return Err(DomainError::ThemeColorCount {
            count: colors.len(),
            max: MAX_COLORS,
        });
    }
    colors
        .into_iter()
        .map(|(key, value)| {
            if !is_color_key(&key) {
                return Err(DomainError::InvalidThemeColorKey(key));
            }
            match normalize_hex(&value) {
                Some(hex) => Ok((key, hex)),
                None => Err(DomainError::InvalidThemeColor { key, value }),
            }
        })
        .collect()
}

/// A letter followed by up to 39 letters or digits (`bg`, `panel2`,
/// `syntaxKeyword`).
pub fn is_color_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && key.len() <= MAX_KEY
        && chars.all(|c| c.is_ascii_alphanumeric())
}

/// `#RGB`, `#RRGGBB` or `#RRGGBBAA` (any case) as uppercase `#RRGGBB[AA]`.
pub fn normalize_hex(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_uppercase();
    match hex.len() {
        3 => Some(format!(
            "#{}",
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        )),
        6 | 8 => Some(format!("#{hex}")),
        _ => None,
    }
}

/// WCAG relative luminance (0 black – 1 white) of a hex color; alpha ignored.
pub fn relative_luminance(hex: &str) -> Option<f64> {
    let hex = normalize_hex(hex)?;
    let channel = |i: usize| {
        let c = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).ok()?) / 255.0;
        Some(if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        })
    };
    Some(0.2126 * channel(1)? + 0.7152 * channel(3)? + 0.0722 * channel(5)?)
}

/// 1 to [`MAX_ID`] lowercase letters, digits or dashes.
pub fn is_theme_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// An id for a new theme: the slug of its name ("One Dark Pro" →
/// `one-dark-pro`), with `-2`, `-3`... when `taken`.
pub fn new_theme_id(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut slug = String::with_capacity(name.len());
    for c in name
        .trim()
        .chars()
        .map(|c| fold_accent(c).to_ascii_lowercase())
    {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = match slug.trim_matches('-') {
        "" => "theme".to_owned(),
        s => s.to_owned(),
    };
    let fit = |base: &str, room: usize| -> String {
        let cut: String = base.chars().take(room).collect();
        cut.trim_end_matches('-').to_owned()
    };
    let first = fit(&slug, MAX_ID);
    if !taken(&first) {
        return first;
    }
    (2u32..)
        .map(|n| {
            let suffix = format!("-{n}");
            format!("{}{suffix}", fit(&slug, MAX_ID - suffix.len()))
        })
        .find(|id| !taken(id))
        .expect("some suffix is free")
}

/// The description to send: trimmed, 1 to [`MAX_DESCRIPTION`] characters.
pub fn theme_description(description: &str) -> Result<String, DomainError> {
    let description = description.trim();
    let count = description.chars().count();
    if count == 0 || count > MAX_DESCRIPTION {
        return Err(DomainError::InvalidThemeDescription {
            max: MAX_DESCRIPTION,
        });
    }
    Ok(description.to_owned())
}

/// Reads an AI's answer: the first JSON object in it (bare, in prose or in a
/// ```` ```json ```` fence) with `name`, `base` and the [`SEED_KEYS`] at the
/// top level or under `colors`. Unknown keys and bad colors are dropped; it
/// needs at least [`REQUIRED_SEEDS`]. `wanted` (if any) wins over the
/// answer's base; without either, the background decides. `None`: unusable.
pub fn parse_theme_answer(
    answer: &str,
    description: &str,
    wanted: Option<ThemeBase>,
) -> Option<ThemeDraft> {
    let object = first_json_object(answer)?;
    let mut colors = BTreeMap::new();
    let nested = object.get("colors").and_then(Value::as_object);
    for source in [Some(&object), nested].into_iter().flatten() {
        for (key, value) in source {
            let Some(seed) = SEED_KEYS.iter().find(|s| s.eq_ignore_ascii_case(key)) else {
                continue;
            };
            if let Some(hex) = value.as_str().and_then(normalize_hex) {
                // Later sources (`colors`) win over the top level.
                colors.insert((*seed).to_owned(), hex);
            }
        }
    }
    if !REQUIRED_SEEDS.iter().all(|k| colors.contains_key(*k)) {
        return None;
    }
    let base = wanted
        .or_else(|| {
            object
                .get("base")
                .and_then(Value::as_str)
                .and_then(|b| b.parse().ok())
        })
        .or_else(|| ThemeBase::of_background(&colors["bg"]))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .and_then(short_name)
        .or_else(|| short_name(description))
        .unwrap_or_else(|| "Theme".to_owned());
    Some(ThemeDraft { name, base, colors })
}

/// The first line of `text`, trimmed and cut to [`MAX_NAME`] characters.
fn short_name(text: &str) -> Option<String> {
    let line = text.trim().lines().next()?.trim();
    let cut: String = line.chars().take(MAX_NAME).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_owned())
}

/// The first `{...}` in `text` that parses as a JSON object.
fn first_json_object(text: &str) -> Option<Map<String, Value>> {
    text.char_indices()
        .filter(|&(_, c)| c == '{')
        .find_map(|(start, _)| {
            let end = matching_brace(&text[start..])?;
            match serde_json::from_str(&text[start..start + end]) {
                Ok(Value::Object(map)) => Some(map),
                _ => None,
            }
        })
}

/// The length of the balanced `{...}` that `text` starts with (braces inside
/// JSON strings do not count).
fn matching_brace(text: &str) -> Option<usize> {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for (i, c) in text.char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn hex_colors_are_normalized() {
        assert_eq!(normalize_hex("#abc").as_deref(), Some("#AABBCC"));
        assert_eq!(normalize_hex(" #1a1B2c ").as_deref(), Some("#1A1B2C"));
        assert_eq!(normalize_hex("#1a1b2c80").as_deref(), Some("#1A1B2C80"));
        for bad in [
            "1a1b2c", "#12", "#1234", "#12345", "#1234567", "#GGGGGG", "red", "",
        ] {
            assert_eq!(normalize_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn color_keys() {
        for good in ["bg", "panel2", "syntaxKeyword", &"a".repeat(40)] {
            assert!(is_color_key(good), "{good}");
        }
        for bad in [
            "",
            "2bg",
            "bg-color",
            "bg_color",
            "fondo ñ",
            &"a".repeat(41),
        ] {
            assert!(!is_color_key(bad), "{bad}");
        }
    }

    #[test]
    fn a_theme_is_validated_and_normalized() {
        let theme = CustomTheme::new(
            "noche",
            "  Noche  ",
            ThemeBase::Dark,
            colors(&[("bg", "#111"), ("ink", "#eeeeee")]),
            ThemeOrigin::Edited,
        )
        .unwrap();
        assert_eq!(theme.name, "Noche");
        assert_eq!(theme.colors["bg"], "#111111");
        assert_eq!(theme.colors["ink"], "#EEEEEE");

        let make = |id: &str, name: &str, colors| {
            CustomTheme::new(id, name, ThemeBase::Dark, colors, ThemeOrigin::Edited)
        };
        let ok = || colors(&[("bg", "#000")]);
        assert!(matches!(
            make("noche", "   ", ok()),
            Err(DomainError::InvalidThemeName(_))
        ));
        assert!(make("noche", &"n".repeat(60), ok()).is_ok());
        assert!(make("noche", &"n".repeat(61), ok()).is_err());
        assert!(matches!(
            make("Noche", "Noche", ok()),
            Err(DomainError::InvalidThemeId(_))
        ));
        assert!(make(&"a".repeat(41), "Noche", ok()).is_err());
        assert!(matches!(
            make("noche", "Noche", BTreeMap::new()),
            Err(DomainError::ThemeColorCount { count: 0, .. })
        ));
        let many = (0..161)
            .map(|i| (format!("c{i}"), "#000".to_owned()))
            .collect();
        assert!(matches!(
            make("noche", "Noche", many),
            Err(DomainError::ThemeColorCount { count: 161, .. })
        ));
        assert!(matches!(
            make("noche", "Noche", colors(&[("bg-x", "#000")])),
            Err(DomainError::InvalidThemeColorKey(_))
        ));
        assert!(matches!(
            make("noche", "Noche", colors(&[("bg", "black")])),
            Err(DomainError::InvalidThemeColor { .. })
        ));
    }

    #[test]
    fn json_shape() {
        let theme = CustomTheme::new(
            "noche",
            "Noche",
            ThemeBase::Light,
            colors(&[("bg", "#fff")]),
            ThemeOrigin::Vscode,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&theme).unwrap(),
            serde_json::json!({
                "id": "noche",
                "name": "Noche",
                "base": "light",
                "colors": { "bg": "#FFFFFF" },
                "origin": "vscode",
            })
        );
    }

    #[test]
    fn ids_come_from_the_name() {
        let none = |_: &str| false;
        assert_eq!(new_theme_id("One Dark Pro", none), "one-dark-pro");
        assert_eq!(new_theme_id("  Café   Nocturno! ", none), "cafe-nocturno");
        assert_eq!(new_theme_id("***", none), "theme");
        assert_eq!(new_theme_id(&"a".repeat(50), none), "a".repeat(40));
        let taken = |id: &str| ["dracula", "dracula-2"].contains(&id);
        assert_eq!(new_theme_id("Dracula", taken), "dracula-3");
        let long = "b".repeat(50);
        let id = new_theme_id(&long, |id| id == "b".repeat(40));
        assert_eq!(id, format!("{}-2", "b".repeat(38)));
        assert!(is_theme_id(&id));
    }

    #[test]
    fn descriptions_are_checked() {
        assert_eq!(theme_description("  calm forest ").unwrap(), "calm forest");
        assert!(theme_description("   ").is_err());
        assert!(theme_description(&"x".repeat(500)).is_ok());
        assert!(theme_description(&"x".repeat(501)).is_err());
    }

    const SEEDS: &str = r##"{
        "name": "Bosque",
        "base": "dark",
        "bg": "#1b2b22", "panel": "#22332a", "panel2": "#2a3b32", "ink": "#e8f0ea",
        "muted": "#9fb3a6", "line": "#34483c", "accent": "#7fd1a0", "action": "#2f8f5b",
        "success": "#7fd1a0", "warning": "#e5c07b", "danger": "#e06c75", "info": "#61afef",
        "keyword": "#c678dd", "string": "#98c379", "number": "#d19a66",
        "function": "#61afef", "type": "#e5c07b", "comment": "#5c6370"
    }"##;

    #[test]
    fn a_bare_answer_gives_every_seed() {
        let draft = parse_theme_answer(SEEDS, "a forest", None).unwrap();
        assert_eq!(draft.name, "Bosque");
        assert_eq!(draft.base, ThemeBase::Dark);
        assert_eq!(draft.colors.len(), SEED_KEYS.len());
        assert_eq!(draft.colors["bg"], "#1B2B22");
        assert_eq!(draft.colors["comment"], "#5C6370");
    }

    #[test]
    fn a_fenced_answer_with_prose() {
        let answer = format!(
            "Here is your theme {{calm}}:\n\n```json\n{SEEDS}\n```\n\nEnjoy! {{\"bg\": \"#fff\"}}"
        );
        let draft = parse_theme_answer(&answer, "a forest", None).unwrap();
        assert_eq!(draft.name, "Bosque");
        assert_eq!(draft.colors["bg"], "#1B2B22");
    }

    #[test]
    fn colors_may_be_nested_and_unknown_keys_dropped() {
        let answer = r##"{"name": "Mar", "base": "light", "colors": {
            "bg": "#fafafa", "ink": "#123", "accent": "#0057b8", "sparkle": "#ffffff",
            "comment": "grey", "number": "#12345"
        }}"##;
        let draft = parse_theme_answer(answer, "sea", None).unwrap();
        assert_eq!(draft.base, ThemeBase::Light);
        assert_eq!(
            draft.colors,
            colors(&[("accent", "#0057B8"), ("bg", "#FAFAFA"), ("ink", "#112233")])
        );
    }

    #[test]
    fn required_seeds_are_needed() {
        let answer = r##"{"name": "X", "bg": "#000000", "ink": "#ffffff"}"##;
        assert_eq!(parse_theme_answer(answer, "x", None), None);
        let bad_accent = r##"{"bg": "#000000", "ink": "#ffffff", "accent": "#zzz"}"##;
        assert_eq!(parse_theme_answer(bad_accent, "x", None), None);
        assert_eq!(parse_theme_answer("no json here", "x", None), None);
        assert_eq!(parse_theme_answer("[1, 2]", "x", None), None);
        assert_eq!(parse_theme_answer("{\"bg\": ", "x", None), None);
    }

    #[test]
    fn the_base_is_inferred_from_the_background() {
        let dark = r##"{"bg": "#1e1e2e", "ink": "#cdd6f4", "accent": "#89b4fa"}"##;
        let light =
            r##"{"bg": "#eff1f5", "ink": "#4c4f69", "accent": "#1e66f5", "base": "pastel"}"##;
        assert_eq!(
            parse_theme_answer(dark, "x", None).unwrap().base,
            ThemeBase::Dark
        );
        assert_eq!(
            parse_theme_answer(light, "x", None).unwrap().base,
            ThemeBase::Light
        );
        // The base the user asked for wins.
        assert_eq!(
            parse_theme_answer(dark, "x", Some(ThemeBase::Light))
                .unwrap()
                .base,
            ThemeBase::Light
        );
    }

    #[test]
    fn the_name_falls_back_to_the_description() {
        let answer = r##"{"name": "  ", "bg": "#000", "ink": "#fff", "accent": "#f0f"}"##;
        let draft = parse_theme_answer(answer, "  Neon night\nwith pink ", None).unwrap();
        assert_eq!(draft.name, "Neon night");
        assert_eq!(draft.colors["bg"], "#000000");
        let long = format!(
            r##"{{"name": "{}", "bg": "#000", "ink": "#fff", "accent": "#f0f"}}"##,
            "n".repeat(80)
        );
        assert_eq!(
            parse_theme_answer(&long, "x", None).unwrap().name.len(),
            MAX_NAME
        );
    }

    #[test]
    fn braces_inside_strings_do_not_confuse_it() {
        let answer = r##"{"name": "Curly } {", "bg": "#000", "ink": "#fff", "accent": "#f0f"}"##;
        assert_eq!(
            parse_theme_answer(answer, "x", None).unwrap().name,
            "Curly } {"
        );
    }
}
