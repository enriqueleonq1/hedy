//! How a project's «Cambiar código» tasks work with one AI (docs/decisions.md
//! §59, §60): whether the AI waits for the user's OK on a plan, and which
//! checks Hedy runs once it is done. The same AI works the whole task, in one
//! session; flows with several AIs are apart.

use serde::{Deserialize, Serialize};

use crate::tr;

/// A project's stops. Every one off: as Hedy always worked («Enseguida»).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkingStyle {
    /// Proposes a plan and waits for the user's OK before changing code.
    pub plan: bool,
    /// Hedy runs the project's tests once the AI is done.
    pub tests: bool,
    /// Hedy runs the user's end-to-end check after the tests.
    pub e2e: bool,
    /// Another AI reads the change with a clean context (§57 B).
    pub review: bool,
    /// The tests' command; `None`: the one Hedy finds in the project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests_command: Option<String>,
    /// The end-to-end check's command. Only the user writes it: without it,
    /// no e2e runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e2e_command: Option<String>,
}

/// The ways of working Hedy offers by name. Any other set of stops is
/// «Personalizada».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// Implements and leaves the changes («Enseguida»).
    Now,
    /// Plan, the user's OK, implement, tests and e2e («Tras tu OK»).
    AfterOk,
    /// As `AfterOk`, and a review with a clean context.
    AfterOkReview,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Self::Now, Self::AfterOk, Self::AfterOkReview];

    pub fn key(self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::AfterOk => "after-ok",
            Self::AfterOkReview => "after-ok-review",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    pub fn name(self) -> String {
        match self {
            Self::Now => tr!("Enseguida", "Right away"),
            Self::AfterOk => tr!("Tras tu OK", "After your OK"),
            Self::AfterOkReview => tr!("Tras tu OK y una revisión", "After your OK, and a review"),
        }
    }

    /// `style` with this preset's stops; its commands stay.
    pub fn apply(self, style: &WorkingStyle) -> WorkingStyle {
        let (plan, checks, review) = match self {
            Self::Now => (false, false, false),
            Self::AfterOk => (true, true, false),
            Self::AfterOkReview => (true, true, true),
        };
        WorkingStyle {
            plan,
            tests: checks,
            e2e: checks,
            review,
            ..style.clone()
        }
    }
}

impl WorkingStyle {
    /// Where every project starts: «Tras tu OK». New tasks always offered
    /// the plan first; now Hedy also runs the tests it finds, which spends
    /// no tokens.
    pub fn initial() -> Self {
        Preset::AfterOk.apply(&Self::default())
    }

    /// The preset these stops are, or `None` («Personalizada»).
    pub fn preset(&self) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| &p.apply(self) == self)
    }

    /// The name the user sees: a preset's, or «Personalizada».
    pub fn name(&self) -> String {
        self.preset()
            .map(Preset::name)
            .unwrap_or_else(|| tr!("Personalizada", "Custom"))
    }

    /// Whether Hedy runs anything once the AI is done.
    pub fn checks(&self) -> bool {
        self.tests || (self.e2e && self.e2e_command.is_some()) || self.review
    }

    /// Trimmed commands; an empty one is none.
    pub fn normalized(mut self) -> Self {
        let clean = |c: Option<String>| c.map(|c| c.trim().to_owned()).filter(|c| !c.is_empty());
        self.tests_command = clean(self.tests_command);
        self.e2e_command = clean(self.e2e_command);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_starts_after_your_ok() {
        assert_eq!(WorkingStyle::initial().preset(), Some(Preset::AfterOk));
    }

    #[test]
    fn every_stop_off_is_right_away() {
        let style = WorkingStyle::default();
        assert_eq!(style.preset(), Some(Preset::Now));
        assert!(!style.checks());
    }

    #[test]
    fn a_preset_keeps_the_commands_and_any_other_set_is_custom() {
        let mine = WorkingStyle {
            e2e_command: Some("npm run e2e".into()),
            ..Default::default()
        };
        let after = Preset::AfterOk.apply(&mine);
        assert!(after.plan && after.tests && after.e2e && !after.review);
        assert_eq!(after.e2e_command.as_deref(), Some("npm run e2e"));
        assert_eq!(after.preset(), Some(Preset::AfterOk));
        assert!(after.checks());

        let custom = WorkingStyle {
            tests: false,
            ..after
        };
        assert_eq!(custom.preset(), None);
        assert_eq!(custom.name(), tr!("Personalizada", "Custom"));
        for preset in Preset::ALL {
            assert_eq!(Preset::from_key(preset.key()), Some(preset));
            assert_eq!(preset.apply(&custom).preset(), Some(preset));
        }
    }

    #[test]
    fn an_e2e_stop_without_its_command_runs_nothing() {
        let style = WorkingStyle {
            e2e: true,
            ..Default::default()
        };
        assert!(!style.checks());
    }

    #[test]
    fn empty_commands_are_none() {
        let style = WorkingStyle {
            tests_command: Some("  ".into()),
            e2e_command: Some(" npm run e2e ".into()),
            ..Default::default()
        }
        .normalized();
        assert_eq!(style.tests_command, None);
        assert_eq!(style.e2e_command.as_deref(), Some("npm run e2e"));
    }

    #[test]
    fn it_reads_with_missing_fields() {
        let style: WorkingStyle = serde_json::from_str(r#"{"plan":true}"#).unwrap();
        assert!(style.plan && !style.tests);
    }
}
