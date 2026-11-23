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
    /// Google's Antigravity CLI (`agy`), which replaced Gemini CLI for personal
    /// Google accounts.
    Antigravity,
}

impl Provider {
    pub const ALL: [Provider; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::Kimi,
        Self::Antigravity,
    ];

    /// Stable identifier used in storage and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Kimi => "kimi",
            Self::Antigravity => "antigravity",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::Kimi => "Kimi",
            Self::Antigravity => "Antigravity",
        }
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
            Self::Kimi => Some("kimi"),
            Self::Antigravity => Some("agy"),
        }
    }

    /// Environment variable that points the official CLI at an account folder.
    /// `None`: the CLI keeps its login in the OS keyring with no way to choose
    /// a folder, so only one account (its default folder) fits per machine.
    pub fn folder_env_var(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("CLAUDE_CONFIG_DIR"),
            Self::Codex => Some("CODEX_HOME"),
            Self::Gemini => Some("GEMINI_CLI_HOME"),
            Self::Kimi => Some("KIMI_CODE_HOME"),
            Self::Antigravity => None,
        }
    }

    /// Arguments that start the CLI as an Agent Client Protocol server, for
    /// providers driven through ACP (docs/decisions.md §14).
    pub fn acp_args(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Gemini => Some(&["--acp"]),
            Self::Kimi | Self::Antigravity => Some(&["acp"]),
            Self::Claude | Self::Codex => None,
        }
    }

    /// The CLI's own name, for messages: "Gemini CLI", "Kimi Code".
    pub fn cli_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini CLI",
            Self::Kimi => "Kimi Code",
            Self::Antigravity => "Antigravity",
        }
    }

    /// The CLI version Hedy was last verified with (docs/decisions.md, table at
    /// the top). `None`: not verified against a real binary yet.
    pub fn verified_version(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("2.1.306"),
            Self::Codex => Some("0.169.0"),
            Self::Gemini => Some("0.68.0"),
            Self::Kimi | Self::Antigravity => None,
        }
    }

    /// The CLI flag that picks a model, where verified (`gemini -m`).
    pub fn model_flag(self) -> Option<&'static str> {
        match self {
            Self::Gemini => Some("-m"),
            _ => None,
        }
    }

    /// Whether its ACP agent reads a PDF sent as an embedded `resource`
    /// (docs/decisions.md §54): Gemini CLI turns it into inline data; Kimi
    /// Code drops blobs and Antigravity does not say.
    pub fn acp_reads_pdf(self) -> bool {
        self == Self::Gemini
    }

    /// Whether its CLI reads files without asking Hedy first. Such agents always
    /// work in a worktree, where protected files like `.env` do not exist.
    pub fn always_in_worktree(self) -> bool {
        self == Self::Codex || self.acp_args().is_some()
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
    fn every_provider_has_an_official_cli() {
        for p in Provider::ALL {
            assert!(p.program().is_some(), "{p}");
            // Only Antigravity keeps one login per machine.
            assert_eq!(
                p.folder_env_var().is_none(),
                p == Provider::Antigravity,
                "{p}"
            );
        }
        assert_eq!(Provider::Gemini.acp_args(), Some(&["--acp"][..]));
        assert!(Provider::Kimi.always_in_worktree() && Provider::Codex.always_in_worktree());
        assert!(!Provider::Claude.always_in_worktree());
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
