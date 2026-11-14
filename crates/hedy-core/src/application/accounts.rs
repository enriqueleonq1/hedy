use super::ports::CapturedOutput;
use super::{AppError, AppResult, Hedy, Setting};
use crate::domain::{
    Account, AccountId, Color, ConfigFolder, Credentials, LaunchPurpose, LaunchSpec, LoginState,
    Provider,
};

/// Request to register an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount {
    pub name: String,
    pub provider: Provider,
    /// API key account instead of a subscription login.
    pub api_key: bool,
    /// Adopt the CLI's existing default folder (e.g. `~/.claude`) instead of
    /// creating a Hedy-managed one.
    pub default_folder: bool,
    /// Defaults to the next palette color.
    pub color: Option<Color>,
}

impl Hedy {
    pub fn accounts(&self) -> AppResult<Vec<Account>> {
        Ok(self.store.list_accounts()?)
    }

    pub fn account(&self, id: &str) -> AppResult<Account> {
        self.accounts()?
            .into_iter()
            .find(|a| a.id.as_str() == id)
            .ok_or_else(|| AppError::AccountNotFound(id.to_owned()))
    }

    /// Registers the account and prepares its folder. Does not log in: call
    /// [`Hedy::login`] (subscription) or [`Hedy::set_api_key`] (API) next.
    pub fn add_account(&self, request: NewAccount) -> AppResult<Account> {
        let existing = self.accounts()?;
        let id = AccountId::from_name(&request.name)?;
        if existing.iter().any(|a| a.id == id) {
            return Err(AppError::AccountExists(id));
        }

        let credentials = if request.api_key {
            Credentials::ApiKey
        } else if request.default_folder {
            let taken = existing.iter().any(|a| {
                a.provider == request.provider
                    && a.credentials == Credentials::Subscription(ConfigFolder::Default)
            });
            if taken {
                return Err(AppError::DefaultFolderTaken(
                    request.provider.display_name(),
                ));
            }
            Credentials::Subscription(ConfigFolder::Default)
        } else {
            Credentials::Subscription(ConfigFolder::Managed(self.fs.account_folder(&id)))
        };
        let color = request
            .color
            .unwrap_or_else(|| Color::nth_in_palette(existing.len()));
        let account = Account::new(id, request.name, request.provider, credentials, color)?;

        if let Credentials::Subscription(ConfigFolder::Managed(folder)) = &account.credentials {
            self.fs.create_dir_all(folder)?;
            for (file, contents) in account.provider.folder_bootstrap() {
                self.fs.write_if_missing(&folder.join(file), contents)?;
            }
        }
        self.store.save_account(&account)?;
        Ok(account)
    }

    /// The CLI's official login flow, for a driving adapter that runs it in its
    /// own terminal (the app's PTY).
    pub fn login_spec(&self, id: &str) -> AppResult<LaunchSpec> {
        let account = self.account(id)?;
        if !account.is_subscription() {
            return Err(AppError::NotASubscription(account.id));
        }
        let program = self.binary_for(account.provider)?;
        Ok(
            LaunchSpec::for_account(&account, LaunchPurpose::Login, program.as_deref())?
                .expect("subscription providers always have a login command"),
        )
    }

    /// Runs the CLI's official login flow attached to the current terminal.
    pub fn login(&self, id: &str) -> AppResult<Option<i32>> {
        Ok(self.runner.run_interactive(&self.login_spec(id)?)?)
    }

    pub fn set_account_color(&self, id: &str, color: Color) -> AppResult<Account> {
        let mut account = self.account(id)?;
        account.color = color;
        self.store.save_account(&account)?;
        Ok(account)
    }

    /// Asks the CLI (or the keyring) whether the account is ready to use.
    pub fn login_state(&self, account: &Account) -> AppResult<LoginState> {
        if !account.is_subscription() {
            return Ok(if self.secrets.has_api_key(&account.id)? {
                LoginState::LoggedIn { plan: None }
            } else {
                LoginState::LoggedOut
            });
        }
        let program = self.binary_for(account.provider)?;
        let Some(spec) =
            LaunchSpec::for_account(account, LaunchPurpose::LoginStatus, program.as_deref())?
        else {
            return Ok(LoginState::Unknown);
        };
        // A missing binary is not an error for status: the account is just unusable.
        let Ok(CapturedOutput { exit_code, stdout }) = self.runner.run_captured(&spec) else {
            return Ok(LoginState::Unknown);
        };
        Ok(LoginState::from_status_output(
            account.provider,
            exit_code,
            &stdout,
        ))
    }

    /// Registers an account defined in another Hedy data folder as it is (same
    /// id, folder and color). Evals use it to run on a separate database.
    pub fn adopt_account(&self, account: &Account) -> AppResult<()> {
        if self.accounts()?.iter().any(|a| a.id == account.id) {
            return Err(AppError::AccountExists(account.id.clone()));
        }
        Ok(self.store.save_account(account)?)
    }

    pub fn set_api_key(&self, id: &str, key: &str) -> AppResult<()> {
        let account = self.account(id)?;
        if account.is_subscription() {
            return Err(AppError::NotAnApiAccount(account.id));
        }
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::EmptyApiKey);
        }
        Ok(self.secrets.set_api_key(&account.id, key)?)
    }

    /// Unregisters the account. Its folder (and the login inside) is left untouched.
    pub fn remove_account(&self, id: &str) -> AppResult<Account> {
        let account = self.account(id)?;
        let projects: Vec<_> = self
            .projects()?
            .into_iter()
            .filter(|p| p.account.as_ref() == Some(&account.id))
            .map(|p| p.path)
            .collect();
        if !projects.is_empty() {
            return Err(AppError::AccountInUse {
                account: account.id,
                projects,
            });
        }
        if !account.is_subscription() {
            self.secrets.delete_api_key(&account.id)?;
        }
        if self.default_account_id()?.as_ref() == Some(&account.id) {
            self.store
                .set_setting(&Setting::DefaultAccount.key(), None)?;
        }
        self.store.delete_account(&account.id)?;
        Ok(account)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::application::testing::Fixture;

    #[test]
    fn subscription_account_gets_a_managed_folder() {
        let f = Fixture::new();
        let account = f.add_claude("Claude personal");
        assert_eq!(account.id.as_str(), "claude-personal");
        let folder = PathBuf::from("/data/accounts/claude-personal");
        assert_eq!(
            account.credentials,
            Credentials::Subscription(ConfigFolder::Managed(folder.clone()))
        );
        assert!(f.fs.dirs().contains(&folder));
    }

    #[test]
    fn codex_folder_is_bootstrapped_for_file_credentials() {
        let f = Fixture::new();
        f.hedy
            .add_account(NewAccount {
                name: "ChatGPT".into(),
                provider: Provider::Codex,
                api_key: false,
                default_folder: false,
                color: None,
            })
            .unwrap();
        let files = f.fs.files();
        let (path, contents) = files.first().unwrap();
        assert_eq!(path, &PathBuf::from("/data/accounts/chatgpt/config.toml"));
        assert!(contents.contains("\"file\""));
    }

    #[test]
    fn default_folder_can_only_be_adopted_once_per_provider() {
        let f = Fixture::new();
        let request = |name: &str| NewAccount {
            name: name.into(),
            provider: Provider::Claude,
            api_key: false,
            default_folder: true,
            color: None,
        };
        let work = f.hedy.add_account(request("Trabajo")).unwrap();
        assert_eq!(
            work.credentials,
            Credentials::Subscription(ConfigFolder::Default)
        );
        assert!(f.fs.dirs().is_empty());
        assert!(matches!(
            f.hedy.add_account(request("Otra")),
            Err(AppError::DefaultFolderTaken(_))
        ));
    }

    #[test]
    fn duplicate_ids_are_rejected_and_colors_rotate() {
        let f = Fixture::new();
        let a = f.add_claude("Personal");
        let b = f.add_claude("Trabajo");
        assert_ne!(a.color, b.color);
        assert!(matches!(
            f.hedy.add_account(NewAccount {
                name: "personal".into(),
                provider: Provider::Codex,
                api_key: false,
                default_folder: false,
                color: None,
            }),
            Err(AppError::AccountExists(_))
        ));
    }

    #[test]
    fn login_runs_the_official_flow_in_the_account_folder() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy.login("personal").unwrap();
        let spec = f.runner.interactive().pop().unwrap();
        assert_eq!(spec.program, "claude");
        assert_eq!(spec.args, vec!["auth", "login"]);
        assert_eq!(
            spec.env,
            vec![("CLAUDE_CONFIG_DIR".into(), "/data/accounts/personal".into())]
        );
    }

    #[test]
    fn account_color_can_change() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let pink: Color = "rosa".parse().unwrap();
        f.hedy.set_account_color("personal", pink).unwrap();
        assert_eq!(f.hedy.account("personal").unwrap().color, pink);
    }

    #[test]
    fn login_state_comes_from_the_cli() {
        let f = Fixture::new();
        let account = f.add_claude("Personal");
        f.runner
            .reply(r#"{"loggedIn":true,"subscriptionType":"max"}"#, 0);
        assert_eq!(
            f.hedy.login_state(&account).unwrap(),
            LoginState::LoggedIn {
                plan: Some("max".into())
            }
        );
    }

    #[test]
    fn api_keys_go_to_the_secret_store() {
        let f = Fixture::new();
        let kimi = f.add_kimi("Kimi");
        assert_eq!(f.hedy.login_state(&kimi).unwrap(), LoginState::LoggedOut);
        assert!(matches!(
            f.hedy.set_api_key("kimi", "  "),
            Err(AppError::EmptyApiKey)
        ));
        f.hedy.set_api_key("kimi", " sk-123 ").unwrap();
        assert_eq!(f.secrets.get("kimi").as_deref(), Some("sk-123"));
        assert!(matches!(
            f.hedy.login_state(&kimi).unwrap(),
            LoginState::LoggedIn { .. }
        ));
        assert!(matches!(
            f.hedy.login("kimi"),
            Err(AppError::NotASubscription(_))
        ));
    }

    #[test]
    fn removing_requires_no_assigned_projects() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .use_account(&f.dir("/code/hedy"), "personal", None)
            .unwrap();
        assert!(matches!(
            f.hedy.remove_account("personal"),
            Err(AppError::AccountInUse { .. })
        ));

        let kimi = f.add_kimi("Kimi");
        f.hedy.set_api_key("kimi", "sk").unwrap();
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("kimi"))
            .unwrap();
        assert_eq!(f.hedy.remove_account("kimi").unwrap(), kimi);
        assert_eq!(f.secrets.get("kimi"), None);
        assert_eq!(f.hedy.setting(Setting::DefaultAccount).unwrap(), None);
    }
}
