use std::fmt;
use std::path::PathBuf;

use super::{Color, DomainError, Provider};

/// Account slug, e.g. `claude-personal`. Also the keyring user for API keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(String);

impl AccountId {
    const MAX_LEN: usize = 48;

    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= Self::MAX_LEN
            && !value.starts_with('-')
            && !value.ends_with('-')
            && value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if valid {
            Ok(Self(value))
        } else {
            Err(DomainError::InvalidAccountId(value))
        }
    }

    /// Builds a slug from a display name: "Claude Trabajo" → `claude-trabajo`.
    pub fn from_name(name: &str) -> Result<Self, DomainError> {
        let mut slug = String::with_capacity(name.len());
        for c in name
            .trim()
            .chars()
            .flat_map(fold_accent)
            .map(|c| c.to_ascii_lowercase())
        {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
            } else if !slug.ends_with('-') && !slug.is_empty() {
                slug.push('-');
            }
        }
        let slug = slug.trim_end_matches('-');
        Self::new(
            slug.chars()
                .take(Self::MAX_LEN)
                .collect::<String>()
                .trim_end_matches('-'),
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Maps common Spanish accented letters to ASCII for slugs.
fn fold_accent(c: char) -> Option<char> {
    Some(match c {
        'á' | 'Á' => 'a',
        'é' | 'É' => 'e',
        'í' | 'Í' => 'i',
        'ó' | 'Ó' => 'o',
        'ú' | 'Ú' | 'ü' | 'Ü' => 'u',
        'ñ' | 'Ñ' => 'n',
        other => other,
    })
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a subscription account keeps its CLI state (and its login).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFolder {
    /// The CLI's own default location (e.g. `~/.claude` + `~/.claude.json`).
    /// Launched *without* the folder variable: setting `CLAUDE_CONFIG_DIR=~/.claude`
    /// would make Claude miss `~/.claude.json` (docs/decisions.md §2).
    Default,
    /// A folder managed by Hedy, passed through the provider's folder variable.
    Managed(PathBuf),
}

/// How an account authenticates. Hedy never holds subscription tokens: the
/// official CLI keeps them inside the account folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credentials {
    Subscription(ConfigFolder),
    /// The key lives in the OS keyring, keyed by the account id.
    ApiKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub provider: Provider,
    pub credentials: Credentials,
    pub color: Color,
}

impl Account {
    pub fn new(
        id: AccountId,
        name: impl Into<String>,
        provider: Provider,
        credentials: Credentials,
        color: Color,
    ) -> Result<Self, DomainError> {
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(DomainError::EmptyAccountName);
        }
        match credentials {
            Credentials::Subscription(_) if !provider.supports_subscription() => {
                return Err(DomainError::SubscriptionNotSupported { provider });
            }
            Credentials::ApiKey if !provider.supports_api_key() => {
                return Err(DomainError::ApiKeyNotSupported { provider });
            }
            _ => {}
        }
        Ok(Self {
            id,
            name,
            provider,
            credentials,
            color,
        })
    }

    pub fn is_subscription(&self) -> bool {
        matches!(self.credentials, Credentials::Subscription(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color() -> Color {
        Color::nth_in_palette(0)
    }

    #[test]
    fn slug_from_name() {
        let cases = [
            ("Claude personal", "claude-personal"),
            ("  Claude   Trabajo ", "claude-trabajo"),
            ("Diseño & Señales", "diseno-senales"),
            ("Kimi (API)", "kimi-api"),
        ];
        for (name, slug) in cases {
            assert_eq!(AccountId::from_name(name).unwrap().as_str(), slug, "{name}");
        }
        assert!(AccountId::from_name("***").is_err());
    }

    #[test]
    fn id_validation() {
        assert!(AccountId::new("work-2").is_ok());
        for bad in ["", "Work", "a b", "-a", "a-", "a_b", &"a".repeat(49)] {
            assert!(AccountId::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_unsupported_credential_kinds() {
        let id = AccountId::new("x").unwrap();
        let sub = Credentials::Subscription(ConfigFolder::Default);
        assert_eq!(
            Account::new(id.clone(), "Kimi", Provider::Kimi, sub, color()),
            Err(DomainError::SubscriptionNotSupported {
                provider: Provider::Kimi
            })
        );
        assert_eq!(
            Account::new(
                id.clone(),
                "Claude",
                Provider::Claude,
                Credentials::ApiKey,
                color()
            ),
            Err(DomainError::ApiKeyNotSupported {
                provider: Provider::Claude
            })
        );
        assert!(Account::new(id, "Gemini", Provider::Gemini, Credentials::ApiKey, color()).is_ok());
    }

    #[test]
    fn rejects_blank_name() {
        let id = AccountId::new("x").unwrap();
        let err = Account::new(id, "  ", Provider::Kimi, Credentials::ApiKey, color());
        assert_eq!(err, Err(DomainError::EmptyAccountName));
    }
}
