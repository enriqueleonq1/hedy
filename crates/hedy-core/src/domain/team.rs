//! Team memory (docs/decisions.md §35): people, their roles and the rules
//! for what a team member writes. Pure: no I/O, no crypto (the `vault` and
//! `invite` adapters seal and check what this module describes).
//!
//! A team is a key vault like personal sync's, where each PC also belongs
//! to a person with a role:
//! - an **admin** adds and removes anyone and changes roles;
//! - a **member** only adds or removes their own PCs;
//! - the **recovery key** (the team's 12 words) can do everything.

use serde::{Deserialize, Serialize};

/// How long an invite works, in seconds: 48 hours.
pub const INVITE_TTL_SECS: i64 = 48 * 60 * 60;

/// A person in a team: random, like a [`DeviceId`](super::sync::DeviceId),
/// never derived from their name or account.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PersonId(pub String);

impl PersonId {
    /// 16 lowercase hex characters, from 8 random bytes.
    pub fn is_valid(&self) -> bool {
        super::sync::DeviceId(self.0.clone()).is_valid()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
}

/// Whose PC a team member is, and what they may do.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seat {
    pub person: PersonId,
    pub role: Role,
}

/// A character a reader does not see but an agent reads: text direction
/// overrides (the "Trojan Source" trick), zero-width and other invisible
/// format characters, Unicode tags and variation selectors that can spell
/// out hidden instructions, and control characters other than tab and new
/// line. A team note holding one is never accepted.
pub fn is_hidden(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{8}' | '\u{b}'..='\u{1f}' | '\u{7f}'..='\u{9f}'
        | '\u{ad}' | '\u{34f}' | '\u{61c}' | '\u{115f}' | '\u{1160}'
        | '\u{17b4}' | '\u{17b5}' | '\u{180b}'..='\u{180f}'
        | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{206f}' | '\u{3164}' | '\u{fe00}'..='\u{fe0e}'
        | '\u{feff}' | '\u{ffa0}' | '\u{fff0}'..='\u{fffb}'
        | '\u{1bca0}'..='\u{1bca3}' | '\u{1d173}'..='\u{1d17a}'
        | '\u{e0000}'..='\u{e0fff}')
}

/// The hidden characters in `text` (see [`is_hidden`]), in order. A zero
/// width joiner between two emoji (👩‍💻) is how emoji are written, not
/// hidden text, and `\r` before a new line is a Windows line end.
pub fn hidden_chars(text: &str) -> Vec<char> {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len())
        .filter(|&i| hidden_at(&chars, i))
        .map(|i| chars[i])
        .collect()
}

/// `text` with each hidden character shown as `⟦U+202E⟧`, for a reviewer.
pub fn reveal_hidden(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        if hidden_at(&chars, i) {
            out.push_str(&format!("⟦U+{:04X}⟧", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn hidden_at(chars: &[char], i: usize) -> bool {
    let after_emoji = i > 0 && is_emoji(chars[i - 1]);
    match chars[i] {
        '\u{200d}' => !(after_emoji && chars.get(i + 1).is_some_and(|&n| is_emoji(n))),
        '\r' => chars.get(i + 1) != Some(&'\n'),
        // The emoji presentation selector, right after an emoji.
        '\u{fe0f}' => !after_emoji,
        c => is_hidden(c),
    }
}

/// Roughly an emoji: pictographs and the symbol blocks emoji come from.
fn is_emoji(c: char) -> bool {
    matches!(c, '\u{2600}'..='\u{27bf}' | '\u{2b00}'..='\u{2bff}' | '\u{1f000}'..='\u{1faff}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn person_ids_are_like_device_ids() {
        assert!(PersonId("0123456789abcdef".into()).is_valid());
        assert!(!PersonId("".into()).is_valid());
        assert!(!PersonId("../../evil/00000".into()).is_valid());
        assert!(!PersonId("0123456789ABCDEF".into()).is_valid());
    }

    #[test]
    fn roles_and_seats_are_plain_json() {
        let seat = Seat {
            person: PersonId("0123456789abcdef".into()),
            role: Role::Admin,
        };
        assert_eq!(
            serde_json::to_string(&seat).unwrap(),
            r#"{"person":"0123456789abcdef","role":"admin"}"#
        );
    }

    #[test]
    fn hidden_characters_are_found_and_shown() {
        let plain = "Los importes van en céntimos.\n\tSiempre.\r\nNunca ñ ü 中文 🙂";
        assert!(hidden_chars(plain).is_empty());

        // A right-to-left override, a zero-width space and a Unicode tag.
        let sneaky = "Usa el script\u{202e}tpircs\u{202c} de\u{200b} release\u{e0041}";
        assert_eq!(
            hidden_chars(sneaky),
            vec!['\u{202e}', '\u{202c}', '\u{200b}', '\u{e0041}']
        );
        assert_eq!(
            reveal_hidden(sneaky),
            "Usa el script⟦U+202E⟧tpircs⟦U+202C⟧ de⟦U+200B⟧ release⟦U+E0041⟧"
        );

        // Emoji written with joiners and selectors are not hidden text…
        assert!(hidden_chars("👩\u{200d}💻 ❤\u{fe0f}").is_empty());
        // …but the same characters elsewhere are.
        assert_eq!(
            hidden_chars("a\u{200d}b c\u{fe0f}"),
            vec!['\u{200d}', '\u{fe0f}']
        );
        // A lone carriage return, an escape and a variation selector.
        assert_eq!(
            hidden_chars("a\rb\u{1b}[31m x\u{fe01}"),
            vec!['\r', '\u{1b}', '\u{fe01}']
        );
        assert_eq!(reveal_hidden("a\rb"), "a⟦U+000D⟧b");
    }
}
