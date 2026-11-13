use std::fmt;
use std::str::FromStr;

use super::DomainError;

/// An AI provider Hedy can work with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    Claude,
    Codex,
    Gemini,
    Kimi,
}

impl Provider {
    pub const ALL: [Provider; 4] = [Self::Claude, Self::Codex, Self::Gemini, Self::Kimi];

    /// Stable identifier used in storage and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Kimi => "kimi",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::Kimi => "Kimi",
        }
    }

    /// Whether the provider has an official CLI that logs in with a subscription.
    pub fn supports_subscription(self) -> bool {
        matches!(self, Self::Claude | Self::Codex | Self::Gemini)
    }

    /// Whether the provider can be used over HTTP with an API key.
    pub fn supports_api_key(self) -> bool {
        matches!(self, Self::Gemini | Self::Kimi)
    }

    /// Official CLI binary name.
    pub fn program(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("claude"),
            Self::Codex => Some("codex"),
            Self::Gemini => Some("gemini"),
            Self::Kimi => None,
        }
    }

    /// Environment variable that points the official CLI at an account folder.
    pub fn folder_env_var(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("CLAUDE_CONFIG_DIR"),
            Self::Codex => Some("CODEX_HOME"),
            Self::Gemini => Some("GEMINI_CLI_HOME"),
            Self::Kimi => None,
        }
    }

    /// Files Hedy writes once into a fresh account folder so the CLI keeps its
    /// login inside that folder. Contains configuration only, never credentials.
    pub fn folder_bootstrap(self) -> &'static [(&'static str, &'static str)] {
        match self {
            // Without this, Codex may store the login in the OS keyring, and how that
            // entry is keyed per CODEX_HOME is undocumented (docs/decisions.md §2).
            Self::Codex => &[("config.toml", "cli_auth_credentials_store = \"file\"\n")],
            _ => &[],
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

impl FromStr for Provider {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_str().eq_ignore_ascii_case(s.trim()))
            .ok_or_else(|| DomainError::UnknownProvider(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_round_trip() {
        for p in Provider::ALL {
            assert_eq!(p.as_str().parse::<Provider>().unwrap(), p);
        }
        assert_eq!(" Claude ".parse::<Provider>().unwrap(), Provider::Claude);
        assert!("openai".parse::<Provider>().is_err());
    }

    #[test]
    fn every_subscription_provider_has_cli_and_folder_var() {
        for p in Provider::ALL
            .into_iter()
            .filter(|p| p.supports_subscription())
        {
            assert!(p.program().is_some(), "{p}");
            assert!(p.folder_env_var().is_some(), "{p}");
        }
    }

    #[test]
    fn codex_bootstrap_forces_file_credentials() {
        let files = Provider::Codex.folder_bootstrap();
        assert_eq!(files[0].0, "config.toml");
        assert!(files[0].1.contains("cli_auth_credentials_store = \"file\""));
    }
}
