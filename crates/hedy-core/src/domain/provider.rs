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

    /// OpenAI-compatible endpoint for API-key accounts (overridable in settings).
    pub fn api_base_url(self) -> Option<&'static str> {
        match self {
            Self::Kimi => Some("https://api.moonshot.ai/v1"),
            Self::Gemini => Some("https://generativelanguage.googleapis.com/v1beta/openai"),
            _ => None,
        }
    }

    /// Model used through the API when none is chosen.
    pub fn default_api_model(self) -> Option<&'static str> {
        match self {
            Self::Kimi => Some("kimi-k3.1-pro"),
            Self::Gemini => Some("gemini-4-hydrogen"),
            _ => None,
        }
    }

    /// How the chat names who did a step: "Claude Opus", "Codex", "Kimi".
    pub fn agent_label(self, model: Option<&str>) -> String {
        if self == Self::Claude
            && let Some(family) = model.and_then(|m| {
                ["fable", "opus", "sonnet", "haiku"]
                    .into_iter()
                    .find(|f| m.to_lowercase().contains(f))
            })
        {
            let mut chars = family.chars();
            let first = chars
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            return format!("Claude {first}{}", chars.as_str());
        }
        self.display_name()
            .split_whitespace()
            .next()
            .unwrap_or("IA")
            .to_owned()
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
    fn agent_labels() {
        assert_eq!(Provider::Claude.agent_label(Some("opus")), "Claude Opus");
        assert_eq!(Provider::Claude.agent_label(None), "Claude");
        assert_eq!(Provider::Codex.agent_label(Some("gpt-x")), "Codex");
        assert_eq!(Provider::Kimi.agent_label(None), "Kimi");
    }

    #[test]
    fn codex_bootstrap_forces_file_credentials() {
        let files = Provider::Codex.folder_bootstrap();
        assert_eq!(files[0].0, "config.toml");
        assert!(files[0].1.contains("cli_auth_credentials_store = \"file\""));
    }
}
