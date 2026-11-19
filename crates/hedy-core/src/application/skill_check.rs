//! Whether a skill helps or gets in the way, before it is used: what Hedy
//! measures on its own (level 1, free, for every skill being imported) and
//! what an AI sees against the project's notes (level 2: one read-only
//! request through the project's account, like checking a change against
//! the rules; only the skill and the notes go, never the code).

use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::ask::AskScope;
use super::one_shot::{OneShot, OneShotFailure};
use super::{AppError, AppResult, Hedy};
use crate::domain::rule_check::pick_rules_for_text;
use crate::domain::skill::check_visible;
use crate::domain::skill_check::{
    Finding, SkillText, Verdict, measure, parse_review, predicted, review_prompt,
};
use crate::domain::{Account, Phase, Provider, Skill, Stage};
use crate::tr;

/// How long the AI may take: the skill can be long.
const REVIEW_TIMEOUT: Duration = Duration::from_secs(120);

/// What the AI saw in a skill (level 2).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillReview {
    pub findings: Vec<Finding>,
    /// The answer could not be read: nothing was judged.
    pub unread: bool,
    /// How many of the project's notes went with the skill.
    pub notes: usize,
    #[serde(skip)]
    pub account: Option<Account>,
    pub duration_ms: u64,
    /// What both levels together suggest (the measured findings given, and
    /// these).
    pub verdict: Verdict,
}

/// The model that reviews when none is chosen: a light one where the
/// provider has it (it only reads a skill and some notes), else the
/// account's default.
fn review_model(account: &Account) -> Option<String> {
    (account.provider == Provider::Claude).then(|| "haiku".to_owned())
}

impl Hedy {
    /// Level 1 for a skill seen from `project`, beside every approved skill
    /// the project's agents see (its own and the global ones), without the
    /// one named `replacing`.
    pub(super) fn measure_skill(
        &self,
        project: &Path,
        skill: &SkillText,
        replacing: Option<&str>,
    ) -> AppResult<Vec<Finding>> {
        let others: Vec<(String, String)> = self
            .agent_skills(project)?
            .into_iter()
            .filter(|s| Some(s.name.as_str()) != replacing)
            .map(|s| (s.name, s.body))
            .collect();
        Ok(measure(skill, &others))
    }

    /// Level 2: asks the project's account (or `account`, never another one:
    /// rule 3) whether the skill goes against the project's notes, which of
    /// its instructions are vague and which really help. Spends a few
    /// tokens (a light model by default); protected projects' notes only go
    /// to their own account. The request has no tools at all: the skill is
    /// someone else's text. Hidden text is refused before anything is sent.
    /// `measured` are the skill's level-1 findings, for the verdict.
    pub fn review_skill(
        &self,
        project: &Path,
        skill: &SkillText,
        measured: &[Finding],
        account: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<SkillReview> {
        check_visible(skill.body)?;
        let project = self.project_at(project)?;
        let scope = AskScope::Project(project.path.clone());
        let text = format!("{} {}", skill.description, skill.body);
        let rules = pick_rules_for_text(&text, self.ask_sources(&scope)?);
        let (account, _) = self.ask_account(&scope, account)?;
        if self.account_at_limit(&account)? {
            return Err(AppError::AskAtLimit {
                account: account.name.clone(),
            });
        }
        let stage = Stage {
            phase: Phase::Review,
            account: account.id.clone(),
            model: model.map(str::to_owned).or_else(|| review_model(&account)),
        };
        let clock = Instant::now();
        let answer = self.one_shot(OneShot {
            task: None,
            account: &account,
            stage: &stage,
            cwd: self.ask_dir()?,
            prompt: review_prompt(skill, &rules),
            refusal: tr!(
                "No uses herramientas: juzga solo la habilidad que te di.",
                "Do not use tools: judge only the skill you were given."
            ),
            no_shell: true,
            no_tools: true,
            timeout: REVIEW_TIMEOUT,
        })?;
        let raw = match answer {
            Ok(raw) => raw,
            Err(OneShotFailure::AtLimit) => {
                return Err(AppError::AskAtLimit {
                    account: account.name.clone(),
                });
            }
            Err(OneShotFailure::Failed(detail)) => return Err(AppError::SkillReviewFailed(detail)),
            Err(OneShotFailure::TimedOut) => return Err(AppError::SkillReviewTimedOut),
        };
        let parsed = parse_review(&raw, skill, &rules);
        let findings = parsed.clone().unwrap_or_default();
        let verdict = predicted(&[measured, &findings].concat());
        Ok(SkillReview {
            unread: parsed.is_none(),
            findings,
            notes: rules.len(),
            account: Some(account),
            duration_ms: clock.elapsed().as_millis() as u64,
            verdict,
        })
    }

    /// Level 1 for an approved skill seen from `project` (the skills screen
    /// shows it).
    pub fn measure_approved_skill(
        &self,
        project: &Path,
        skill: &Skill,
    ) -> AppResult<(Vec<Finding>, Verdict)> {
        let text = SkillText {
            name: &skill.name,
            description: &skill.description,
            body: &skill.body,
            extra_files: &[],
        };
        let findings = self.measure_skill(project, &text, Some(&skill.name))?;
        let verdict = predicted(&findings);
        Ok((findings, verdict))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::SkillDraft;
    use crate::application::testing::Fixture;
    use crate::domain::{AgentEvent, Category, UsageSample};

    fn answer(f: &Fixture, text: &str) {
        f.agents.reply_on_start(vec![
            AgentEvent::Usage(UsageSample {
                input_tokens: 900,
                output_tokens: 30,
                ..Default::default()
            }),
            AgentEvent::Finished {
                ok: true,
                summary: text.into(),
                duration_ms: None,
            },
        ]);
    }

    fn setup() -> Fixture {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.fs.add_git_root("/code/tienda");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "personal", None)
            .unwrap();
        f
    }

    fn skill() -> SkillText<'static> {
        SkillText {
            name: "react-styles",
            description: "Style React components in this shop.",
            body: "## Styles\nUse CSS-in-JS with styled-components.\n\n## Tests\n1. Render with Testing Library",
            extra_files: &[],
        }
    }

    #[test]
    fn the_ai_judges_the_skill_against_the_projects_notes_only() {
        let f = setup();
        let tienda = f.dir("/code/tienda");
        f.hedy
            .add_note(
                Some(&tienda),
                "Styles go in CSS modules, never CSS-in-JS.",
                Category::Rule,
            )
            .unwrap();
        answer(
            &f,
            "CONTRADICTS [1] \"Use CSS-in-JS with styled-components\"\nHELPFUL \"Render with Testing Library\" => pasos concretos",
        );
        let review = f
            .hedy
            .review_skill(&tienda, &skill(), &[], None, Some("haiku"))
            .unwrap();
        assert!(!review.unread);
        assert_eq!(review.notes, 1);
        assert_eq!(review.account.unwrap().id.as_str(), "personal");
        assert!(matches!(
            review.findings[0],
            Finding::Contradicts { rule: 1, .. }
        ));
        assert_eq!(review.findings.len(), 2);
        let request = f.agents.last_request();
        assert!(request.prompt.contains("CSS modules"));
        assert_eq!(request.model.as_deref(), Some("haiku"));
        // An answer that cannot be read judges nothing.
        answer(&f, "I am not sure.");
        let review = f
            .hedy
            .review_skill(&tienda, &skill(), &[], None, None)
            .unwrap();
        assert!(review.unread && review.findings.is_empty());
    }

    #[test]
    fn a_protected_projects_notes_only_go_to_its_account() {
        let f = setup();
        f.hedy
            .set_setting(
                crate::application::Setting::DefaultAccount,
                Some("personal"),
            )
            .unwrap();
        let work = f.dir("/code/obra");
        f.hedy.use_account(&work, "trabajo", Some(true)).unwrap();
        assert!(matches!(
            f.hedy
                .review_skill(&work, &skill(), &[], Some("personal"), None),
            Err(AppError::Domain(_))
        ));
        answer(&f, "NOTHING");
        let review = f
            .hedy
            .review_skill(&work, &skill(), &[], None, None)
            .unwrap();
        assert_eq!(review.account.unwrap().id.as_str(), "trabajo");
    }

    #[test]
    fn approved_skills_are_measured_beside_the_others() {
        let f = setup();
        let tienda = f.dir("/code/tienda");
        let body = "Run the linter with npm run lint, then fix every warning it reports in \
                    the source folder, format with prettier, check types with tsc, run unit \
                    tests with vitest and commit only when everything passes cleanly";
        let add = |name: &str, body: &str| {
            f.hedy
                .add_skill(
                    &tienda,
                    &SkillDraft {
                        name: name.into(),
                        description: "Before committing a change here.".into(),
                        body: body.into(),
                        global: false,
                    },
                )
                .unwrap()
                .skill
        };
        add("lint", body);
        let twin = add("check", &body.replace("cleanly", "green"));
        let (findings, verdict) = f.hedy.measure_approved_skill(&tienda, &twin).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::Overlaps { skill, .. } if skill == "lint"))
        );
        assert_eq!(
            verdict,
            Verdict::Doubtful,
            "it overlaps and has no concrete steps"
        );
    }
}
