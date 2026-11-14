use std::str::FromStr;

use super::ports::McpServer;
use super::{AppError, AppResult, Hedy};
use crate::domain::{AccountId, Provider, policy};

/// User settings that are stored as plain key/value pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Account used where a project has none.
    DefaultAccount,
    /// Path to an official CLI binary when it is not on `PATH`.
    Binary(Provider),
    /// API endpoint for an API-key provider (e.g. Kimi China or Kimi Code).
    ApiUrl(Provider),
    /// App theme: `system`, `light` or `dark`.
    Theme,
    /// Name used to greet the user in the app.
    UserName,
    /// Code-map MCP server (codebase-memory-mcp) to register in every run.
    /// Unset: no code map.
    CodeMap,
}

pub const THEMES: [&str; 3] = ["system", "light", "dark"];

impl Setting {
    pub fn key(self) -> String {
        match self {
            Self::DefaultAccount => "default_account".into(),
            Self::Binary(p) => format!("binary.{}", p.as_str()),
            Self::ApiUrl(p) => format!("api_url.{}", p.as_str()),
            Self::Theme => "ui.theme".into(),
            Self::UserName => "ui.name".into(),
            Self::CodeMap => "code_map.binary".into(),
        }
    }

    pub fn all() -> Vec<Self> {
        let binaries = Provider::ALL
            .into_iter()
            .filter(|p| p.program().is_some())
            .map(Self::Binary);
        let api_urls = Provider::ALL
            .into_iter()
            .filter(|p| p.supports_api_key())
            .map(Self::ApiUrl);
        std::iter::once(Self::DefaultAccount)
            .chain(binaries)
            .chain(api_urls)
            .chain([Self::Theme, Self::UserName, Self::CodeMap])
            .collect()
    }
}

impl FromStr for Setting {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::all()
            .into_iter()
            .find(|setting| setting.key() == s)
            .ok_or_else(|| AppError::UnknownSetting(s.to_owned()))
    }
}

impl Hedy {
    pub fn setting(&self, setting: Setting) -> AppResult<Option<String>> {
        Ok(self.store.get_setting(&setting.key())?)
    }

    /// `None` clears the setting.
    pub fn set_setting(&self, setting: Setting, value: Option<&str>) -> AppResult<()> {
        match (setting, value) {
            (Setting::DefaultAccount, Some(id)) => {
                let account = self.account(id)?;
                // A reserved account cannot be offered everywhere.
                policy::check_launch(None, &account.id, &self.projects()?)?;
            }
            (Setting::Theme, Some(theme)) if !THEMES.contains(&theme) => {
                return Err(AppError::InvalidSettingValue {
                    key: setting.key(),
                    value: theme.to_owned(),
                });
            }
            _ => {}
        }
        Ok(self.store.set_setting(&setting.key(), value)?)
    }

    pub(crate) fn default_account_id(&self) -> AppResult<Option<AccountId>> {
        Ok(self
            .setting(Setting::DefaultAccount)?
            .and_then(|id| AccountId::new(id).ok()))
    }

    pub(crate) fn binary_for(&self, provider: Provider) -> AppResult<Option<String>> {
        self.setting(Setting::Binary(provider))
    }

    /// The code map for a run, limited to the project folder. Hedy only
    /// registers it; the server indexes on its own (spec phase 5).
    pub(crate) fn code_map_servers(&self, project: &std::path::Path) -> AppResult<Vec<McpServer>> {
        Ok(self
            .setting(Setting::CodeMap)?
            .filter(|program| !program.trim().is_empty())
            .map(|program| McpServer {
                name: "codebase-memory".into(),
                command: program,
                args: Vec::new(),
                env: vec![(
                    "CBM_ALLOWED_ROOT".into(),
                    project.to_string_lossy().into_owned(),
                )],
            })
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::testing::Fixture;

    #[test]
    fn keys_round_trip() {
        for s in Setting::all() {
            assert_eq!(s.key().parse::<Setting>().unwrap(), s);
        }
        assert!("binary.kimi".parse::<Setting>().is_err());
    }

    #[test]
    fn theme_is_validated() {
        let f = Fixture::new();
        f.hedy.set_setting(Setting::Theme, Some("light")).unwrap();
        assert!(matches!(
            f.hedy.set_setting(Setting::Theme, Some("pink")),
            Err(AppError::InvalidSettingValue { .. })
        ));
        assert_eq!(
            f.hedy.setting(Setting::Theme).unwrap().as_deref(),
            Some("light")
        );
    }

    #[test]
    fn default_account_must_exist() {
        let f = Fixture::new();
        assert!(matches!(
            f.hedy.set_setting(Setting::DefaultAccount, Some("ghost")),
            Err(AppError::AccountNotFound(_))
        ));
        f.add_claude("Personal");
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("personal"))
            .unwrap();
        assert_eq!(
            f.hedy.setting(Setting::DefaultAccount).unwrap().as_deref(),
            Some("personal")
        );
    }
}
