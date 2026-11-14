//! Hedy's blob drawn in the terminal with half blocks: each character cell holds
//! two pixels (`▀`: the upper one is the text color, the lower one the background).
//! Same shape and colors as the logo (`app/src-tauri/icons/logo.svg`).

use hedy_core::domain::Color;

use crate::style::Style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    /// Eyes open, small smile: greetings and status.
    Hi,
    /// Eyes closed in a smile, mouth open: something went well.
    Happy,
    /// Sleepy: nothing to show yet.
    Sleepy,
}

/// Width in character cells.
pub const WIDTH: usize = 16;

// Pixels: `.` empty, `B` body, `L` highlight, `D` eyes and mouth,
// `P` cheeks, `T` tongue. 16 × 14 pixels, 7 rows of text.
const HI: [&str; 14] = [
    ".......BB.......",
    ".......BB.......",
    "................",
    ".....BBBBBB.....",
    "...BBLLBBBBBB...",
    "..BLLLBBBBBBBB..",
    ".BBLBBBBBBBBBBB.",
    ".BBBDDBBBBDDBBB.",
    ".BBBDDBBBBDDBBB.",
    ".BBPBBDBBDBBPBB.",
    "BBBBBBBDDBBBBBBB",
    "BBBBBBBBBBBBBBBB",
    ".BBBBBBBBBBBBBB.",
    "...BBBBBBBBBB...",
];

const HAPPY: [&str; 14] = [
    ".......BB.......",
    ".......BB.......",
    "................",
    ".....BBBBBB.....",
    "...BBLLBBBBBB...",
    "..BLLLBBBBBBBB..",
    ".BBLBBBBBBBBBBB.",
    ".BBBDDBBBBDDBBB.",
    ".BBDBBDBBDBBDBB.",
    ".BBPBBBBBBBBPBB.",
    "BBBBBBDDDDBBBBBB",
    "BBBBBBDTTDBBBBBB",
    ".BBBBBBDDBBBBBB.",
    "...BBBBBBBBBB...",
];

const SLEEPY: [&str; 14] = [
    "................",
    "................",
    "................",
    ".....BBBBBB.....",
    "...BBLLBBBBBB...",
    "..BLLLBBBBBBBB..",
    ".BBLBBBBBBBBBBB.",
    ".BBBBBBBBBBBBBB.",
    ".BBBDDBBBBDDBBB.",
    ".BBPBBBBBBBBPBB.",
    "BBBBBBBDDBBBBBBB",
    "BBBBBBBBBBBBBBBB",
    ".BBBBBBBBBBBBBB.",
    "...BBBBBBBBBB...",
];

fn color(pixel: u8) -> Option<Color> {
    match pixel {
        b'B' => Some(Color::rgb(0x7A, 0xA2, 0xF7)),
        b'L' => Some(Color::rgb(0xA9, 0xC2, 0xFA)),
        b'D' => Some(Color::rgb(0x1A, 0x1B, 0x26)),
        // The logo's pink cheeks (#F7768E at 35 %) over the body.
        b'P' => Some(Color::rgb(0xA6, 0x93, 0xD2)),
        b'T' => Some(Color::rgb(0xF7, 0x76, 0x8E)),
        _ => None,
    }
}

/// The blob as lines of text, `WIDTH` cells wide. Empty when colors are off:
/// without colors it would be a block of noise.
pub fn render(style: &Style, mood: Mood) -> Vec<String> {
    if !style.enabled() {
        return Vec::new();
    }
    let pixels = match mood {
        Mood::Hi => &HI,
        Mood::Happy => &HAPPY,
        Mood::Sleepy => &SLEEPY,
    };
    pixels
        .chunks(2)
        .map(|pair| {
            let (top, bottom) = (pair[0].as_bytes(), pair[1].as_bytes());
            top.iter()
                .zip(bottom)
                .map(|(&t, &b)| cell(color(t), color(b)))
                .collect()
        })
        .collect()
}

fn cell(top: Option<Color>, bottom: Option<Color>) -> String {
    let fg = |c: Color| format!("\x1b[38;2;{};{};{}m", c.r, c.g, c.b);
    let bg = |c: Color| format!("\x1b[48;2;{};{};{}m", c.r, c.g, c.b);
    match (top, bottom) {
        (None, None) => " ".into(),
        (Some(t), None) => format!("{}▀\x1b[0m", fg(t)),
        (None, Some(b)) => format!("{}▄\x1b[0m", fg(b)),
        (Some(t), Some(b)) if t == b => format!("{}█\x1b[0m", fg(t)),
        (Some(t), Some(b)) => format!("{}{}▀\x1b[0m", fg(t), bg(b)),
    }
}

/// The blob with `text` beside it, vertically centered. Without colors, only
/// the text.
pub fn beside(style: &Style, mood: Mood, text: &[String]) -> Vec<String> {
    let art = render(style, mood);
    if art.is_empty() {
        return text.to_vec();
    }
    let top = art.len().saturating_sub(text.len()) / 2;
    let rows = art.len().max(top + text.len());
    (0..rows)
        .map(|row| {
            let left = art.get(row).cloned().unwrap_or_else(|| " ".repeat(WIDTH));
            let right = row
                .checked_sub(top)
                .and_then(|i| text.get(i))
                .map_or("", String::as_str);
            format!("  {left}   {right}").trim_end().to_owned()
        })
        .collect()
}

/// Prints [`beside`], with some air around the blob.
pub fn print_beside(style: &Style, mood: Mood, text: &[String]) {
    let framed = style.enabled();
    if framed {
        println!();
    }
    for line in beside(style, mood, text) {
        println!("{line}");
    }
    if framed {
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mood_is_a_full_rectangle() {
        for pixels in [&HI, &HAPPY, &SLEEPY] {
            assert_eq!(pixels.len() % 2, 0);
            assert!(pixels.iter().all(|row| row.len() == WIDTH));
        }
    }

    #[test]
    fn the_blob_is_hidden_without_colors() {
        assert!(render(&Style::plain(), Mood::Hi).is_empty());
        assert_eq!(render(&Style::colored(), Mood::Happy).len(), 7);
    }
}
