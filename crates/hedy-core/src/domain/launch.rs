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
    /// Non-interactive read of the plan's usage limits that never reaches the
    /// model. `None` from [`LaunchSpec::for_account`] when the CLI has none.
    PlanUsage,
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
        let Some(default_program) = provider.program() else {
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
            // Gemini logs in from its first interactive run, Kimi Code with `/login`
            // inside it; neither has a status command.
            (super::Provider::Gemini | super::Provider::Kimi, LaunchPurpose::Login) => vec![],
            // Signs in with Google by pasting a code, then exits.
            (super::Provider::Antigravity, LaunchPurpose::Login) => {
                vec!["acp".into(), "--login".into()]
            }
            // The built-in `/usage` runs locally in print mode (turns and cost
            // 0, docs/decisions.md §3). Safe mode skips hooks, MCP servers and
            // custom commands; nothing is saved as a session.
            (super::Provider::Claude, LaunchPurpose::PlanUsage) => [
                "-p",
                "/usage",
                "--output-format",
                "json",
                "--no-session-persistence",
                "--safe-mode",
            ]
            .map(String::from)
            .to_vec(),
            // JSON-RPC over stdio; the adapter asks `account/rateLimits/read`.
            (super::Provider::Codex, LaunchPurpose::PlanUsage) => {
                ["app-server", "--listen", "stdio://"]
                    .map(String::from)
                    .to_vec()
            }
            (_, LaunchPurpose::LoginStatus | LaunchPurpose::PlanUsage) => return Ok(None),
        };

        let env = match folder {
            ConfigFolder::Default => vec![],
            ConfigFolder::Managed(path) => {
                let var = provider
                    .folder_env_var()
                    .ok_or(DomainError::SingleAccountProvider { provider })?;
                vec![(var.to_owned(), path.clone())]
            }
        };
        Ok(Some(Self {
            program: program.unwrap_or(default_program).to_owned(),
            args,
            env,
            cwd: None,
        }))
    }

    /// Any program, without account variables (e.g. the code editor).
    pub fn program(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            env: vec![],
            cwd: None,
        }
    }

    /// The user's shell. With a subscription account in a Hedy-managed folder it
    /// exports the folder variable, so `claude`/`codex`/`gemini` typed in that
    /// shell use the account.
    pub fn shell(program: impl Into<String>, account: Option<&Account>) -> Self {
        let mut spec = Self::program(program, vec![]);
        if let Some(account) = account
            && let Credentials::Subscription(ConfigFolder::Managed(path)) = &account.credentials
            && let Some(var) = account.provider.folder_env_var()
        {
            spec.env.push((var.to_owned(), path.clone()));
        }
        spec
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
            (
                Provider::Claude,
                LaunchPurpose::PlanUsage,
                Some(vec![
                    "-p",
                    "/usage",
                    "--output-format",
                    "json",
                    "--no-session-persistence",
                    "--safe-mode",
                ]),
            ),
            (
                Provider::Codex,
                LaunchPurpose::PlanUsage,
                Some(vec!["app-server", "--listen", "stdio://"]),
            ),
            (Provider::Kimi, LaunchPurpose::PlanUsage, None),
            (Provider::Gemini, LaunchPurpose::PlanUsage, None),
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
    fn shell_exports_the_managed_folder_only() {
        let managed_acc = account(Provider::Codex, managed("/acc/codex"));
        let spec = LaunchSpec::shell("fish", Some(&managed_acc));
        assert_eq!(spec.program, "fish");
        assert_eq!(spec.env, vec![("CODEX_HOME".into(), "/acc/codex".into())]);

        let default_acc = account(
            Provider::Claude,
            Credentials::Subscription(ConfigFolder::Default),
        );
        assert!(LaunchSpec::shell("fish", Some(&default_acc)).env.is_empty());
        assert!(LaunchSpec::shell("fish", None).env.is_empty());
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
