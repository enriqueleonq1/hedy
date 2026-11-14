//! The language of the texts Hedy shows, and of what it asks the agents.
//!
//! One process-wide setting: driving adapters pick it at start (the app from
//! its setting or the system locale, the CLI from the same setting or `LANG`)
//! and the core writes every user-facing text through [`tr!`].

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Spanish,
    English,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

#[cfg(test)]
thread_local! {
    /// Lets a test use a language without touching other tests' threads.
    static OVERRIDE: std::cell::Cell<Option<Language>> = const { std::cell::Cell::new(None) };
}

/// Runs `f` with `language` on this thread only (tests).
#[cfg(test)]
pub fn with_language<T>(language: Language, f: impl FnOnce() -> T) -> T {
    OVERRIDE.with(|o| o.set(Some(language)));
    let result = f();
    OVERRIDE.with(|o| o.set(None));
    result
}

impl Language {
    /// `"es"` / `"en"`.
    pub fn code(self) -> &'static str {
        match self {
            Self::Spanish => "es",
            Self::English => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_lowercase().as_str() {
            "es" => Some(Self::Spanish),
            "en" => Some(Self::English),
            _ => None,
        }
    }

    /// From a POSIX locale such as `es_CL.UTF-8`: Spanish for `es*`, else English.
    pub fn from_locale(locale: &str) -> Self {
        if locale.trim().to_lowercase().starts_with("es") {
            Self::Spanish
        } else {
            Self::English
        }
    }

    /// The system's language, from the usual locale variables.
    pub fn from_environment() -> Self {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|var| {
                std::env::var(var)
                    .ok()
                    .filter(|v| !v.is_empty() && v != "C")
            })
            .map_or(Self::English, |locale| Self::from_locale(&locale))
    }
}

/// Passes the language to the processes Hedy starts (the hook, the memory
/// server), which cannot read the user's setting.
pub const ENV_VAR: &str = "HEDY_LANGUAGE";

/// Picks the language: the user's choice (`"es"`, `"en"`), else what the
/// parent Hedy passed down, else the system's.
pub fn init(preference: Option<&str>) {
    let language = preference
        .and_then(Language::from_code)
        .or_else(|| {
            std::env::var(ENV_VAR)
                .ok()
                .and_then(|v| Language::from_code(&v))
        })
        .unwrap_or_else(Language::from_environment);
    set_language(language);
}

pub fn set_language(language: Language) {
    CURRENT.store(language as u8, Ordering::Relaxed);
}

pub fn language() -> Language {
    #[cfg(test)]
    if let Some(language) = OVERRIDE.with(std::cell::Cell::get) {
        return language;
    }
    match CURRENT.load(Ordering::Relaxed) {
        1 => Language::English,
        _ => Language::Spanish,
    }
}

/// A text in the current language: `tr!("hola {name}", "hi {name}")`. Both
/// strings are format strings and capture variables from the scope.
#[macro_export]
macro_rules! tr {
    ($es:literal, $en:literal $(,)?) => {
        match $crate::i18n::language() {
            $crate::i18n::Language::English => format!($en),
            $crate::i18n::Language::Spanish => format!($es),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_and_codes() {
        assert_eq!(Language::from_locale("es_CL.UTF-8"), Language::Spanish);
        assert_eq!(Language::from_locale("en_US.UTF-8"), Language::English);
        assert_eq!(Language::from_locale("de_DE"), Language::English);
        assert_eq!(Language::from_code("EN"), Some(Language::English));
        assert_eq!(Language::from_code("fr"), None);
    }
}
