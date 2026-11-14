//! Terminal colors from Hedy's palette (Tokyo Night). Disabled when stdout is
//! not a terminal or `NO_COLOR` is set.

use std::fmt::Display;
use std::io::IsTerminal;
use std::path::Path;

use hedy_core::domain::Color;

pub const BLUE: Color = Color::rgb(0x7A, 0xA2, 0xF7);
pub const CYAN: Color = Color::rgb(0x7D, 0xCF, 0xFF);
pub const GREEN: Color = Color::rgb(0x9E, 0xCE, 0x6A);
pub const YELLOW: Color = Color::rgb(0xE0, 0xAF, 0x68);
pub const RED: Color = Color::rgb(0xF7, 0x76, 0x8E);
pub const MUTED: Color = Color::rgb(0x9A, 0xA5, 0xCE);
pub const FAINT: Color = Color::rgb(0x41, 0x48, 0x68);

/// Width of section rules, so the screen reads as one block.
const RULE_WIDTH: usize = 66;

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

    #[cfg(test)]
    pub fn plain() -> Self {
        Self { enabled: false }
    }

    #[cfg(test)]
    pub fn colored() -> Self {
        Self { enabled: true }
    }

    /// Whether colors (and with them the blob, bars and spinner) are on.
    pub fn enabled(&self) -> bool {
        self.enabled
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
        self.paint(BLUE, text)
    }

    /// Something to type: a command or a key.
    pub fn command(&self, text: impl Display) -> String {
        self.paint(CYAN, text)
    }

    /// A section title followed by a faint rule: `Cuentas ───────`.
    pub fn section(&self, title: &str) -> String {
        let rule = "─".repeat(RULE_WIDTH.saturating_sub(title.chars().count() + 1));
        format!("  {} {}", self.bold(title), self.paint(FAINT, rule))
    }

    /// A horizontal bar `fraction` (0–1) of `width` cells, in eighths of a cell.
    pub fn bar(&self, fraction: f64, width: usize) -> String {
        const EIGHTHS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
        let eighths = (fraction.clamp(0.0, 1.0) * (width * 8) as f64).round() as usize;
        // Anything above zero gets at least a sliver.
        let eighths = if fraction > 0.0 { eighths.max(1) } else { 0 };
        let (full, rest) = (eighths / 8, eighths % 8);
        let filled = format!("{}{}", "█".repeat(full), EIGHTHS[rest]);
        let len = full + usize::from(rest > 0);
        format!(
            "{}{}",
            self.paint(BLUE, filled),
            " ".repeat(width.saturating_sub(len))
        )
    }

    pub fn muted(&self, text: impl Display) -> String {
        self.paint(MUTED, text)
    }
}

/// Colors for clap's help, from the same palette. clap turns them off by
/// itself outside a terminal or with `NO_COLOR`.
pub fn help_styles() -> clap::builder::Styles {
    use clap::builder::styling::{RgbColor, Style as Ansi};
    let rgb = |c: Color| Ansi::new().fg_color(Some(RgbColor(c.r, c.g, c.b).into()));
    clap::builder::Styles::styled()
        .header(rgb(BLUE).bold())
        .usage(rgb(BLUE).bold())
        .literal(rgb(CYAN))
        .placeholder(rgb(MUTED))
        .valid(rgb(GREEN))
        .invalid(rgb(YELLOW))
        .error(rgb(RED).bold())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_fill_in_eighths_and_keep_their_width() {
        let s = Style::plain();
        assert_eq!(s.bar(1.0, 4), "████");
        assert_eq!(s.bar(0.5, 4), "██  ");
        assert_eq!(s.bar(0.0, 4), "    ");
        // Tiny but not zero: still visible.
        assert_eq!(s.bar(0.001, 4), "▏   ");
        assert_eq!(s.bar(0.53, 4), "██▏ ");
    }
}
