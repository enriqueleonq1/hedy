//! Tauri commands for custom themes (Ajustes → Apariencia): the saved ones
//! and "Describir con IA". Which theme each mode uses is a setting
//! (`ui.theme.dark` / `ui.theme.light`, see `set_setting`).

use std::collections::BTreeMap;

use hedy_core::application::NewTheme;
use hedy_core::domain::{CustomTheme, DomainError, ThemeBase, ThemeDraft, ThemeOrigin};
use serde::Deserialize;
use tauri::State;

use crate::AppState;
use crate::commands::{CommandResult, blocking};

/// A theme to save; `id: null` makes a new one.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub base: ThemeBase,
    pub colors: BTreeMap<String, String>,
    pub origin: ThemeOrigin,
}

impl From<ThemeInput> for NewTheme {
    fn from(t: ThemeInput) -> Self {
        Self {
            id: t.id.filter(|id| !id.trim().is_empty()),
            name: t.name,
            base: t.base,
            colors: t.colors,
            origin: t.origin,
        }
    }
}

/// The saved themes, by name.
#[tauri::command]
pub async fn list_themes(state: State<'_, AppState>) -> CommandResult<Vec<CustomTheme>> {
    blocking(&state, |h| h.list_themes()).await
}

#[tauri::command]
pub async fn save_theme(
    state: State<'_, AppState>,
    theme: ThemeInput,
) -> CommandResult<CustomTheme> {
    blocking(&state, move |h| h.save_theme(theme.into())).await
}

/// A mode that used the theme goes back to its default.
#[tauri::command]
pub async fn delete_theme(state: State<'_, AppState>, id: String) -> CommandResult<()> {
    blocking(&state, move |h| h.delete_theme(&id)).await
}

/// "Describir con IA" with the chosen account (spends a few tokens). The
/// draft is not saved.
#[tauri::command]
pub async fn describe_theme(
    state: State<'_, AppState>,
    account: String,
    description: String,
    base: Option<String>,
) -> CommandResult<ThemeDraft> {
    let base = base
        .filter(|b| !b.trim().is_empty())
        .map(|b| b.parse::<ThemeBase>())
        .transpose()
        .map_err(|e: DomainError| e.to_string())?;
    blocking(&state, move |h| {
        h.describe_theme(&account, &description, base)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_input_from_camel_case() {
        let input: ThemeInput = serde_json::from_str(
            r##"{"id":null,"name":"Noche","base":"dark","colors":{"bg":"#000"},"origin":"vscode"}"##,
        )
        .unwrap();
        let theme = NewTheme::from(input);
        assert_eq!(theme.id, None);
        assert_eq!(theme.base, ThemeBase::Dark);
        assert_eq!(theme.origin, ThemeOrigin::Vscode);
        let input: ThemeInput = serde_json::from_str(
            r##"{"id":"noche","name":"Noche","base":"light","colors":{},"origin":"edited"}"##,
        )
        .unwrap();
        assert_eq!(NewTheme::from(input).id.as_deref(), Some("noche"));
    }
}
