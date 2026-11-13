use std::fmt;
use std::str::FromStr;

use super::DomainError;

/// An RGB color chosen by the user for an account or a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Account palette from the spec (Tokyo Night). English and Spanish names.
    pub const PALETTE: [(&'static str, &'static str, Color); 8] = [
        ("blue", "azul", Color::rgb(0x7A, 0xA2, 0xF7)),
        ("purple", "morado", Color::rgb(0xBB, 0x9A, 0xF7)),
        ("green", "verde", Color::rgb(0x9E, 0xCE, 0x6A)),
        ("orange", "naranja", Color::rgb(0xFF, 0x9E, 0x64)),
        ("cyan", "cian", Color::rgb(0x7D, 0xCF, 0xFF)),
        ("pink", "rosa", Color::rgb(0xFF, 0x7A, 0xB6)),
        ("yellow", "amarillo", Color::rgb(0xE0, 0xAF, 0x68)),
        ("teal", "turquesa", Color::rgb(0x73, 0xDA, 0xCA)),
    ];

    /// Palette color for the n-th account, so new accounts get different colors.
    pub fn nth_in_palette(n: usize) -> Self {
        Self::PALETTE[n % Self::PALETTE.len()].2
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl FromStr for Color {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let input = s.trim();
        if let Some((_, _, color)) = Self::PALETTE
            .iter()
            .find(|(en, es, _)| en.eq_ignore_ascii_case(input) || es.eq_ignore_ascii_case(input))
        {
            return Ok(*color);
        }
        let invalid = || DomainError::InvalidColor(s.to_owned());
        let hex = input.strip_prefix('#').ok_or_else(invalid)?;
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| invalid());
        Ok(Self::rgb(channel(0)?, channel(2)?, channel(4)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_palette_names_in_both_languages() {
        assert_eq!(
            "blue".parse::<Color>().unwrap(),
            Color::rgb(0x7A, 0xA2, 0xF7)
        );
        assert_eq!(
            "Morado".parse::<Color>().unwrap(),
            Color::rgb(0xBB, 0x9A, 0xF7)
        );
    }

    #[test]
    fn parses_hex_and_round_trips() {
        let c: Color = "#ff7ab6".parse().unwrap();
        assert_eq!(c.to_string(), "#FF7AB6");
        assert_eq!(c.to_string().parse::<Color>().unwrap(), c);
    }

    #[test]
    fn rejects_invalid_values() {
        for bad in ["", "#12345", "#GGGGGG", "7AA2F7", "#7AA2F7FF", "rojo"] {
            assert!(bad.parse::<Color>().is_err(), "{bad}");
        }
    }

    #[test]
    fn palette_cycles() {
        assert_eq!(Color::nth_in_palette(0), Color::nth_in_palette(8));
        assert_ne!(Color::nth_in_palette(0), Color::nth_in_palette(1));
    }
}
