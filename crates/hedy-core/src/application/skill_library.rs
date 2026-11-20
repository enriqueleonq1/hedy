//! The app's skills section: every skill in one place (the global ones and
//! every project's), each with what Hedy measures and its last trial; and
//! "Usar también fuera de Hedy", which writes a project skill where the
//! user's own CLIs read it (`.claude/skills` and `.agents/skills` of the
//! project, excluded from git). Off by default: skills live inside Hedy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::skills::{SkillFiles, update_target};
use super::{AppError, AppResult, Hedy};
use crate::domain::context::estimate_tokens;
use crate::domain::skill::{SkillFolder, is_managed, skill_file};
use crate::domain::skill_check::{Finding, SkillText, Verdict, measure, predicted};
use crate::domain::skill_trial::TrialResult;
use crate::domain::{Skill, SkillStatus};

/// Settings key of the skills used outside Hedy too, on this PC.
const OUTSIDE_KEY: &str = "skill.outside";

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

    /// The skills used outside Hedy too, on this PC: by id, each with what
    /// it was when turned on (an id may be reused, a skill renamed on
    /// another PC: only the same skill, by its creation, keeps it).
    fn outside_entries(&self) -> BTreeMap<i64, OutsideEntry> {
        self.store
            .get_setting(OUTSIDE_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    fn save_outside_entries(&self, entries: &BTreeMap<i64, OutsideEntry>) -> AppResult<()> {
        let json = match entries.is_empty() {
            true => None,
            false => serde_json::to_string(entries).ok(),
        };
        Ok(self.store.set_setting(OUTSIDE_KEY, json.as_deref())?)
    }

    /// Whether a skill is used outside Hedy too, on this PC.
    pub fn skill_outside(&self, id: i64) -> bool {
        let Some(entry) = self.outside_entries().remove(&id) else {
            return false;
        };
        matches!(self.store.skill(id), Ok(Some(skill)) if entry.is(&skill))
    }

    /// Turns "Usar también fuera de Hedy" on or off for an approved project
    /// skill: on, it is written in the project's `.claude/skills` and
    /// `.agents/skills` (only where git ignores it; a file of the user's own
    /// or one git tracks is never replaced); off, Hedy's copies there go.
    pub fn set_skill_outside(&self, id: i64, on: bool) -> AppResult<SkillFiles> {
        let skill = self.skill_or_error(id)?;
        let Some(project) = skill.project.clone() else {
            return Err(AppError::SkillOutsideProjectOnly);
        };
        if skill.status != SkillStatus::Approved {
            return Err(AppError::SkillOutsideProjectOnly);
        }
        let mut files = SkillFiles::default();
        let mut entries = self.outside_entries();
        if on {
            entries.insert(id, OutsideEntry::of(&skill));
            self.save_outside_entries(&entries)?;
            self.write_outside(&skill, &mut files)?;
            // Nothing could be written (files of the user's own there): it
            // stays inside Hedy rather than look on.
            if files.written.is_empty() {
                entries.remove(&id);
                self.save_outside_entries(&entries)?;
            }
        } else {
            entries.remove(&id);
            self.save_outside_entries(&entries)?;
            self.remove_outside(&project, &skill.name, &mut files);
        }
        Ok(files)
    }

    /// Whether Hedy may own `file` in the project's checkout: outside git, or
    /// ignored and untracked there (a copy someone committed, or a project
    /// inside a bigger checkout whose exclude does not reach it, stays).
    fn ours_to_touch(&self, project: &Path, file: &Path) -> bool {
        self.fs.git_root(project).is_none() || self.git.is_ignored(project, &file.to_string_lossy())
    }

    /// Writes a project skill where the user's own CLIs read it (never
    /// making a project folder that is gone).
    pub(super) fn write_outside(&self, skill: &Skill, files: &mut SkillFiles) -> AppResult<()> {
        let Some(project) = &skill.project else {
            return Ok(());
        };
        if !self.fs.exists(project) {
            return Ok(());
        }
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
            if !self.ours_to_touch(project, &file) {
                files.skipped.push(path);
                continue;
            }
            self.fs.write_text(&path, &skill_file(skill))?;
            files.written.push(path);
        }
        Ok(())
    }

    /// Removes Hedy's copies of a skill from the project's own checkout (a
    /// file of the user's own, one git tracks, or one behind a link, stays).
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
            if !self.through_link(project, &file)
                && ours
                && self.ours_to_touch(project, &file)
                && self.fs.remove_file(&path).is_ok()
            {
                files.removed.push(path);
                let _ = self.remove_empty_skill_dirs(project, folder, name);
            }
        }
    }

    /// After a skill changed here (edited, updated, moved or removed): its
    /// copies outside Hedy follow it. `old` is the skill before the change.
    pub(super) fn follow_outside(&self, old: &Skill, new: Option<&Skill>, files: &mut SkillFiles) {
        let mut entries = self.outside_entries();
        let Some(entry) = entries.get(&old.id).cloned() else {
            return;
        };
        if !entry.is(old) {
            return;
        }
        let moved = new.is_none_or(|n| n.name != old.name || n.project != old.project);
        if moved && let Some(project) = &old.project {
            self.remove_outside(project, &old.name, files);
        }
        match new.filter(|n| n.project.is_some() && n.status == SkillStatus::Approved) {
            Some(new) => {
                entries.insert(old.id, OutsideEntry::of(new));
                let _ = self.write_outside(new, files);
            }
            None => {
                entries.remove(&old.id);
            }
        }
        let _ = self.save_outside_entries(&entries);
    }

    /// Copies outside Hedy brought up to date, after a sync and at start: a
    /// skill changed on another PC is rewritten, renamed or moved ones leave
    /// their old copies, deleted ones (or an id taken by another skill) take
    /// theirs back. A project folder that is gone is never made again.
    /// Best effort.
    pub(super) fn refresh_outside(&self) {
        let entries = self.outside_entries();
        if entries.is_empty() {
            return;
        }
        let mut kept = BTreeMap::new();
        for (id, entry) in entries {
            let now = self.store.skill(id).ok().flatten();
            let same = now
                .as_ref()
                .is_some_and(|s| s.created_at == entry.created_at);
            let changed = now.as_ref().is_none_or(|s| {
                !same || s.name != entry.name || s.project.as_ref() != Some(&entry.project)
            });
            if changed && self.fs.exists(&entry.project) {
                self.remove_outside(&entry.project, &entry.name, &mut SkillFiles::default());
            }
            let Some(skill) = now.filter(|s| same && s.status == SkillStatus::Approved) else {
                continue;
            };
            let Some(project) = skill.project.clone() else {
                continue;
            };
            if self.fs.exists(&project) {
                let _ = self.write_outside(&skill, &mut SkillFiles::default());
            }
            kept.insert(id, OutsideEntry::of(&skill));
        }
        let _ = self.save_outside_entries(&kept);
    }

    /// A skill row is deleted (by sync): its copies outside Hedy go and the
    /// switch is forgotten now, before its id can be taken by another skill.
    pub(super) fn forget_outside_of(&self, id: i64) {
        let mut entries = self.outside_entries();
        let Some(entry) = entries.remove(&id) else {
            return;
        };
        if self.fs.exists(&entry.project) {
            self.remove_outside(&entry.project, &entry.name, &mut SkillFiles::default());
        }
        let _ = self.save_outside_entries(&entries);
    }

    /// A project leaves Hedy: its skills' copies outside Hedy go with it.
    pub(super) fn forget_outside_in(&self, project: &Path) {
        let mut entries = self.outside_entries();
        let before = entries.len();
        entries.retain(|_, entry| {
            let here = entry.project == project;
            if here && self.fs.exists(project) {
                self.remove_outside(project, &entry.name, &mut SkillFiles::default());
            }
            !here
        });
        if entries.len() != before {
            let _ = self.save_outside_entries(&entries);
        }
    }
}

/// What a skill used outside Hedy was when turned on (or last followed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutsideEntry {
    name: String,
    project: PathBuf,
    created_at: i64,
}

impl OutsideEntry {
    fn of(skill: &Skill) -> Self {
        Self {
            name: skill.name.clone(),
            project: skill.project.clone().unwrap_or_default(),
            created_at: skill.created_at,
        }
    }

    /// The same skill, as it is now (an id reused by another skill is not).
    fn is(&self, skill: &Skill) -> bool {
        skill.created_at == self.created_at
            && skill.name == self.name
            && skill.project.as_ref() == Some(&self.project)
            && skill.status == SkillStatus::Approved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::SkillDraft;
    use crate::application::ports::{FileSystem, SkillRepository};
    use crate::application::testing::Fixture;

    fn setup() -> (Fixture, Skill) {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_git_root("/code/agenda");
        f.fs.add_existing("/code/agenda");
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
        // Excluded by Hedy, so git ignores them (as `git check-ignore` says).
        for name in ["pr-checklist", "pr-list"] {
            for folder in [".claude", ".agents"] {
                f.git.ignore(&format!("{folder}/skills/{name}/SKILL.md"));
            }
        }

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
    fn a_copy_git_tracks_is_never_written_or_removed() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        // Someone committed an older Hedy's copy: git does not ignore it.
        let tracked = agenda.join(".claude/skills/pr-checklist/SKILL.md");
        let committed = skill_file(&skill).replace("CHANGELOG", "the changelog");
        f.fs.write_text(&tracked, &committed).unwrap();
        f.git.ignore(".agents/skills/pr-checklist/SKILL.md");
        let files = f.hedy.set_skill_outside(skill.id, true).unwrap();
        assert_eq!(files.skipped, std::slice::from_ref(&tracked));
        assert_eq!(f.fs.read_text(&tracked).unwrap().unwrap(), committed);
        f.hedy.set_skill_outside(skill.id, false).unwrap();
        assert!(f.fs.exists(&tracked), "a committed copy stays");
    }

    #[test]
    fn the_switch_stays_off_when_nothing_could_be_written() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        for folder in [".claude", ".agents"] {
            let mine = agenda.join(format!("{folder}/skills/pr-checklist/SKILL.md"));
            f.fs.write_text(&mine, "---\nname: pr-checklist\n---\nMine")
                .unwrap();
        }
        let files = f.hedy.set_skill_outside(skill.id, true).unwrap();
        assert!(files.written.is_empty() && files.skipped.len() == 2);
        assert!(!f.hedy.skill_outside(skill.id));
    }

    #[test]
    fn an_id_taken_by_another_skill_never_inherits_the_switch() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        f.git.ignore(".claude/skills/pr-checklist/SKILL.md");
        f.git.ignore(".agents/skills/pr-checklist/SKILL.md");
        f.hedy.set_skill_outside(skill.id, true).unwrap();
        // Deleted on another PC (sync drops the row), then the id is reused.
        f.store.delete_skill(skill.id).unwrap();
        f.clock.advance(60);
        let other = f
            .hedy
            .add_skill(
                &agenda,
                &SkillDraft {
                    name: "deploy".into(),
                    description: "Before deploying.".into(),
                    body: "1. Run `make deploy`".into(),
                    global: false,
                },
            )
            .unwrap()
            .skill;
        assert!(!f.hedy.skill_outside(other.id));
        // The next refresh takes the old copies back and forgets the switch.
        f.hedy.refresh_skills_plugins();
        assert!(
            !f.fs
                .exists(&agenda.join(".claude/skills/pr-checklist/SKILL.md"))
        );
        assert!(!f.fs.exists(&agenda.join(".claude/skills/deploy/SKILL.md")));
    }

    #[test]
    fn a_skill_deleted_by_sync_takes_its_copies_back_at_once() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        f.git.ignore(".claude/skills/pr-checklist/SKILL.md");
        f.git.ignore(".agents/skills/pr-checklist/SKILL.md");
        f.hedy.set_skill_outside(skill.id, true).unwrap();
        // What sync does when another PC deleted it: before the id is reused.
        f.hedy.forget_outside_of(skill.id);
        f.store.delete_skill(skill.id).unwrap();
        assert!(!f.fs.exists(&agenda.join(".claude/skills/pr-checklist/SKILL.md")));
        let other = f
            .hedy
            .add_skill(
                &agenda,
                &SkillDraft {
                    name: "deploy".into(),
                    description: "Before deploying.".into(),
                    body: "1. Run `make deploy`".into(),
                    global: false,
                },
            )
            .unwrap()
            .skill;
        assert!(!f.hedy.skill_outside(other.id), "even created in the same second");
    }

    #[test]
    fn removing_a_project_takes_its_copies_back() {
        let (f, skill) = setup();
        let agenda = f.dir("/code/agenda");
        f.git.ignore(".claude/skills/pr-checklist/SKILL.md");
        f.git.ignore(".agents/skills/pr-checklist/SKILL.md");
        f.hedy.set_skill_outside(skill.id, true).unwrap();
        f.hedy.remove_project(&agenda).unwrap();
        assert!(
            !f.fs
                .exists(&agenda.join(".claude/skills/pr-checklist/SKILL.md"))
        );
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
        f.git.ignore(".claude/skills/pr-checklist/SKILL.md");
        f.git.ignore(".agents/skills/pr-checklist/SKILL.md");
        f.hedy.set_skill_outside(skill.id, true).unwrap();
        f.hedy.remove_skill(skill.id).unwrap();
        assert!(
            !f.fs
                .exists(&agenda.join(".claude/skills/pr-checklist/SKILL.md"))
        );
        assert!(!f.hedy.skill_outside(skill.id));
    }
}
