//! Custom themes (Ajustes → Apariencia): the ones the user saved, and "Describir
//! con IA", a one-shot request to one of the user's accounts for a theme's
//! seed colors. Presets and VS Code imports live in the UI.

use std::collections::BTreeMap;
use std::time::Duration;

use super::one_shot::{OneShot, OneShotFailure};
use super::settings::CUSTOM_THEME_PREFIX;
use super::{AppError, AppResult, Hedy, Setting};
use crate::domain::theme::{
    self, CustomTheme, ThemeBase, ThemeDraft, ThemeOrigin, new_theme_id, parse_theme_answer,
    theme_description, theme_name,
};
use crate::domain::{Phase, Stage, policy};
use crate::tr;

/// How long the AI gets to describe a theme.
const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(120);

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

    /// "Describir con IA": `account` (never another one: rule 3) proposes a
    /// theme's seed colors from `description`, read-only and without tools.
    /// `base` (if any) is the mode it must suit. Spends a few tokens, counted
    /// for the account. The draft is not saved.
    pub fn describe_theme(
        &self,
        account: &str,
        description: &str,
        base: Option<ThemeBase>,
    ) -> AppResult<ThemeDraft> {
        self.describe_theme_within(account, description, base, DESCRIBE_TIMEOUT)
    }

    pub(super) fn describe_theme_within(
        &self,
        account: &str,
        description: &str,
        base: Option<ThemeBase>,
        timeout: Duration,
    ) -> AppResult<ThemeDraft> {
        let description = theme_description(description)?;
        let account = self.account(account)?;
        // Outside any project: an account reserved by a protected one is not used.
        policy::check_launch(None, &account.id, &self.projects()?)?;
        if self.account_at_limit(&account)? {
            return Err(AppError::ThemeAtLimit {
                account: account.name.clone(),
            });
        }
        let stage = Stage {
            phase: Phase::Review,
            account: account.id.clone(),
            model: None,
        };
        let answer = self.one_shot(OneShot {
            task: None,
            account: &account,
            stage: &stage,
            cwd: self.themes.work_dir()?,
            prompt: describe_prompt(&description, base),
            refusal: tr!(
                "No uses herramientas: responde solo con el JSON del tema.",
                "Do not use tools: reply with the theme's JSON only."
            ),
            no_shell: true,
            no_tools: false,
            timeout,
        })?;
        match answer {
            Ok(text) => parse_theme_answer(&text, &description, base)
                .ok_or(AppError::ThemeDescriptionFailed(None)),
            Err(OneShotFailure::AtLimit) => Err(AppError::ThemeAtLimit {
                account: account.name.clone(),
            }),
            Err(OneShotFailure::Failed(detail)) => Err(AppError::ThemeDescriptionFailed(detail)),
            Err(OneShotFailure::TimedOut) => Err(AppError::ThemeDescriptionTimedOut),
        }
    }
}

/// Each seed's role, in the order the prompt lists them.
const SEED_ROLES: [(&str, &str); 18] = [
    ("bg", "page background"),
    ("panel", "cards and sidebar"),
    ("panel2", "nested surfaces inside panels"),
    ("ink", "main text"),
    ("muted", "secondary text"),
    ("line", "borders and dividers"),
    ("accent", "accent text and links"),
    ("action", "primary button fill"),
    ("success", "success status"),
    ("warning", "warning status"),
    ("danger", "error status"),
    ("info", "info status"),
    ("keyword", "code syntax: keywords"),
    ("string", "code syntax: strings"),
    ("number", "code syntax: numbers"),
    ("function", "code syntax: function names"),
    ("type", "code syntax: types"),
    ("comment", "code syntax: comments"),
];

/// What the account is asked (in English, whatever the UI language).
fn describe_prompt(description: &str, base: Option<ThemeBase>) -> String {
    debug_assert!(
        SEED_ROLES
            .iter()
            .map(|(k, _)| *k)
            .eq(theme::SEED_KEYS.iter().copied())
    );
    let roles: String = SEED_ROLES
        .iter()
        .map(|(key, role)| format!("- {key}: {role}\n"))
        .collect();
    let base_rule = match base {
        Some(base) => format!("\"base\" must be \"{base}\": design it for {base} mode.\n"),
        None => "\"base\" is \"dark\" or \"light\", whichever suits the description.\n".into(),
    };
    format!(
        "Design a color theme for a desktop app with a code editor, from this description:\n\
         \"\"\"\n{description}\n\"\"\"\n\n\
         Reply with ONLY a JSON object, no prose and no code fence, with these keys:\n\
         - name: a short name for the theme (at most 60 characters)\n\
         - base: \"dark\" or \"light\"\n\
         {roles}\
         Every color is a hex string like \"#1E1E2E\".\n\
         {base_rule}\
         Keep text readable: ink, muted and accent need a contrast of at least 4.5:1 \
         on both bg and panel.\n\
         If the description names a known editor theme (for example One Dark Pro or \
         Dracula), use that theme's real colors.\n\
         Do not use any tool."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{RateLimit, ThemeStore, UsageRepository};
    use crate::application::testing::Fixture;
    use crate::domain::{AgentEvent, DomainError, TaskMode, UsageSample};

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

    const ANSWER: &str = r##"Sure! ```json
{"name": "One Dark Pro", "base": "dark", "bg": "#282c34", "panel": "#21252b",
 "ink": "#abb2bf", "accent": "#61afef", "keyword": "#c678dd", "glow": "#fff"}
```"##;

    fn finished(summary: &str) -> AgentEvent {
        AgentEvent::Finished {
            ok: true,
            summary: summary.into(),
            duration_ms: None,
        }
    }

    #[test]
    fn describing_uses_the_chosen_account_read_only() {
        let f = Fixture::new();
        let account = f.add_claude("Personal");
        f.agents.reply_on_start(vec![
            AgentEvent::Usage(UsageSample {
                input_tokens: 40,
                output_tokens: 9,
                ..Default::default()
            }),
            finished(ANSWER),
        ]);
        let draft = f
            .hedy
            .describe_theme("personal", "  like One Dark Pro ", None)
            .unwrap();
        assert_eq!(draft.name, "One Dark Pro");
        assert_eq!(draft.base, ThemeBase::Dark);
        assert_eq!(
            draft.colors,
            colors(&[
                ("accent", "#61AFEF"),
                ("bg", "#282C34"),
                ("ink", "#ABB2BF"),
                ("keyword", "#C678DD"),
                ("panel", "#21252B"),
            ])
        );
        let request = f.agents.last_request();
        assert_eq!(request.mode, TaskMode::Ask);
        assert!(request.memory.is_none() && request.mcp.is_empty() && request.resume.is_none());
        assert_eq!(request.launch.cwd, Some("/data/themes".into()));
        assert!(request.prompt.contains("\"\"\"\nlike One Dark Pro\n\"\"\""));
        assert!(request.prompt.contains("- panel2: nested surfaces"));
        assert!(request.prompt.contains("4.5:1"));
        assert_eq!(request.launch.env[0].0, "CLAUDE_CONFIG_DIR");
        // The tokens count for the account, under no project.
        assert_eq!(
            f.store.usage_since(&account.id, 0).unwrap().input_tokens,
            40
        );
        assert!(f.hedy.usage_summary(30).unwrap().projects.is_empty());
        // Nothing was saved.
        assert!(f.hedy.list_themes().unwrap().is_empty());
    }

    #[test]
    fn a_requested_base_is_asked_and_kept() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.agents.reply_on_start(vec![finished(ANSWER)]);
        let draft = f
            .hedy
            .describe_theme("personal", "One Dark Pro", Some(ThemeBase::Light))
            .unwrap();
        assert_eq!(draft.base, ThemeBase::Light);
        assert!(
            f.agents
                .last_request()
                .prompt
                .contains("\"base\" must be \"light\"")
        );
    }

    #[test]
    fn describing_fails_clearly() {
        let f = Fixture::new();
        let account = f.add_claude("Personal");
        assert!(matches!(
            f.hedy.describe_theme("personal", "   ", None),
            Err(AppError::Domain(
                DomainError::InvalidThemeDescription { .. }
            ))
        ));
        assert!(matches!(
            f.hedy.describe_theme("ghost", "calm", None),
            Err(AppError::AccountNotFound(_))
        ));
        assert!(f.agents.requests().is_empty(), "no agent started");

        f.agents
            .reply_on_start(vec![finished("I cannot do colors, sorry.")]);
        assert!(matches!(
            f.hedy.describe_theme("personal", "calm", None),
            Err(AppError::ThemeDescriptionFailed(None))
        ));
        f.agents.reply_on_start(vec![AgentEvent::Error {
            message: "boom".into(),
        }]);
        assert!(matches!(
            f.hedy.describe_theme("personal", "calm", None),
            Err(AppError::ThemeDescriptionFailed(Some(m))) if m == "boom"
        ));
        f.agents.reply_on_start(vec![]);
        assert!(matches!(
            f.hedy
                .describe_theme_within("personal", "calm", None, Duration::from_millis(20)),
            Err(AppError::ThemeDescriptionTimedOut)
        ));

        // Rule 3: at its limit, no agent starts and no other account is used.
        f.add_claude("Trabajo");
        let started = f.agents.requests().len();
        f.store
            .set_rate_limit(
                &account.id,
                &RateLimit {
                    status: "rejected".into(),
                    utilization: None,
                    resets_at: Some(f.hedy.now_secs() + 600),
                    at: f.hedy.now_secs(),
                },
            )
            .unwrap();
        assert!(matches!(
            f.hedy.describe_theme("personal", "calm", None),
            Err(AppError::ThemeAtLimit { account }) if account == "Personal"
        ));
        assert_eq!(f.agents.requests().len(), started, "no agent started");
    }

    #[test]
    fn a_limit_reported_while_answering_stops_it() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.agents.reply_on_start(vec![
            AgentEvent::RateLimit {
                status: "rejected".into(),
                utilization: None,
                resets_at: None,
            },
            finished(ANSWER),
        ]);
        assert!(matches!(
            f.hedy.describe_theme("personal", "calm", None),
            Err(AppError::ThemeAtLimit { .. })
        ));
    }
}
