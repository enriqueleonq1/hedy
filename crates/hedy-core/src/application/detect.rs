//! What the onboarding finds on this computer: the official CLIs installed and
//! whether they already have a login in their default folder.

use super::{AppResult, Hedy};
use crate::domain::{Account, AccountId, Color, ConfigFolder, Credentials, LoginState, Provider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedCli {
    pub provider: Provider,
    pub installed: bool,
    /// The default folder's login, when the CLI can tell (Claude, Codex).
    pub login: LoginState,
    /// A Hedy account already using that default folder.
    pub account: Option<AccountId>,
}

impl Hedy {
    /// Checks every provider's CLI. Login checks run in parallel, since each
    /// one starts a process.
    pub fn detect_clis(&self) -> AppResult<Vec<DetectedCli>> {
        let accounts = self.accounts()?;
        let checks: Vec<AppResult<DetectedCli>> = std::thread::scope(|scope| {
            let handles: Vec<_> = Provider::ALL
                .into_iter()
                .map(|provider| {
                    let accounts = &accounts;
                    scope.spawn(move || self.detect(provider, accounts))
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("detection panicked"))
                .collect()
        });
        checks.into_iter().collect()
    }

    fn detect(&self, provider: Provider, accounts: &[Account]) -> AppResult<DetectedCli> {
        let program = self
            .binary_for(provider)?
            .or_else(|| provider.program().map(str::to_owned))
            .unwrap_or_default();
        let installed = self.runner.is_installed(&program);
        let default = Credentials::Subscription(ConfigFolder::Default);
        let account = accounts
            .iter()
            .find(|a| a.provider == provider && a.credentials == default)
            .map(|a| a.id.clone());
        let login = if installed {
            // A throwaway account on the default folder, only to ask the CLI.
            let probe = Account::new(
                AccountId::new("probe")?,
                provider.cli_name(),
                provider,
                default,
                Color::nth_in_palette(0),
            )?;
            self.login_state(&probe)?
        } else {
            LoginState::Unknown
        };
        Ok(DetectedCli {
            provider,
            installed,
            login,
            account,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::application::testing::Fixture;
    use crate::domain::{LoginState, Provider};

    #[test]
    fn finds_installed_clis_and_their_default_login() {
        let f = Fixture::new();
        f.runner.install("claude");
        f.runner
            .reply(r#"{"loggedIn":true,"subscriptionType":"team"}"#, 0);
        let found = f.hedy.detect_clis().unwrap();
        let claude = found
            .iter()
            .find(|d| d.provider == Provider::Claude)
            .unwrap();
        assert!(claude.installed);
        assert_eq!(
            claude.login,
            LoginState::LoggedIn {
                plan: Some("team".into())
            }
        );
        let kimi = found.iter().find(|d| d.provider == Provider::Kimi).unwrap();
        assert!(!kimi.installed);
        assert_eq!(found.len(), Provider::ALL.len());
    }

    #[test]
    fn only_https_and_local_links_open() {
        let f = Fixture::new();
        assert!(f.hedy.open_link("file:///etc/passwd").is_err());
        // The preview's local dev server, and nothing that only looks like it.
        f.hedy.open_link("http://localhost:5173/citas").unwrap();
        f.hedy.open_link("http://127.0.0.1:3000").unwrap();
        for bad in [
            "http://localhost.evil.com",
            "http://localhost@evil.com",
            "http://localhost:80a",
            "http://example.com",
            "http://localhost:3000/ x",
        ] {
            assert!(f.hedy.open_link(bad).is_err(), "{bad}");
        }
        f.hedy.open_link("https://antigravity.google").unwrap();
    }
}
