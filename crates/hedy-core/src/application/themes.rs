//! Custom themes (Ajustes → Apariencia): the ones the user saved. Presets and
//! VS Code imports live in the UI.

use std::collections::BTreeMap;

use super::settings::CUSTOM_THEME_PREFIX;
use super::{AppError, AppResult, Hedy, Setting};
use crate::domain::theme::{CustomTheme, ThemeBase, ThemeOrigin, new_theme_id, theme_name};

/// A theme to save: `id: None` makes a new one (its id comes from the name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTheme {
    pub id: Option<String>,
    pub name: String,
    pub base: ThemeBase,
    pub colors: BTreeMap<String, String>,
    pub origin: ThemeOrigin,
}

impl Hedy {
    /// The saved themes, by name.
    pub fn list_themes(&self) -> AppResult<Vec<CustomTheme>> {
        let mut themes = self.themes.list()?;
        themes.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(themes)
    }

    /// Saves a new theme, or replaces the one with `input.id`.
    pub fn save_theme(&self, input: NewTheme) -> AppResult<CustomTheme> {
        let id = match input.id {
            Some(id) => {
                if self.themes.get(&id)?.is_none() {
                    return Err(AppError::ThemeNotFound(id));
                }
                id
            }
            None => {
                // A broken file (an error here) keeps its id too.
                let name = theme_name(&input.name)?;
                new_theme_id(&name, |id| !matches!(self.themes.get(id), Ok(None)))
            }
        };
        let theme = CustomTheme::new(id, &input.name, input.base, input.colors, input.origin)?;
        self.themes.save(&theme)?;
        Ok(theme)
    }

    /// Deletes a theme (nothing happens when it is gone); a mode that used
    /// it goes back to its default theme.
    pub fn delete_theme(&self, id: &str) -> AppResult<()> {
        self.themes.delete(id)?;
        let chosen = format!("{CUSTOM_THEME_PREFIX}{id}");
        for setting in [Setting::ThemeDark, Setting::ThemeLight] {
            if self.setting(setting)?.as_deref() == Some(chosen.as_str()) {
                self.set_setting(setting, None)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::ThemeStore;
    use crate::application::testing::Fixture;
    use crate::domain::DomainError;

    fn colors(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn new(name: &str) -> NewTheme {
        NewTheme {
            id: None,
            name: name.into(),
            base: ThemeBase::Dark,
            colors: colors(&[("bg", "#282a36"), ("ink", "#f8f8f2")]),
            origin: ThemeOrigin::Vscode,
        }
    }

    #[test]
    fn themes_are_saved_with_ids_from_their_names() {
        let f = Fixture::new();
        let dracula = f.hedy.save_theme(new("Dracula")).unwrap();
        assert_eq!(dracula.id, "dracula");
        assert_eq!(dracula.colors["bg"], "#282A36");
        let again = f.hedy.save_theme(new("  Dracula ")).unwrap();
        assert_eq!(again.id, "dracula-2");
        assert_eq!(again.name, "Dracula");
        f.hedy.save_theme(new("alba")).unwrap();
        let names: Vec<_> = f
            .hedy
            .list_themes()
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(names, ["alba", "dracula", "dracula-2"]);

        // Editing keeps the id.
        let edited = f
            .hedy
            .save_theme(NewTheme {
                id: Some("dracula".into()),
                name: "Drácula editado".into(),
                origin: ThemeOrigin::Edited,
                ..new("x")
            })
            .unwrap();
        assert_eq!(edited.id, "dracula");
        assert_eq!(
            f.themes.get("dracula").unwrap().unwrap().name,
            "Drácula editado"
        );
        assert!(matches!(
            f.hedy.save_theme(NewTheme {
                id: Some("ghost".into()),
                ..new("Ghost")
            }),
            Err(AppError::ThemeNotFound(id)) if id == "ghost"
        ));
    }

    #[test]
    fn invalid_themes_are_not_saved() {
        let f = Fixture::new();
        assert!(matches!(
            f.hedy.save_theme(new("  ")),
            Err(AppError::Domain(DomainError::InvalidThemeName(_)))
        ));
        assert!(matches!(
            f.hedy.save_theme(NewTheme {
                colors: colors(&[("bg", "oscuro")]),
                ..new("Malo")
            }),
            Err(AppError::Domain(DomainError::InvalidThemeColor { .. }))
        ));
        assert!(f.hedy.list_themes().unwrap().is_empty());
    }

    #[test]
    fn deleting_a_theme_resets_the_modes_that_used_it() {
        let f = Fixture::new();
        let theme = f.hedy.save_theme(new("Dracula")).unwrap();
        f.hedy.save_theme(new("Otro")).unwrap();
        f.hedy
            .set_setting(Setting::ThemeDark, Some("custom:dracula"))
            .unwrap();
        f.hedy
            .set_setting(Setting::ThemeLight, Some("custom:otro"))
            .unwrap();
        f.hedy.delete_theme(&theme.id).unwrap();
        assert_eq!(
            f.hedy.mode_themes().unwrap(),
            ("tokyo-night".into(), "custom:otro".into())
        );
        assert_eq!(f.hedy.list_themes().unwrap().len(), 1);
        // Already gone: nothing happens.
        f.hedy.delete_theme(&theme.id).unwrap();
    }
}
