//! Interpreting each CLI's login check. Only non-identifying fields are kept:
//! no email, no organization, never tokens.

use super::Provider;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginState {
    /// `plan` is the subscription type when the CLI reports it (e.g. "max", "team").
    LoggedIn {
        plan: Option<String>,
    },
    LoggedOut,
    /// The CLI has no status command, or its answer could not be understood.
    Unknown,
}

impl LoginState {
    pub fn from_status_output(provider: Provider, exit_code: Option<i32>, stdout: &str) -> Self {
        match provider {
            Provider::Claude => from_claude(exit_code, stdout),
            // `codex login status` exits 0 when logged in.
            Provider::Codex => match exit_code {
                Some(0) => Self::LoggedIn { plan: None },
                Some(_) => Self::LoggedOut,
                None => Self::Unknown,
            },
            Provider::Gemini | Provider::Kimi => Self::Unknown,
        }
    }
}

/// `claude auth status --json`: exit 0 + `loggedIn: true` when logged in.
fn from_claude(exit_code: Option<i32>, stdout: &str) -> LoginState {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return LoginState::Unknown;
    };
    match (exit_code, json["loggedIn"].as_bool()) {
        (Some(0), Some(true)) => LoginState::LoggedIn {
            plan: json["subscriptionType"].as_str().map(str::to_owned),
        },
        (_, Some(false)) => LoginState::LoggedOut,
        _ => LoginState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_logged_in_keeps_only_the_plan() {
        let out = r#"{"loggedIn":true,"authMethod":"claude.ai","email":"x@y.z","subscriptionType":"team"}"#;
        assert_eq!(
            LoginState::from_status_output(Provider::Claude, Some(0), out),
            LoginState::LoggedIn {
                plan: Some("team".into())
            }
        );
    }

    #[test]
    fn claude_logged_out_and_garbage() {
        let out = r#"{"loggedIn":false,"authMethod":"none"}"#;
        assert_eq!(
            LoginState::from_status_output(Provider::Claude, Some(1), out),
            LoginState::LoggedOut
        );
        assert_eq!(
            LoginState::from_status_output(Provider::Claude, Some(0), "oops"),
            LoginState::Unknown
        );
    }

    #[test]
    fn codex_uses_exit_code() {
        let state = |code| LoginState::from_status_output(Provider::Codex, code, "");
        assert_eq!(state(Some(0)), LoginState::LoggedIn { plan: None });
        assert_eq!(state(Some(1)), LoginState::LoggedOut);
        assert_eq!(state(None), LoginState::Unknown);
    }
}
