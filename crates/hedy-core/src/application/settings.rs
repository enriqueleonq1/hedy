use std::str::FromStr;

use super::ports::McpServer;
use super::{AppError, AppResult, Hedy};
use crate::domain::metrics::ApiPrice;
use crate::domain::{AccountId, Provider, policy};
use crate::experiments::{self, Experiments};

/// User settings that are stored as plain key/value pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Account used where a project has none.
    DefaultAccount,
    /// Path to an official CLI binary when it is not on `PATH`.
    Binary(Provider),
    /// API endpoint for an API-key provider (e.g. Kimi China or Kimi Code).
    ApiUrl(Provider),
    /// Price of an API-key provider, `input/output` in USD per million tokens,
    /// to estimate what API tasks cost.
    ApiPrice(Provider),
    /// App theme mode: `system`, `light` or `dark`.
    Theme,
    /// Theme used in dark mode: a preset id or `custom:<id>`. Unset:
    /// [`DEFAULT_DARK_THEME`].
    ThemeDark,
    /// Theme used in light mode: a preset id or `custom:<id>`. Unset:
    /// [`DEFAULT_LIGHT_THEME`].
    ThemeLight,
    /// Name used to greet the user in the app.
    UserName,
    /// Interface language: `system`, `es` or `en`.
    Language,
    /// `true` once the user has gone through (or skipped) the onboarding.
    Onboarded,
    /// Desktop notifications about tasks: `on` (default) or `off`.
    Notifications,
    /// Code-map MCP server (codebase-memory-mcp) to register in every run.
    /// Unset: no code map.
    CodeMap,
    /// The context-saving package (docs/decisions.md §33): `on` (default) or
    /// `off`. `HEDY_EXPERIMENTS` wins over it when set.
    ContextSaving,
}

pub const THEMES: [&str; 3] = ["system", "light", "dark"];
pub const LANGUAGES: [&str; 3] = ["system", "es", "en"];
pub const SWITCH: [&str; 2] = ["on", "off"];
/// The dark-mode theme when none is chosen (a preset of the UI).
pub const DEFAULT_DARK_THEME: &str = "hedy";
/// The light-mode theme when none is chosen (a preset of the UI).
pub const DEFAULT_LIGHT_THEME: &str = "hedy-light";
/// How a theme setting points at a saved theme: `custom:<id>`.
pub const CUSTOM_THEME_PREFIX: &str = "custom:";
/// Longest preset or custom theme id in a theme setting.
const MAX_THEME_CHOICE: usize = 80;

/// A preset id or `custom:<id>`: 1–80 of `[a-z0-9-]`. Only the shape is
/// checked; presets live in the UI.
fn is_theme_choice(value: &str) -> bool {
    let id = value.strip_prefix(CUSTOM_THEME_PREFIX).unwrap_or(value);
    !id.is_empty()
        && id.len() <= MAX_THEME_CHOICE
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Setting {
    pub fn key(self) -> String {
        match self {
            Self::DefaultAccount => "default_account".into(),
            Self::Binary(p) => format!("binary.{}", p.as_str()),
            Self::ApiUrl(p) => format!("api_url.{}", p.as_str()),
            Self::ApiPrice(p) => format!("api_price.{}", p.as_str()),
            Self::Theme => "ui.theme".into(),
            Self::ThemeDark => "ui.theme.dark".into(),
            Self::ThemeLight => "ui.theme.light".into(),
            Self::UserName => "ui.name".into(),
            Self::Language => "ui.language".into(),
            Self::Onboarded => "ui.onboarded".into(),
            Self::Notifications => "ui.notifications".into(),
            Self::CodeMap => "code_map.binary".into(),
            Self::ContextSaving => "context_saving".into(),
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
            .flat_map(|p| [Self::ApiUrl(p), Self::ApiPrice(p)]);
        std::iter::once(Self::DefaultAccount)
            .chain(binaries)
            .chain(api_urls)
            .chain([
                Self::Theme,
                Self::ThemeDark,
                Self::ThemeLight,
                Self::UserName,
                Self::Language,
                Self::Onboarded,
                Self::Notifications,
                Self::CodeMap,
                Self::ContextSaving,
            ])
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
            (Setting::ApiPrice(_), Some(price)) => {
                price.parse::<ApiPrice>()?;
            }
            (Setting::Theme, Some(theme)) if !THEMES.contains(&theme) => {
                return Err(AppError::InvalidSettingValue {
                    key: setting.key(),
                    value: theme.to_owned(),
                });
            }
            (Setting::ThemeDark | Setting::ThemeLight, Some(theme)) if !is_theme_choice(theme) => {
                return Err(AppError::InvalidSettingValue {
                    key: setting.key(),
                    value: theme.to_owned(),
                });
            }
            (Setting::Language, Some(language)) if !LANGUAGES.contains(&language) => {
                return Err(AppError::InvalidSettingValue {
                    key: setting.key(),
                    value: language.to_owned(),
                });
            }
            (Setting::Notifications | Setting::ContextSaving, Some(value))
                if !SWITCH.contains(&value) =>
            {
                return Err(AppError::InvalidSettingValue {
                    key: setting.key(),
                    value: value.to_owned(),
                });
            }
            _ => {}
        }
        self.store.set_setting(&setting.key(), value)?;
        if setting == Setting::Language {
            self.apply_language()?;
        }
        if setting == Setting::ContextSaving {
            self.apply_context_saving()?;
        }
        Ok(())
    }

    /// Whether the context-saving package is on (the default).
    pub fn context_saving_on(&self) -> AppResult<bool> {
        Ok(self.setting(Setting::ContextSaving)?.as_deref() != Some("off"))
    }

    /// Runs this process with the package or without it, as the setting
    /// says, unless `HEDY_EXPERIMENTS` decides. Every process Hedy starts
    /// from now on gets the same.
    pub fn apply_context_saving(&self) -> AppResult<()> {
        if !experiments::environment_decides() {
            experiments::set(if self.context_saving_on()? {
                Experiments::DEFAULT
            } else {
                Experiments::NONE
            });
        }
        Ok(())
    }

    /// Uses the language setting (or the system's) for every text from now on.
    pub fn apply_language(&self) -> AppResult<()> {
        crate::i18n::init(self.setting(Setting::Language)?.as_deref());
        Ok(())
    }

    /// The theme each mode uses (`(dark, light)`), with the defaults when
    /// unset.
    pub fn mode_themes(&self) -> AppResult<(String, String)> {
        Ok((
            self.setting(Setting::ThemeDark)?
                .unwrap_or_else(|| DEFAULT_DARK_THEME.into()),
            self.setting(Setting::ThemeLight)?
                .unwrap_or_else(|| DEFAULT_LIGHT_THEME.into()),
        ))
    }

    /// Whether desktop notifications are on (the default).
    pub fn notifications_on(&self) -> AppResult<bool> {
        Ok(self.setting(Setting::Notifications)?.as_deref() != Some("off"))
    }

    pub(crate) fn default_account_id(&self) -> AppResult<Option<AccountId>> {
        Ok(self
            .setting(Setting::DefaultAccount)?
            .and_then(|id| AccountId::new(id).ok()))
    }

    /// The configured price of an API-key provider, if any.
    pub(crate) fn api_price(&self, provider: Provider) -> AppResult<Option<ApiPrice>> {
        Ok(self
            .setting(Setting::ApiPrice(provider))?
            .and_then(|p| p.parse().ok()))
    }

    pub(crate) fn binary_for(&self, provider: Provider) -> AppResult<Option<String>> {
        self.setting(Setting::Binary(provider))
    }

    /// The code map for a run, limited to the project folder. Hedy only
    /// registers it; the server indexes on its own.
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
        assert!("binary.nope".parse::<Setting>().is_err());
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
    fn mode_themes_have_defaults_and_a_checked_shape() {
        let f = Fixture::new();
        assert_eq!(
            f.hedy.mode_themes().unwrap(),
            ("hedy".into(), "hedy-light".into())
        );
        f.hedy
            .set_setting(Setting::ThemeDark, Some("custom:one-dark-pro"))
            .unwrap();
        f.hedy
            .set_setting(Setting::ThemeLight, Some("solarized-light"))
            .unwrap();
        assert_eq!(
            f.hedy.mode_themes().unwrap(),
            ("custom:one-dark-pro".into(), "solarized-light".into())
        );
        let long = "a".repeat(81);
        for bad in [
            "",
            "custom:",
            "Dracula",
            "one dark",
            "custom:x_y",
            "other:x",
            &long,
        ] {
            assert!(
                matches!(
                    f.hedy.set_setting(Setting::ThemeDark, Some(bad)),
                    Err(AppError::InvalidSettingValue { .. })
                ),
                "{bad}"
            );
        }
        assert!(
            f.hedy
                .set_setting(Setting::ThemeDark, Some(&"a".repeat(80)))
                .is_ok()
        );
        f.hedy.set_setting(Setting::ThemeLight, None).unwrap();
        assert_eq!(f.hedy.mode_themes().unwrap().1, "hedy-light");
    }

    #[test]
    fn notifications_are_on_unless_turned_off() {
        let f = Fixture::new();
        assert!(f.hedy.notifications_on().unwrap());
        f.hedy
            .set_setting(Setting::Notifications, Some("off"))
            .unwrap();
        assert!(!f.hedy.notifications_on().unwrap());
        assert!(matches!(
            f.hedy.set_setting(Setting::Notifications, Some("maybe")),
            Err(AppError::InvalidSettingValue { .. })
        ));
        f.hedy.set_setting(Setting::Notifications, None).unwrap();
        assert!(f.hedy.notifications_on().unwrap());
    }

    #[test]
    fn context_saving_is_on_by_default_and_switches_the_package() {
        let f = Fixture::new();
        assert!(f.hedy.context_saving_on().unwrap());
        f.hedy.apply_context_saving().unwrap();
        assert_eq!(experiments::current(), Experiments::DEFAULT);

        f.hedy
            .set_setting(Setting::ContextSaving, Some("off"))
            .unwrap();
        assert!(!f.hedy.context_saving_on().unwrap());
        assert_eq!(experiments::current(), Experiments::NONE);
        assert!(matches!(
            f.hedy.set_setting(Setting::ContextSaving, Some("maybe")),
            Err(AppError::InvalidSettingValue { .. })
        ));
        f.hedy.set_setting(Setting::ContextSaving, None).unwrap();
        assert_eq!(experiments::current(), Experiments::DEFAULT);
        assert_eq!(
            "context_saving".parse::<Setting>().unwrap(),
            Setting::ContextSaving
        );
        experiments::set(Experiments::NONE);
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
