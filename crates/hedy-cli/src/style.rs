//! Terminal colors from Hedy's palette (Tokyo Night). Disabled when stdout is
//! not a terminal or `NO_COLOR` is set.

use std::fmt::Display;
use std::io::IsTerminal;
use std::path::Path;

use hedy_core::domain::Color;

const GREEN: Color = Color::rgb(0x9E, 0xCE, 0x6A);
const YELLOW: Color = Color::rgb(0xE0, 0xAF, 0x68);
const RED: Color = Color::rgb(0xF7, 0x76, 0x8E);
const MAGENTA: Color = Color::rgb(0xBB, 0x9A, 0xF7);
const MUTED: Color = Color::rgb(0x9A, 0xA5, 0xCE);

#[derive(Debug, Clone, Copy)]
pub struct Style {
    enabled: bool,
}

impl Style {
    pub fn for_stdout() -> Self {
        Self::detect(std::io::stdout().is_terminal())
    }

    pub fn for_stderr() -> Self {
        Self::detect(std::io::stderr().is_terminal())
    }

    fn detect(is_terminal: bool) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Self {
            enabled: is_terminal && !no_color,
        }
    }

    pub fn paint(&self, color: Color, text: impl Display) -> String {
        if self.enabled {
            format!(
                "\x1b[38;2;{};{};{}m{text}\x1b[0m",
                color.r, color.g, color.b
            )
        } else {
            text.to_string()
        }
    }

    pub fn bold(&self, text: impl Display) -> String {
        if self.enabled {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    /// Account color dot.
    pub fn chip(&self, color: Color) -> String {
        self.paint(color, "●")
    }

    pub fn ok(&self, text: impl Display) -> String {
        self.paint(GREEN, text)
    }

    pub fn warn(&self, text: impl Display) -> String {
        self.paint(YELLOW, text)
    }

    pub fn error(&self, text: impl Display) -> String {
        self.paint(RED, text)
    }

    pub fn accent(&self, text: impl Display) -> String {
        self.paint(MAGENTA, text)
    }

    pub fn muted(&self, text: impl Display) -> String {
        self.paint(MUTED, text)
    }
}

/// Shows paths under the home folder as `~/…`.
pub fn tilde(path: &Path) -> String {
    match std::env::var_os("HOME").map(std::path::PathBuf::from) {
        Some(home) if path.starts_with(&home) => {
            format!("~/{}", path.strip_prefix(&home).unwrap().display())
                .trim_end_matches('/')
                .to_owned()
        }
        _ => path.display().to_string(),
    }
}

/// Pads `text` to `width` visible characters (ANSI codes are not counted).
pub fn pad(text: &str, visible_len: usize, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(visible_len)))
}
