use std::str::FromStr;

use super::{AppError, AppResult, Hedy};
use crate::domain::{AccountId, Provider, policy};

/// User settings that are stored as plain key/value pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Account used where a project has none.
    DefaultAccount,
    /// Path to an official CLI binary when it is not on `PATH`.
    Binary(Provider),
}

impl Setting {
    pub fn key(self) -> String {
        match self {
            Self::DefaultAccount => "default_account".into(),
            Self::Binary(p) => format!("binary.{}", p.as_str()),
        }
    }

    pub fn all() -> Vec<Self> {
        let binaries = Provider::ALL
            .into_iter()
            .filter(|p| p.program().is_some())
            .map(Self::Binary);
        std::iter::once(Self::DefaultAccount)
            .chain(binaries)
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
        if let (Setting::DefaultAccount, Some(id)) = (setting, value) {
            let account = self.account(id)?;
            // A reserved account cannot be offered everywhere.
            policy::check_launch(None, &account.id, &self.projects()?)?;
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
