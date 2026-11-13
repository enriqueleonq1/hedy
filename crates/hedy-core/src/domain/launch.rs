//! How each official CLI is launched for an account. Pure: builds a description
//! of the process; running it is an adapter's job.

use std::path::PathBuf;

use super::{Account, ConfigFolder, Credentials, DomainError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPurpose {
    /// Interactive session, with extra arguments passed through.
    Run(Vec<String>),
    /// The CLI's official login flow.
    Login,
    /// Non-interactive login check. `None` from [`LaunchSpec::for_account`] when
    /// the CLI has no such command.
    LoginStatus,
}

/// A process to start: program, arguments and the account folder variable.
/// Blocked variables (see [`super::env`]) are removed by the process adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, PathBuf)>,
    pub cwd: Option<PathBuf>,
}

impl LaunchSpec {
    /// `program` overrides the binary path (user setting); defaults to the CLI name.
    pub fn for_account(
        account: &Account,
        purpose: LaunchPurpose,
        program: Option<&str>,
    ) -> Result<Option<Self>, DomainError> {
        let Credentials::Subscription(folder) = &account.credentials else {
            return Err(DomainError::NothingToLaunch);
        };
        let provider = account.provider;
        let (Some(default_program), Some(folder_var)) =
            (provider.program(), provider.folder_env_var())
        else {
            return Err(DomainError::NothingToLaunch);
        };

        let args: Vec<String> = match (provider, purpose) {
            (_, LaunchPurpose::Run(extra)) => extra,
            (super::Provider::Claude, LaunchPurpose::Login) => vec!["auth".into(), "login".into()],
            (super::Provider::Claude, LaunchPurpose::LoginStatus) => {
                vec!["auth".into(), "status".into(), "--json".into()]
            }
            (super::Provider::Codex, LaunchPurpose::Login) => vec!["login".into()],
            (super::Provider::Codex, LaunchPurpose::LoginStatus) => {
                vec!["login".into(), "status".into()]
            }
            // Gemini logs in from its first interactive run and has no status command.
            (super::Provider::Gemini, LaunchPurpose::Login) => vec![],
            (_, LaunchPurpose::LoginStatus) => return Ok(None),
            (_, LaunchPurpose::Login) => return Err(DomainError::NothingToLaunch),
        };

        let env = match folder {
            ConfigFolder::Default => vec![],
            ConfigFolder::Managed(path) => vec![(folder_var.to_owned(), path.clone())],
        };
        Ok(Some(Self {
            program: program.unwrap_or(default_program).to_owned(),
            args,
            env,
            cwd: None,
        }))
    }

    pub fn in_dir(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccountId, Color, Provider};

    fn account(provider: Provider, credentials: Credentials) -> Account {
        Account::new(
            AccountId::new("a").unwrap(),
            "A",
            provider,
            credentials,
            Color::nth_in_palette(0),
        )
        .unwrap()
    }

    fn managed(path: &str) -> Credentials {
        Credentials::Subscription(ConfigFolder::Managed(path.into()))
    }

    #[test]
    fn managed_folder_sets_the_provider_variable() {
        let spec = LaunchSpec::for_account(
            &account(Provider::Claude, managed("/acc/claude")),
            LaunchPurpose::Run(vec!["--resume".into()]),
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(spec.program, "claude");
        assert_eq!(spec.args, vec!["--resume"]);
        assert_eq!(
            spec.env,
            vec![("CLAUDE_CONFIG_DIR".into(), "/acc/claude".into())]
        );
    }

    #[test]
    fn default_folder_sets_no_variable() {
        let acc = account(
            Provider::Claude,
            Credentials::Subscription(ConfigFolder::Default),
        );
        let spec = LaunchSpec::for_account(&acc, LaunchPurpose::Login, None)
            .unwrap()
            .unwrap();
        assert!(spec.env.is_empty());
        assert_eq!(spec.args, vec!["auth", "login"]);
    }

    #[test]
    fn login_commands_per_provider() {
        let cases = [
            (Provider::Codex, LaunchPurpose::Login, Some(vec!["login"])),
            (
                Provider::Codex,
                LaunchPurpose::LoginStatus,
                Some(vec!["login", "status"]),
            ),
            (
                Provider::Claude,
                LaunchPurpose::LoginStatus,
                Some(vec!["auth", "status", "--json"]),
            ),
            (Provider::Gemini, LaunchPurpose::Login, Some(vec![])),
            (Provider::Gemini, LaunchPurpose::LoginStatus, None),
        ];
        for (provider, purpose, args) in cases {
            let spec =
                LaunchSpec::for_account(&account(provider, managed("/f")), purpose, None).unwrap();
            assert_eq!(
                spec.map(|s| s.args),
                args.map(|a| a.iter().map(|s| s.to_string()).collect())
            );
        }
    }

    #[test]
    fn program_override_and_api_accounts() {
        let spec = LaunchSpec::for_account(
            &account(Provider::Codex, managed("/f")),
            LaunchPurpose::Run(vec![]),
            Some("/opt/codex/bin/codex"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(spec.program, "/opt/codex/bin/codex");
        assert_eq!(spec.env[0].0, "CODEX_HOME");

        let api = account(Provider::Kimi, Credentials::ApiKey);
        assert_eq!(
            LaunchSpec::for_account(&api, LaunchPurpose::Run(vec![]), None),
            Err(DomainError::NothingToLaunch)
        );
    }
}
