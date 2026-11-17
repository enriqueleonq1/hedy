//! Learned skills: reusable procedures agents propose at the end of a task
//! and the user approves (spec rule 6, as with memory notes). Approved
//! project skills are written in each CLI's own skill folder, in the project
//! and in its task worktrees; every skill is also served by the memory MCP
//! server (`skill_list` / `skill_get`), which is the only way global skills
//! reach the agents: Hedy never writes into an account's config folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::ports::{NewSkill, SkillRevision, SkillTools};
use super::{AppError, AppResult, Hedy};
use crate::domain::memory::{USER_AUTHOR, preview};
use crate::domain::recall::{PageRequest, Paged, SKILLS_MAX, SKILLS_PAGE, paginate};
use crate::domain::skill::{
    SkillFolder, find_secret, is_managed, normalize_body, normalize_description,
    normalize_skill_name, skill_file,
};
use crate::domain::{DomainError, Project, Skill, SkillStatus, TaskId};
use crate::experiments::{self, Experiment};
use crate::tr;

/// Characters of each description an agent sees in `skill_list`.
const DESCRIPTION_PREVIEW: usize = 200;

/// A skill an AI proposed, with what the user needs to decide on it.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillProposal {
    pub skill: Skill,
    /// The approved skill with the same name in the same scope: approving
    /// the proposal replaces its text (it keeps its id); rejecting it
    /// changes nothing.
    pub replaces: Option<Skill>,
}

/// Everything the skills screen shows for a project.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillsView {
    /// The project's approved skills and, unless it is protected, global ones.
    pub approved: Vec<Skill>,
    pub proposed: Vec<SkillProposal>,
    /// A protected project neither reads nor shares global skills.
    pub protected: bool,
}

/// A skill as the user writes or edits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDraft {
    pub name: String,
    pub description: String,
    pub body: String,
    /// For every project (served only through the MCP tools).
    pub global: bool,
}

/// What happened to the CLIs' skill files after a change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillFiles {
    pub written: Vec<PathBuf>,
    pub removed: Vec<PathBuf>,
    /// Left alone: a skill file with that name that Hedy did not write
    /// (the user's own). Hedy never overwrites or deletes those.
    pub skipped: Vec<PathBuf>,
}

impl SkillFiles {
    fn extend(&mut self, other: SkillFiles) {
        self.written.extend(other.written);
        self.removed.extend(other.removed);
        self.skipped.extend(other.skipped);
    }
}

/// The skill after a change, and what happened to its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillChange {
    pub skill: Skill,
    pub files: SkillFiles,
}

/// The approved skill a proposal would update: same name, same scope.
fn update_target(skills: &[Skill], proposal: &Skill) -> Option<Skill> {
    skills
        .iter()
        .find(|s| {
            s.id != proposal.id
                && s.status == SkillStatus::Approved
                && s.project == proposal.project
                && s.name == proposal.name
        })
        .cloned()
}

/// Name, description and body checked: slug, limits and no secrets.
fn validated(name: &str, description: &str, body: &str) -> AppResult<(String, String, String)> {
    let name = normalize_skill_name(name)?;
    let description = normalize_description(description)?;
    let body = normalize_body(body)?;
    for text in [&description, &body] {
        if let Some(kind) = find_secret(text) {
            return Err(DomainError::SkillLooksSecret(kind).into());
        }
    }
    Ok((name, description, body))
}

impl Hedy {
    fn skill_or_error(&self, id: i64) -> AppResult<Skill> {
        self.store
            .skill(id)?
            .ok_or_else(|| AppError::SkillNotFound(id.to_string()))
    }

    /// Skills in the same scope as `skill` (its project, or global).
    fn skills_beside(&self, skill: &Skill) -> AppResult<Vec<Skill>> {
        Ok(self.store.skills(skill.project.as_deref(), false)?)
    }

    /// Whether `skill` belongs to what `project` sees.
    fn in_skill_scope(project: &Project, skill: &Skill) -> bool {
        match &skill.project {
            Some(path) => *path == project.path,
            None => !project.protected,
        }
    }

    pub fn skills(&self, path: &Path) -> AppResult<SkillsView> {
        let project = self.project_at(path)?;
        let all = self.store.skills(Some(&project.path), !project.protected)?;
        let (approved, proposed): (Vec<_>, Vec<_>) = all
            .into_iter()
            .partition(|s| s.status == SkillStatus::Approved);
        let proposed = proposed
            .into_iter()
            .map(|skill| SkillProposal {
                replaces: update_target(&approved, &skill),
                skill,
            })
            .collect();
        Ok(SkillsView {
            approved,
            proposed,
            protected: project.protected,
        })
    }

    /// A skill by name for the user (`hedy skills show`): the approved one
    /// (the project's before a global one), else a proposal with that name.
    pub fn skill_named(&self, path: &Path, name: &str) -> AppResult<Skill> {
        let wanted = normalize_skill_name(name)
            .map_err(|_| AppError::SkillNotFound(name.trim().to_owned()))?;
        let view = self.skills(path)?;
        view.approved
            .into_iter()
            .filter(|s| s.name == wanted)
            .min_by_key(|s| s.project.is_none())
            .or_else(|| {
                view.proposed
                    .into_iter()
                    .map(|p| p.skill)
                    .find(|s| s.name == wanted)
            })
            .ok_or(AppError::SkillNotFound(wanted))
    }

    /// A skill suggested by an AI: it waits for the user. With the name of
    /// an approved skill of the project, it is a proposed update of it.
    pub fn propose_skill(
        &self,
        project: &Path,
        name: &str,
        description: &str,
        body: &str,
        author: &str,
        task: Option<&TaskId>,
    ) -> AppResult<Skill> {
        let project = self.project_at(project)?;
        let (name, description, body) = validated(name, description, body)?;
        Ok(self.store.add_skill(&NewSkill {
            project: Some(project.path),
            name,
            description,
            body,
            author: author.to_owned(),
            status: SkillStatus::Proposed,
            created_at: self.now_secs(),
            source_task: task.cloned(),
        })?)
    }

    /// A skill written by the user: approved right away.
    pub fn add_skill(&self, project: &Path, draft: &SkillDraft) -> AppResult<SkillChange> {
        let project = self.project_at(project)?;
        let (name, description, body) = validated(&draft.name, &draft.description, &draft.body)?;
        let scope = self.skill_scope(&project, draft.global)?;
        let taken = self
            .store
            .skills(scope.as_deref(), false)?
            .into_iter()
            .any(|s| s.status == SkillStatus::Approved && s.name == name);
        if taken {
            return Err(AppError::SkillNameTaken(name));
        }
        let skill = self.store.add_skill(&NewSkill {
            project: scope,
            name,
            description,
            body,
            author: USER_AUTHOR.to_owned(),
            status: SkillStatus::Approved,
            created_at: self.now_secs(),
            source_task: None,
        })?;
        let files = self.install_skill(&skill)?;
        Ok(SkillChange { skill, files })
    }

    fn skill_scope(&self, project: &Project, global: bool) -> AppResult<Option<PathBuf>> {
        match (global, project.protected) {
            (true, true) => Err(AppError::GlobalSkillInProtectedProject),
            (true, false) => Ok(None),
            (false, _) => Ok(Some(project.path.clone())),
        }
    }

    /// Approves a proposal and makes it available: an update replaces the
    /// approved skill with its name (same id), and the CLIs' files are
    /// written or rewritten.
    pub fn approve_skill(&self, id: i64) -> AppResult<SkillChange> {
        let proposal = self.skill_or_error(id)?;
        if proposal.status == SkillStatus::Approved {
            return Ok(SkillChange {
                skill: proposal,
                files: SkillFiles::default(),
            });
        }
        let approved = match update_target(&self.skills_beside(&proposal)?, &proposal) {
            Some(target) => {
                self.store.revise_skill(
                    target.id,
                    &SkillRevision {
                        project: target.project.clone(),
                        name: target.name.clone(),
                        description: proposal.description.clone(),
                        body: proposal.body.clone(),
                        author: proposal.author.clone(),
                        updated_at: self.now_secs(),
                    },
                )?;
                self.store.delete_skill(proposal.id)?;
                self.skill_or_error(target.id)?
            }
            None => {
                self.store.set_skill_status(id, SkillStatus::Approved)?;
                self.skill_or_error(id)?
            }
        };
        let files = self.install_skill(&approved)?;
        Ok(SkillChange {
            skill: approved,
            files,
        })
    }

    /// Rejects a proposal: it is deleted and nothing else changes.
    pub fn reject_skill(&self, id: i64) -> AppResult<()> {
        if self.skill_or_error(id)?.status != SkillStatus::Proposed {
            return Err(AppError::SkillNotAProposal(id));
        }
        Ok(self.store.delete_skill(id)?)
    }

    /// The user edits a skill seen from `project` (a proposal before
    /// approving it, or an approved one). An approved skill's files follow:
    /// renamed, moved to global (MCP only) or rewritten.
    pub fn edit_skill(
        &self,
        id: i64,
        project: &Path,
        draft: &SkillDraft,
    ) -> AppResult<SkillChange> {
        let project = self.project_at(project)?;
        let old = self.skill_or_error(id)?;
        if !Self::in_skill_scope(&project, &old) {
            return Err(AppError::SkillNotFound(id.to_string()));
        }
        let (name, description, body) = validated(&draft.name, &draft.description, &draft.body)?;
        let scope = self.skill_scope(&project, draft.global)?;
        if old.status == SkillStatus::Approved {
            let taken = self
                .store
                .skills(scope.as_deref(), false)?
                .into_iter()
                .any(|s| s.id != id && s.status == SkillStatus::Approved && s.name == name);
            if taken {
                return Err(AppError::SkillNameTaken(name));
            }
        }
        self.store.revise_skill(
            id,
            &SkillRevision {
                project: scope,
                name,
                description,
                body,
                author: old.author.clone(),
                updated_at: self.now_secs(),
            },
        )?;
        let skill = self.skill_or_error(id)?;
        let mut files = SkillFiles::default();
        if skill.status == SkillStatus::Approved {
            if old.name != skill.name || old.project != skill.project {
                files.extend(self.uninstall_skill(&old)?);
            }
            files.extend(self.install_skill(&skill)?);
        }
        Ok(SkillChange { skill, files })
    }

    /// Deletes a skill (approved or proposed). An approved project skill's
    /// files are removed too, but only those Hedy wrote.
    pub fn remove_skill(&self, id: i64) -> AppResult<SkillFiles> {
        let skill = self.skill_or_error(id)?;
        self.store.delete_skill(id)?;
        if skill.status == SkillStatus::Approved {
            return self.uninstall_skill(&skill);
        }
        Ok(SkillFiles::default())
    }

    /// Folders where a project's CLIs work: the project itself (`false`)
    /// and the worktrees of its open tasks (`true`).
    fn skill_dirs(&self, project: &Path) -> AppResult<Vec<(PathBuf, bool)>> {
        let mut dirs = vec![(project.to_owned(), false)];
        for task in self.store.tasks(project)? {
            if let Some(ws) = &task.workspace
                && !task.status.is_closed()
                && self.fs.exists(&ws.worktree)
            {
                dirs.push((ws.worktree.clone(), true));
            }
        }
        Ok(dirs)
    }

    fn install_skill(&self, skill: &Skill) -> AppResult<SkillFiles> {
        let mut files = SkillFiles::default();
        let Some(project) = &skill.project else {
            return Ok(files);
        };
        for (dir, worktree) in self.skill_dirs(project)? {
            self.write_skill_files(project, &dir, worktree, skill, &mut files)?;
        }
        Ok(files)
    }

    /// Writes an approved skill in each CLI's folder inside `dir`, after
    /// excluding it from git. A file Hedy did not write is never replaced.
    ///
    /// Claude Code reads the main checkout's `.claude/skills` from a
    /// worktree that has none, so a worktree only gets Claude's copy when it
    /// already has that folder (committed by the team); otherwise the
    /// user's own uncommitted skills would stop loading there.
    fn write_skill_files(
        &self,
        repo: &Path,
        dir: &Path,
        worktree: bool,
        skill: &Skill,
        files: &mut SkillFiles,
    ) -> AppResult<()> {
        for folder in SkillFolder::ALL {
            if worktree
                && folder == SkillFolder::Claude
                && !self.fs.exists(&dir.join(folder.path()))
            {
                continue;
            }
            let path = dir.join(folder.file(&skill.name));
            if self
                .fs
                .read_text(&path)?
                .is_some_and(|text| !is_managed(&text))
            {
                files.skipped.push(path);
                continue;
            }
            if self.fs.git_root(repo).is_some() {
                self.git
                    .ensure_excluded(repo, &folder.exclude_pattern(&skill.name))?;
            }
            self.fs.write_text(&path, &skill_file(skill))?;
            files.written.push(path);
        }
        Ok(())
    }

    fn uninstall_skill(&self, skill: &Skill) -> AppResult<SkillFiles> {
        let mut files = SkillFiles::default();
        let Some(project) = &skill.project else {
            return Ok(files);
        };
        for (dir, _) in self.skill_dirs(project)? {
            for folder in SkillFolder::ALL {
                let path = dir.join(folder.file(&skill.name));
                if !self
                    .fs
                    .read_text(&path)?
                    .is_some_and(|text| is_managed(&text))
                {
                    continue;
                }
                self.fs.remove_file(&path)?;
                files.removed.push(path);
                // Empty folders left behind go too: `<name>`, `skills`, `.claude`.
                let mut empty = dir.join(folder.path()).join(&skill.name);
                loop {
                    self.fs.remove_empty_dir(&empty)?;
                    if !empty.pop() || empty == dir {
                        break;
                    }
                }
            }
        }
        Ok(files)
    }

    /// A new task worktree gets the project's approved skills (they are
    /// excluded from git, so `git worktree add` does not bring them).
    pub(super) fn install_skills_in(&self, repo: &Path, worktree: &Path) -> AppResult<()> {
        let mut files = SkillFiles::default();
        for skill in self.store.skills(Some(repo), false)? {
            if skill.status == SkillStatus::Approved {
                self.write_skill_files(repo, worktree, true, &skill, &mut files)?;
            }
        }
        Ok(())
    }

    /// Approved skills an agent may read in the project containing `path`.
    fn agent_skills(&self, path: &Path) -> AppResult<Vec<Skill>> {
        Ok(self.skills(path)?.approved)
    }

    /// One approved skill for an agent, by name: the project's before a
    /// global one.
    pub fn read_skill_for_agent(&self, path: &Path, name: &str) -> AppResult<Option<Skill>> {
        let Ok(name) = normalize_skill_name(name) else {
            return Ok(None);
        };
        Ok(self
            .agent_skills(path)?
            .into_iter()
            .filter(|s| s.name == name)
            .min_by_key(|s| s.project.is_none()))
    }

    /// The skill tools an agent uses in the project containing `path`: what
    /// the memory MCP server serves. Proposals are signed by `author` and
    /// remember `task`.
    pub fn skill_tools(
        &self,
        path: &Path,
        task: Option<&TaskId>,
        author: &str,
    ) -> AppResult<Arc<dyn SkillTools>> {
        Ok(Arc::new(TaskSkills {
            hedy: self.clone(),
            project: self.project_at(path)?.path,
            task: task.cloned(),
            author: author.to_owned(),
        }))
    }

    pub(super) fn task_skills(
        &self,
        project: &Project,
        task: &TaskId,
        author: &str,
    ) -> Arc<dyn SkillTools> {
        Arc::new(TaskSkills {
            hedy: self.clone(),
            project: project.path.clone(),
            task: Some(task.clone()),
            author: author.to_owned(),
        })
    }
}

fn scope_label(skill: &Skill) -> String {
    if skill.project.is_some() {
        tr!("proyecto", "project")
    } else {
        "global".to_owned()
    }
}

/// What an agent reads for `skill_list`: one line per skill.
pub fn format_skills(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return tr!(
            "Todavía no hay habilidades aprobadas en este proyecto.",
            "This project has no approved skills yet."
        );
    }
    skills
        .iter()
        .map(|s| {
            let (description, _) = preview(&s.description, DESCRIPTION_PREVIEW);
            format!("- {} [{}]: {description}", s.name, scope_label(s))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One page of `skill_list` ([`SKILLS_PAGE`] by default), then how to ask
/// for more when the page is not the end.
pub fn format_skill_page(page: &Paged<Skill>) -> String {
    let mut text = format_skills(&page.items);
    if page.remaining > 0 && !page.items.is_empty() {
        let (n, offset) = (page.remaining, page.next_offset());
        text.push('\n');
        text.push_str(&tr!(
            "… {n} más: llama a skill_list con offset {offset}",
            "… {n} more: call skill_list with offset {offset}"
        ));
    }
    text
}

/// One whole skill, for `skill_get`.
pub fn format_skill(skill: &Skill) -> String {
    format!(
        "# {} [{}]\n{}\n\n{}",
        skill.name,
        scope_label(skill),
        skill.description,
        skill.body
    )
}

/// Skill tools for one agent run.
struct TaskSkills {
    hedy: Hedy,
    project: PathBuf,
    task: Option<TaskId>,
    author: String,
}

impl SkillTools for TaskSkills {
    fn list(&self) -> String {
        self.list_page(PageRequest::default())
    }

    fn list_page(&self, page: PageRequest) -> String {
        match self.hedy.agent_skills(&self.project) {
            Ok(skills) if experiments::on(Experiment::Search8) => {
                format_skill_page(&paginate(skills, page, SKILLS_PAGE, SKILLS_MAX))
            }
            Ok(skills) => format_skills(&skills),
            Err(e) => tr!(
                "No se pudieron leer las habilidades: {e}",
                "Could not read the skills: {e}"
            ),
        }
    }

    fn get(&self, name: &str) -> String {
        match self.hedy.read_skill_for_agent(&self.project, name) {
            Ok(Some(skill)) => format_skill(&skill),
            Ok(None) => tr!(
                "No hay una habilidad aprobada con ese nombre; skill_list las lista.",
                "There is no approved skill with that name; skill_list lists them."
            ),
            Err(e) => tr!(
                "No se pudieron leer las habilidades: {e}",
                "Could not read the skills: {e}"
            ),
        }
    }

    fn propose(&self, name: &str, description: &str, body: &str) -> String {
        let proposed = self.hedy.propose_skill(
            &self.project,
            name,
            description,
            body,
            &self.author,
            self.task.as_ref(),
        );
        let target = proposed.as_ref().ok().and_then(|skill| {
            let beside = self.hedy.skills_beside(skill).ok()?;
            update_target(&beside, skill)
        });
        match (proposed, target) {
            (Ok(skill), Some(_)) => {
                let name = skill.name;
                tr!(
                    "Propuesta guardada como actualización de la habilidad «{name}»; el usuario la revisará.",
                    "Proposal saved as an update of skill «{name}»; the user will review it."
                )
            }
            (Ok(skill), None) => {
                let name = skill.name;
                tr!(
                    "Propuesta guardada como «{name}»; el usuario la revisará.",
                    "Proposal saved as «{name}»; the user will review it."
                )
            }
            (Err(e), _) => tr!(
                "No se pudo proponer la habilidad: {e}",
                "Could not propose the skill: {e}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewTask;
    use crate::application::ports::FileSystem;
    use crate::application::testing::Fixture;
    use crate::domain::TaskMode;
    use crate::domain::skill::MANAGED_MARKER;

    const CLAUDE_FILE: &str = "/code/agenda/.claude/skills/run-e2e-tests/SKILL.md";
    const AGENTS_FILE: &str = "/code/agenda/.agents/skills/run-e2e-tests/SKILL.md";

    fn setup() -> Fixture {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.fs.add_git_root("/code/agenda");
        f.hedy
            .use_account(&f.dir("/code/agenda"), "personal", None)
            .unwrap();
        f
    }

    fn propose(f: &Fixture, body: &str) -> Skill {
        f.hedy
            .propose_skill(
                &f.dir("/code/agenda"),
                "Run E2E tests",
                "Run the end-to-end tests before a release.",
                body,
                "Claude",
                Some(&TaskId::new("t-1")),
            )
            .unwrap()
    }

    #[test]
    fn proposals_wait_and_approval_writes_each_cli_folder() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let proposal = propose(&f, "1. npm run build\n2. npm run test:e2e");
        assert_eq!(proposal.name, "run-e2e-tests");
        assert_eq!(proposal.source_task, Some(TaskId::new("t-1")));
        let tools = f.hedy.skill_tools(&agenda, None, "Codex").unwrap();
        assert!(
            tools.list().contains("no hay habilidades"),
            "proposals stay hidden"
        );
        assert!(tools.get("run-e2e-tests").contains("No hay una habilidad"));
        assert!(f.fs.files().is_empty());

        let change = f.hedy.approve_skill(proposal.id).unwrap();
        assert_eq!(change.skill.status, SkillStatus::Approved);
        assert_eq!(
            change.files.written,
            [PathBuf::from(CLAUDE_FILE), PathBuf::from(AGENTS_FILE)]
        );
        let file = f.fs.file(CLAUDE_FILE).unwrap();
        assert!(file.starts_with("---\nname: run-e2e-tests\n"));
        assert!(file.contains(MANAGED_MARKER) && file.contains("npm run test:e2e"));
        assert_eq!(f.fs.file(AGENTS_FILE), Some(file));
        assert_eq!(
            f.git.excluded(),
            [
                "/.claude/skills/run-e2e-tests/",
                "/.agents/skills/run-e2e-tests/"
            ]
        );

        assert_eq!(
            tools.list(),
            "- run-e2e-tests [proyecto]: Run the end-to-end tests before a release."
        );
        let whole = tools.get("Run E2E tests");
        assert!(
            whole.starts_with("# run-e2e-tests [proyecto]\n") && whole.contains("npm run build")
        );
        assert!(matches!(
            f.hedy.reject_skill(proposal.id),
            Err(AppError::SkillNotAProposal(_))
        ));
    }

    #[test]
    fn a_proposal_with_the_name_of_an_approved_skill_updates_it() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let old = f
            .hedy
            .approve_skill(propose(&f, "1. npm test").id)
            .unwrap()
            .skill;
        let tools = f
            .hedy
            .skill_tools(&agenda, Some(&TaskId::new("t-2")), "Codex")
            .unwrap();
        let answer = tools.propose(
            "run-e2e-tests",
            "Run the e2e tests, headless.",
            "1. npm run test:e2e -- --headless",
        );
        assert!(
            answer.contains("actualización de la habilidad «run-e2e-tests»"),
            "{answer}"
        );
        let view = f.hedy.skills(&agenda).unwrap();
        assert_eq!(
            view.proposed[0].replaces.as_ref().map(|s| s.id),
            Some(old.id)
        );
        assert!(
            f.fs.file(AGENTS_FILE).unwrap().contains("1. npm test"),
            "not yet"
        );

        // Rejecting changes nothing.
        f.hedy.reject_skill(view.proposed[0].skill.id).unwrap();
        assert!(f.hedy.skills(&agenda).unwrap().proposed.is_empty());

        tools.propose(
            "run-e2e-tests",
            "Headless now.",
            "1. npm run test:e2e -- --headless",
        );
        let update = f.hedy.skills(&agenda).unwrap().proposed[0].skill.clone();
        let change = f.hedy.approve_skill(update.id).unwrap();
        assert_eq!(change.skill.id, old.id);
        assert_eq!(
            (
                change.skill.description.as_str(),
                change.skill.author.as_str()
            ),
            ("Headless now.", "Codex")
        );
        assert!(change.skill.updated_at.is_some());
        assert!(f.fs.file(AGENTS_FILE).unwrap().contains("--headless"));
        let view = f.hedy.skills(&agenda).unwrap();
        assert_eq!((view.approved.len(), view.proposed.len()), (1, 0));
    }

    #[test]
    fn a_users_own_skill_file_is_never_overwritten_or_deleted() {
        let f = setup();
        f.fs.add_file(CLAUDE_FILE, "---\nname: run-e2e-tests\n---\nMine");
        let change = f.hedy.approve_skill(propose(&f, "1. npm test").id).unwrap();
        assert_eq!(change.files.skipped, [PathBuf::from(CLAUDE_FILE)]);
        assert_eq!(change.files.written, [PathBuf::from(AGENTS_FILE)]);
        assert_eq!(
            f.fs.file(CLAUDE_FILE).unwrap(),
            "---\nname: run-e2e-tests\n---\nMine"
        );

        let files = f.hedy.remove_skill(change.skill.id).unwrap();
        assert_eq!(files.removed, [PathBuf::from(AGENTS_FILE)]);
        assert!(f.fs.file(CLAUDE_FILE).is_some(), "still the user's");
        assert_eq!(f.fs.file(AGENTS_FILE), None);
        let removed = f.fs.removed_dirs();
        assert!(removed.contains(&f.dir("/code/agenda/.agents/skills/run-e2e-tests")));
        assert!(removed.contains(&f.dir("/code/agenda/.agents")));
        assert!(!removed.contains(&f.dir("/code/agenda")));
        assert!(
            f.hedy
                .skills(&f.dir("/code/agenda"))
                .unwrap()
                .approved
                .is_empty()
        );
    }

    #[test]
    fn secrets_bad_names_and_empty_bodies_are_refused() {
        let f = setup();
        let tools = f
            .hedy
            .skill_tools(&f.dir("/code/agenda"), None, "Kimi")
            .unwrap();
        let answer = tools.propose(
            "deploy",
            "Deploy to staging.",
            "1. export API_TOKEN=9f8e7d6c5b4a3210\n2. make deploy",
        );
        assert!(
            answer.contains("parece contener un secreto") && !answer.contains("9f8e7d"),
            "{answer}"
        );
        assert!(
            tools
                .propose("¿?", "x", "y")
                .contains("nombre de habilidad no válido")
        );
        assert!(
            tools
                .propose("deploy", "Deploy.", "  ")
                .contains("no puede estar vacío")
        );
        assert!(
            tools
                .propose("deploy", "Deploy.", "1. API_TOKEN=$API_TOKEN make deploy")
                .contains("Propuesta guardada como «deploy»")
        );
        assert_eq!(
            f.hedy
                .skills(&f.dir("/code/agenda"))
                .unwrap()
                .proposed
                .len(),
            1
        );
    }

    #[test]
    fn global_skills_live_only_in_hedy_and_stay_out_of_protected_projects() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let draft = |name: &str, global| SkillDraft {
            name: name.into(),
            description: "Write commit messages in English.".into(),
            body: "Small commits, imperative mood.".into(),
            global,
        };
        let change = f.hedy.add_skill(&agenda, &draft("commits", true)).unwrap();
        assert_eq!(change.skill.project, None);
        assert_eq!(change.skill.author, "user");
        assert!(change.files.written.is_empty() && f.fs.files().is_empty());
        assert!(matches!(
            f.hedy.add_skill(&agenda, &draft("commits", true)),
            Err(AppError::SkillNameTaken(_))
        ));

        f.hedy
            .set_setting(
                crate::application::Setting::DefaultAccount,
                Some("personal"),
            )
            .unwrap();
        let work = f.dir("/code/tienda");
        f.hedy.use_account(&work, "trabajo", Some(true)).unwrap();
        let view = f.hedy.skills(&work).unwrap();
        assert!(view.protected && view.approved.is_empty());
        assert!(matches!(
            f.hedy.add_skill(&work, &draft("x", true)),
            Err(AppError::GlobalSkillInProtectedProject)
        ));
        let tools = f.hedy.skill_tools(&work, None, "Claude").unwrap();
        assert!(tools.get("commits").contains("No hay"));
        assert!(matches!(
            f.hedy
                .edit_skill(change.skill.id, &work, &draft("commits", false)),
            Err(AppError::SkillNotFound(_))
        ));
        assert_eq!(
            f.hedy.skill_named(&agenda, "Commits").unwrap().id,
            change.skill.id
        );
    }

    #[test]
    fn editing_an_approved_skill_moves_its_files() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let skill = f
            .hedy
            .approve_skill(propose(&f, "1. npm test").id)
            .unwrap()
            .skill;
        let draft = SkillDraft {
            name: "e2e".into(),
            description: "Run the e2e tests.".into(),
            body: "1. npm run e2e".into(),
            global: false,
        };
        let change = f.hedy.edit_skill(skill.id, &agenda, &draft).unwrap();
        assert_eq!(change.skill.name, "e2e");
        assert_eq!(change.skill.author, "Claude", "the author stays");
        assert_eq!(change.files.removed.len(), 2);
        assert!(f.fs.file(AGENTS_FILE).is_none());
        let renamed =
            f.fs.file("/code/agenda/.agents/skills/e2e/SKILL.md")
                .unwrap();
        assert!(renamed.contains("npm run e2e"));

        // To global: the files go, the MCP tools keep serving it.
        let global = SkillDraft {
            global: true,
            ..draft
        };
        let change = f.hedy.edit_skill(skill.id, &agenda, &global).unwrap();
        assert_eq!(change.files.removed.len(), 2);
        assert!(f.fs.files().is_empty());
        let tools = f.hedy.skill_tools(&agenda, None, "x").unwrap();
        assert!(tools.list().contains("e2e [global]"));
    }

    #[test]
    fn task_worktrees_get_the_approved_skills() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        f.hedy.approve_skill(propose(&f, "1. npm test").id).unwrap();
        let task = f
            .hedy
            .start_task(NewTask {
                images: Vec::new(),
                project: agenda.clone(),
                prompt: "Arregla el login".into(),
                mode: TaskMode::Project,
                model: None,
                plan_first: false,
            })
            .unwrap();
        let wt = task.workspace.unwrap().worktree;
        let read = |path: PathBuf| f.fs.read_text(&path).unwrap();
        // Claude Code reads the main checkout's skills from a worktree
        // without its own `.claude/skills`.
        assert!(read(wt.join(".agents/skills/run-e2e-tests/SKILL.md")).is_some());
        assert!(read(wt.join(".claude/skills/run-e2e-tests/SKILL.md")).is_none());

        // An open worktree that has the folder (committed by the team)
        // follows later changes, Claude's copy included.
        f.fs.add_existing(&wt.to_string_lossy());
        f.fs.add_existing(&wt.join(".claude/skills").to_string_lossy());
        let lint = f
            .hedy
            .propose_skill(
                &agenda,
                "lint",
                "Lint first.",
                "1. npm run lint",
                "Codex",
                None,
            )
            .unwrap();
        f.hedy.approve_skill(lint.id).unwrap();
        assert!(read(wt.join(".claude/skills/lint/SKILL.md")).is_some());
        assert!(read(wt.join(".agents/skills/lint/SKILL.md")).is_some());

        let files = f.hedy.remove_skill(lint.id).unwrap();
        assert_eq!(files.removed.len(), 4, "project and worktree, both folders");
    }

    #[test]
    fn memory_links_carry_the_skill_tools() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let project = f.hedy.project_at(&agenda).unwrap();
        let account = f.hedy.account("personal").unwrap();
        let link = f
            .hedy
            .memory_link(&project, &TaskId::new("t-7"), &account)
            .unwrap();
        let skills = link.skills.unwrap();
        assert!(
            skills
                .propose("lint", "Lint first.", "npm run lint")
                .contains("Propuesta")
        );
        let proposal = &f.hedy.skills(&agenda).unwrap().proposed[0].skill;
        assert_eq!(proposal.author, "Claude");
        assert_eq!(proposal.source_task, Some(TaskId::new("t-7")));
    }
}
