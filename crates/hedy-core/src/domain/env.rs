//! Environment variables a launched CLI must never inherit.
//!
//! Any of these would make the CLI use a credential other than the login in the
//! account folder, bill through an API key, or point at another account's folder
//! (docs/decisions.md §2).

const BLOCKED: &[&str] = &[
    // Claude Code: credentials that outrank the folder login.
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    // Leftovers when Hedy itself runs inside a Claude Code session.
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "AI_AGENT",
    // Codex.
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "CODEX_ACCESS_TOKEN",
    // Gemini. The last one stores every folder's login under one keyring key.
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_GENAI_USE_VERTEXAI",
    "GEMINI_FORCE_ENCRYPTED_FILE_STORAGE",
    // Folder variables: Hedy sets the right one for the selected account.
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "GEMINI_CLI_HOME",
];

/// Includes `CLAUDE_CODE_OAUTH_TOKEN`, which outranks the folder login.
const BLOCKED_PREFIXES: &[&str] = &["CLAUDE_CODE_"];

pub fn is_blocked(name: &str) -> bool {
    BLOCKED.contains(&name) || BLOCKED_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_credentials_and_folder_vars() {
        for var in [
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_ENTRYPOINT",
            "OPENAI_API_KEY",
            "GEMINI_FORCE_ENCRYPTED_FILE_STORAGE",
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
        ] {
            assert!(is_blocked(var), "{var}");
        }
    }

    #[test]
    fn keeps_ordinary_vars() {
        for var in [
            "PATH",
            "HOME",
            "TERM",
            "LANG",
            "CLAUDE",
            "MY_ANTHROPIC_API_KEY_NOTE",
        ] {
            assert!(!is_blocked(var), "{var}");
        }
    }
}
