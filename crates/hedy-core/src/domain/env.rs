//! Environment variables a launched CLI must never inherit.
//!
//! Any of these would make the CLI use a credential other than the login in the
//! account folder, bill through an API key, or point at another account's folder
//! (docs/decisions.md §2).

const BLOCKED: &[&str] = &[
    // Claude Code: credentials that outrank the folder login.
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    // Codex.
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "CODEX_ACCESS_TOKEN",
    // Gemini. The last one stores every folder's login under one keyring key.
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_GENAI_USE_VERTEXAI",
    "GEMINI_FORCE_ENCRYPTED_FILE_STORAGE",
    "GOOGLE_APPLICATION_CREDENTIALS",
    // Antigravity: Application Default Credentials instead of the Google login.
    "AGY_ADC_AUTH",
    // Kimi Code: API keys, endpoints and OAuth hosts that replace the login.
    "KIMI_API_KEY",
    "KIMI_BASE_URL",
    "KIMI_CODE_BASE_URL",
    "KIMI_CODE_OAUTH_HOST",
    "KIMI_OAUTH_HOST",
    // Folder variables: Hedy sets the right one for the selected account.
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "GEMINI_CLI_HOME",
    "KIMI_CODE_HOME",
];

/// `CLAUDE_CODE_OAUTH_TOKEN` outranks the folder login; `KIMI_MODEL_*` sets a
/// temporary model with its own API key and endpoint.
const BLOCKED_PREFIXES: &[&str] = &["CLAUDE_CODE_", "KIMI_MODEL_"];

/// Left behind when Hedy itself runs inside a Claude Code session. Blocked too,
/// but they change neither the account nor the billing.
const SESSION_LEFTOVERS: &[&str] = &["CLAUDECODE", "CLAUDE_PID", "CLAUDE_EFFORT", "AI_AGENT"];

/// `CLAUDE_CODE_*` variables that do change the account or the billing.
const CLAUDE_CODE_CREDENTIALS: &[&str] = &[
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

/// Set in every process Hedy starts (agents, logins, `hedy run`): what
/// only the user may do (approving memory, importing skills) is refused
/// there, so an agent cannot do it through a shell command. A second guard:
/// the permission card for that command is the first.
pub const AGENT_SESSION: &str = "HEDY_AGENT_SESSION";

pub fn is_blocked(name: &str) -> bool {
    BLOCKED.contains(&name)
        || SESSION_LEFTOVERS.contains(&name)
        || BLOCKED_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// A blocked variable that a Claude Code session sets for its own children
/// (`CLAUDECODE`, `CLAUDE_CODE_ENTRYPOINT`…), as opposed to one that would
/// change the account or the billing.
pub fn is_session_leftover(name: &str) -> bool {
    SESSION_LEFTOVERS.contains(&name)
        || (name.starts_with("CLAUDE_CODE_") && !CLAUDE_CODE_CREDENTIALS.contains(&name))
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
            "KIMI_CODE_HOME",
            "KIMI_MODEL_API_KEY",
            "AGY_ADC_AUTH",
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

    #[test]
    fn tells_session_leftovers_from_credentials() {
        for var in [
            "CLAUDECODE",
            "AI_AGENT",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_SESSION_ID",
        ] {
            assert!(is_blocked(var) && is_session_leftover(var), "{var}");
        }
        for var in [
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CONFIG_DIR",
            "KIMI_MODEL_API_KEY",
        ] {
            assert!(!is_session_leftover(var), "{var}");
        }
    }
}
