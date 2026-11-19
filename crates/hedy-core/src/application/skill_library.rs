//! The app's skills section: every skill in one place (the global ones and
//! every project's), each with what Hedy measures and its last trial; and
//! "Usar también fuera de Hedy", which writes a project skill where the
//! user's own CLIs read it (`.claude/skills` and `.agents/skills` of the
//! project, excluded from git). Off by default: skills live inside Hedy.

use std::path::Path;

use serde::Serialize;

use super::skills::{SkillFiles, update_target};
use super::{AppError, AppResult, Hedy};
use crate::domain::context::estimate_tokens;
use crate::domain::skill::{SkillFolder, is_managed, skill_file};
use crate::domain::skill_check::{Finding, SkillText, Verdict, measure, predicted};
use crate::domain::skill_trial::TrialResult;
use crate::domain::{Skill, SkillStatus};

/// Settings key of a skill used outside Hedy too (by skill id, this PC).
const OUTSIDE_KEY: &str = "skill.outside.";

/// One skill of the section.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOverview {
    pub skill: Skill,
    /// Its project's name; `None` for a global skill.
    pub project_name: Option<String>,
    /// For a proposal: the approved skill it would update.
    pub replaces: Option<Skill>,
    /// Written where the user's own CLIs read it too.
    pub outside: bool,
    /// About how much context it takes when loaded.
    pub tokens: usize,
    /// What Hedy measures on its own (level 1), beside the skills of its
    /// scope.
    pub findings: Vec<Finding>,
    pub verdict: Verdict,
    /// Its last trial, while it is the version tried.
    pub trial: Option<TrialResult>,
}

impl Hedy {
    /// Every skill: the global ones first, then each project's (proposals
    /// included), each measured beside the approved skills its agents see.
    pub fn skills_overview(&self) -> AppResult<Vec<SkillOverview>> {
        let globals = self.store.skills(None, false)?;
        let approved_globals: Vec<Skill> = globals
            .iter()
            .filter(|s| s.status == SkillStatus::Approved)
            .cloned()
            .collect();
        let mut out: Vec<SkillOverview> = globals
            .iter()
            .map(|s| self.overview_of(s, None, &approved_globals))
            .collect();
        let mut projects = self.store.list_projects()?;
        projects.sort_by_key(|p| p.name.to_lowercase());
        for project in projects {
            let own = self.store.skills(Some(&project.path), false)?;
            let mut seen: Vec<Skill> = own
                .iter()
                .filter(|s| s.status == SkillStatus::Approved)
                .cloned()
                .collect();
            if !project.protected {
                seen.extend(approved_globals.iter().cloned());
            }
            out.extend(
                own.iter()
                    .map(|s| self.overview_of(s, Some(project.name.clone()), &seen)),
            );
        }
        Ok(out)
    }

    fn overview_of(
        &self,
        skill: &Skill,
        project_name: Option<String>,
        seen: &[Skill],
    ) -> SkillOverview {
        let replaces = match skill.status {
            SkillStatus::Proposed => update_target(seen, skill),
            SkillStatus::Approved => None,
        };
        let others: Vec<(String, String)> = seen
            .iter()
            .filter(|s| s.id != skill.id && Some(s.id) != replaces.as_ref().map(|r| r.id))
            .map(|s| (s.name.clone(), s.body.clone()))
            .collect();
        let text = SkillText {
            name: &skill.name,
            description: &skill.description,
            body: &skill.body,
            extra_files: &[],
        };
        let findings = measure(&text, &others);
        SkillOverview {
            verdict: predicted(&findings),
            findings,
            tokens: estimate_tokens(&skill.description) + estimate_tokens(&skill.body),
            trial: self.skill_trial(skill.id).ok().flatten(),
            outside: self.skill_outside(skill.id),
            replaces,
            project_name,
            skill: skill.clone(),
        }
    }

    /// Whether a skill is used outside Hedy too, on this PC.
    pub fn skill_outside(&self, id: i64) -> bool {
        matches!(
            self.store.get_setting(&format!("{OUTSIDE_KEY}{id}")),
            Ok(Some(_))
        )
    }

    /// Turns "Usar también fuera de Hedy" on or off for an approved project
    /// skill: on, it is written in the project's `.claude/skills` and
    /// `.agents/skills` (excluded from git; a file of the user's own is never
    /// replaced); off, Hedy's copies there go.
    pub fn set_skill_outside(&self, id: i64, on: bool) -> AppResult<SkillFiles> {
        let skill = self.skill_or_error(id)?;
        let Some(project) = skill.project.clone() else {
            return Err(AppError::SkillOutsideProjectOnly);
        };
        if skill.status != SkillStatus::Approved {
            return Err(AppError::SkillOutsideProjectOnly);
        }
        let mut files = SkillFiles::default();
        let key = format!("{OUTSIDE_KEY}{id}");
        if on {
            self.store.set_setting(&key, Some("1"))?;
            self.write_outside(&skill, &mut files)?;
        } else {
            self.store.set_setting(&key, None)?;
            self.remove_outside(&project, &skill.name, &mut files);
        }
        Ok(files)
    }

    /// Writes a project skill where the user's own CLIs read it.
    pub(super) fn write_outside(&self, skill: &Skill, files: &mut SkillFiles) -> AppResult<()> {
        let Some(project) = &skill.project else {
            return Ok(());
        };
        let in_git = self.fs.git_root(project).is_some();
        for folder in SkillFolder::ALL {
            let file = folder.file(&skill.name);
            let path = project.join(&file);
            let theirs = self
                .fs
                .read_text(&path)?
                .is_some_and(|text| !is_managed(&text));
            if self.through_link(project, &file) || theirs {
                files.skipped.push(path);
                continue;
            }
            if in_git {
                self.git
                    .ensure_excluded(project, &folder.exclude_pattern(&skill.name))?;
            }
            self.fs.write_text(&path, &skill_file(skill))?;
            files.written.push(path);
        }
        Ok(())
    }

    /// Removes Hedy's copies of a skill from the project's own checkout (a
    /// file of the user's own, or one behind a link, stays).
    pub(super) fn remove_outside(&self, project: &Path, name: &str, files: &mut SkillFiles) {
        for folder in SkillFolder::ALL {
            let file = folder.file(name);
            let path = project.join(&file);
            let ours = self
                .fs
                .read_text(&path)
                .ok()
                .flatten()
                .is_some_and(|text| is_managed(&text));
            if !self.through_link(project, &file) && ours && self.fs.remove_file(&path).is_ok() {
                files.removed.push(path);
                let _ = self.remove_empty_skill_dirs(project, folder, name);
            }
        }
    }

    /// After a skill changed (edited, updated, moved or removed): its copies
    /// outside Hedy follow it. `old` is the skill before the change.
    pub(super) fn follow_outside(&self, old: &Skill, new: Option<&Skill>, files: &mut SkillFiles) {
        if !self.skill_outside(old.id) {
            return;
        }
        let moved = new.is_none_or(|n| n.name != old.name || n.project != old.project);
        if moved && let Some(project) = &old.project {
            self.remove_outside(project, &old.name, files);
        }
        match new.filter(|n| n.project.is_some() && n.status == SkillStatus::Approved) {
            Some(new) => {
                let _ = self.write_outside(new, files);
            }
            None => {
                let _ = self
                    .store
                    .set_setting(&format!("{OUTSIDE_KEY}{}", old.id), None);
            }
        }
    }

    /// Copies outside Hedy brought up to date (a skill changed on another
    /// PC). Best effort.
    pub(super) fn refresh_outside(&self) {
        let Ok(projects) = self.store.list_projects() else {
            return;
        };
        for project in projects {
            let Ok(skills) = self.store.skills(Some(&project.path), false) else {
                continue;
            };
            for skill in skills {
                if skill.status == SkillStatus::Approved && self.skill_outside(skill.id) {
                    let _ = self.write_outside(&skill, &mut SkillFiles::default());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::SkillDraft;
    use crate::application::ports::FileSystem;
    use crate::application::testing::Fixture;

    fn setup() -> (Fixture, Skill) {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        let skill = f
            .hedy
            .add_skill(
                &f.dir("/code/agenda"),
                &SkillDraft {
                    name: "pr-checklist".into(),
                    description: "Before opening a pull request.".into(),
                    body: "1. Update CHANGELOG.md\n2. Run `npm test`".into(),
                    global: false,
                },
            )
            .unwrap()
            .skill;
        (f, skill)
    }

    #[test]
    fn the_overview_has_every_skill_with_its_measure() {
        let (f, skill) = setup();
        f.hedy
            .add_skill(
                &f.dir("/code/agenda"),
                &SkillDraft {
                    name: "commit-style".into(),
                    description: "When writing a commit message.".into(),
                    body: "1. One line: `type(scope): message`".into(),
                    global: true,
                },
            )
            .unwrap();
        let all = f.hedy.skills_overview().unwrap();
        let names: Vec<_> = all.iter().map(|o| o.skill.name.as_str()).collect();
        assert_eq!(names, ["commit-style", "pr-checklist"], "globals first");
        assert_eq!(all[0].project_name, None);
        let ours = &all[1];
        assert_eq!(ours.skill.id, skill.id);
        assert_eq!(ours.project_name.as_deref(), Some("agenda"));
        assert!(ours.tokens > 0 && !ours.outside && ours.trial.is_none());
        assert!(ours.findings.contains(&Finding::ConcreteSteps));
    }

    #[test]
    fn a_project_skill_can_be_used_outside_hedy_and_follows_its_changes() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        let claude = agenda.join(".claude/skills/pr-checklist/SKILL.md");
        let agents = agenda.join(".agents/skills/pr-checklist/SKILL.md");
        assert!(!f.fs.exists(&claude), "inside Hedy only by default");

        let files = f.hedy.set_skill_outside(skill.id, true).unwrap();
        assert_eq!(files.written, [claude.clone(), agents.clone()]);
        assert!(f.hedy.skill_outside(skill.id));
        assert!(
            f.git
                .excluded()
                .contains(&"/.claude/skills/pr-checklist/".to_owned())
        );
        // Hedy's own cleanup of old copies leaves it alone.
        f.hedy.refresh_skills_plugins();
        assert!(f.fs.exists(&claude));

        // A rename moves it; the old name's copies go.
        let draft = SkillDraft {
            name: "pr-list".into(),
            description: skill.description.clone(),
            body: "1. Run `npm test`".into(),
            global: false,
        };
        f.hedy.edit_skill(skill.id, &agenda, &draft).unwrap();
        assert!(!f.fs.exists(&claude));
        let renamed = agenda.join(".claude/skills/pr-list/SKILL.md");
        assert!(
            f.fs.read_text(&renamed)
                .unwrap()
                .unwrap()
                .contains("Run `npm test`")
        );

        // Off: Hedy's copies go, a file of the user's own stays.
        let mine = agenda.join(".agents/skills/pr-list/SKILL.md");
        f.fs.write_text(&mine, "---\nname: pr-list\n---\nMine")
            .unwrap();
        f.hedy.set_skill_outside(skill.id, false).unwrap();
        assert!(!f.fs.exists(&renamed) && f.fs.exists(&mine));
        assert!(!f.hedy.skill_outside(skill.id));
    }

    #[test]
    fn only_approved_project_skills_go_outside_and_removing_takes_them_back() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        let global = f
            .hedy
            .add_skill(
                &agenda,
                &SkillDraft {
                    name: "commit-style".into(),
                    description: "When writing a commit message.".into(),
                    body: "1. One line".into(),
                    global: true,
                },
            )
            .unwrap()
            .skill;
        assert!(matches!(
            f.hedy.set_skill_outside(global.id, true),
            Err(AppError::SkillOutsideProjectOnly)
        ));
        f.hedy.set_skill_outside(skill.id, true).unwrap();
        f.hedy.remove_skill(skill.id).unwrap();
        assert!(
            !f.fs
                .exists(&agenda.join(".claude/skills/pr-checklist/SKILL.md"))
        );
        assert!(!f.hedy.skill_outside(skill.id));
    }
}
