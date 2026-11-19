//! Skills in and out of Hedy as `SKILL.md` files, the format every CLI
//! reads. An import is previewed first, with what is wrong with each skill,
//! and then only what the user saw goes in, checked again. An export writes
//! plain copies (without Hedy's marker) and never replaces a file or writes
//! through a link.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::ports::{NewSkill, SkillRevision};
use super::skills::{SkillDraft, SkillFiles, validated};
use super::{AppError, AppResult, Hedy};
use crate::domain::memory::USER_AUTHOR;
use crate::domain::skill::{
    BODY_MAX, check_visible, find_secret, normalize_skill_name, runs_commands,
};
use crate::domain::skill_check::{Finding, SkillText, Verdict, predicted};
use crate::domain::skill_import::{parse_skill_md, portable_skill_file};
use crate::domain::team::strip_hidden;
use crate::domain::{DomainError, Skill, SkillStatus};
use crate::tr;

/// Most skills one import looks at.
const IMPORT_MAX: usize = 100;

/// The file every skill folder has.
const SKILL_FILE: &str = "SKILL.md";

/// Most bytes read from one skill file: its frontmatter and a body at the
/// limit, written with multi-byte characters.
const READ_MAX: u64 = BODY_MAX as u64 * 4 + 64 * 1024;

/// Why a skill cannot be imported (or need not be).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ImportProblem {
    /// The file could not be read: not text, too big, not a regular file.
    Unreadable,
    /// Its `SKILL.md` is a link: it could point at any file of the user's.
    Link,
    /// No name can be made of it.
    BadName { name: String },
    /// No description, or one Hedy does not keep (too long, `<` or `>`).
    BadDescription,
    /// Empty or too long.
    BadBody,
    /// It looks like it holds a secret: nothing of it is shown.
    LooksSecret { what: String },
    /// Characters the reader cannot see, which the AIs would read: shown
    /// around the first one, each as `⟦U+…⟧`.
    HiddenText { at: String },
    /// It would run commands as soon as Claude Code loads it, without asking.
    RunsCommands,
    /// An approved skill already has its name (import replacing to update it).
    NameTaken,
    /// Another skill in the same import has its name.
    Repeated,
    /// The approved skill with its name already says exactly this.
    Unchanged,
}

impl ImportProblem {
    fn from_domain(error: DomainError) -> Self {
        match error {
            DomainError::InvalidSkillName(name) => Self::BadName { name },
            DomainError::InvalidSkillDescription => Self::BadDescription,
            DomainError::SkillLooksSecret(what) => Self::LooksSecret { what: what.into() },
            DomainError::SkillHiddenText(at) => Self::HiddenText { at },
            DomainError::SkillRunsCommands => Self::RunsCommands,
            _ => Self::BadBody,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unreadable => tr!(
                "no se pudo leer: no es un archivo de texto normal o es demasiado grande",
                "it could not be read: not a plain text file, or too big"
            ),
            Self::Link => tr!(
                "su SKILL.md es un enlace a otro archivo: no se lee",
                "its SKILL.md is a link to another file: it is not read"
            ),
            Self::BadName { name } => DomainError::InvalidSkillName(name.clone()).to_string(),
            Self::BadDescription => DomainError::InvalidSkillDescription.to_string(),
            Self::BadBody => DomainError::InvalidSkillBody.to_string(),
            Self::LooksSecret { what } => tr!(
                "parece contener un secreto ({what}): no se muestra ni se importa",
                "it looks like it holds a secret ({what}): it is not shown or imported"
            ),
            Self::HiddenText { at } => DomainError::SkillHiddenText(at.clone()).to_string(),
            Self::RunsCommands => tr!(
                "corre comandos en cuanto Claude Code la carga (!`…` o un bloque ```!), sin preguntar",
                "it runs commands as soon as Claude Code loads it (!`…` or a ```! block), without asking"
            ),
            Self::NameTaken => tr!(
                "ya tienes una habilidad aprobada con ese nombre",
                "you already have an approved skill with that name"
            ),
            Self::Repeated => tr!(
                "otra habilidad de esta importación tiene ese nombre",
                "another skill in this import has that name"
            ),
            Self::Unchanged => tr!(
                "ya la tienes tal cual",
                "you already have it exactly like this"
            ),
        }
    }
}

/// One skill found in what the user wants to import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidate {
    /// Its `SKILL.md` (or `.md`) file.
    pub source: PathBuf,
    /// What would be saved: name, description and body as Hedy keeps them
    /// (the name as a slug), for the project or for every project. Empty
    /// text when it looks like a secret.
    pub draft: SkillDraft,
    /// Frontmatter keys Hedy does not keep (`allowed-tools`, `hooks`…).
    pub dropped_keys: Vec<String>,
    /// Other files in its folder (scripts, references): Hedy keeps only the
    /// text, so steps that use them may not work.
    pub extra_files: Vec<String>,
    /// The approved skill with its name: importing replacing updates it.
    pub replaces: Option<Skill>,
    pub problem: Option<ImportProblem>,
    /// What Hedy measures on its own (level 1), beside the approved skills;
    /// empty when nothing of its text can be shown.
    pub findings: Vec<Finding>,
    /// What those findings suggest.
    pub verdict: Verdict,
}

impl ImportCandidate {
    /// The skill as the analysis reads it (for [`Hedy::review_skill`]).
    pub fn text(&self) -> SkillText<'_> {
        SkillText {
            name: &self.draft.name,
            description: &self.draft.description,
            body: &self.draft.body,
            extra_files: &self.extra_files,
        }
    }
}

/// What [`Hedy::preview_import`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPreview {
    /// At most [`IMPORT_MAX`], in path order.
    pub candidates: Vec<ImportCandidate>,
    /// How many skill files there were.
    pub total: usize,
}

/// What an import did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportReport {
    pub imported: Vec<Skill>,
    /// Approved skills replaced by the imported text (same id).
    pub updated: Vec<Skill>,
    /// By name, with the reason.
    pub skipped: Vec<(String, ImportProblem)>,
    pub files: SkillFiles,
}

/// What an export did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    pub written: Vec<PathBuf>,
    /// Something was already there (a file or a link): never replaced.
    pub skipped: Vec<PathBuf>,
}

impl Hedy {
    /// The skills in `source`, ready to import into `project` (or every
    /// project with `global`), each with what would stop it. `source` is a
    /// `SKILL.md`, a skill's folder, a folder of skill folders (such as
    /// `~/.claude/skills`) or a project, with its `.claude/skills`,
    /// `.agents/skills` and `skills` together. Nothing is saved.
    pub fn preview_import(
        &self,
        project: &Path,
        source: &Path,
        global: bool,
    ) -> AppResult<ImportPreview> {
        let project = self.project_at(project)?;
        let scope = self.skill_scope(&project, global)?;
        let files = self.skill_files_in(source);
        if files.is_empty() {
            return Err(AppError::NoSkillsToImport(source.to_owned()));
        }
        let approved = self.approved_in(scope.as_deref())?;
        let mut seen = HashSet::new();
        let total = files.len();
        let mut candidates = files
            .into_iter()
            .take(IMPORT_MAX)
            .map(|file| {
                let mut candidate = self.read_candidate(&file, global);
                if candidate.problem.is_none() {
                    let (draft, replaces, problem) =
                        checked(&candidate.draft, &approved, &mut seen);
                    candidate.draft = draft;
                    candidate.replaces = replaces;
                    candidate.problem = problem;
                }
                candidate
            })
            .collect::<Vec<_>>();
        for candidate in &mut candidates {
            let unseen = matches!(
                candidate.problem,
                Some(
                    ImportProblem::Unreadable
                        | ImportProblem::Link
                        | ImportProblem::LooksSecret { .. }
                        | ImportProblem::HiddenText { .. }
                )
            );
            if !unseen && !candidate.draft.body.is_empty() {
                let replacing = candidate.replaces.as_ref().map(|s| s.name.clone());
                candidate.findings =
                    self.measure_skill(&project.path, &candidate.text(), replacing.as_deref())?;
                candidate.verdict = predicted(&candidate.findings);
            }
        }
        Ok(ImportPreview { candidates, total })
    }

    /// Imports what the user saw in [`Hedy::preview_import`], checked again:
    /// approved right away (the user chose them), as with a skill they write.
    /// A skill with the name of an approved one updates it when its name is
    /// in `replace`, else it is skipped. Whatever happens, the skills saved
    /// so far reach the sessions.
    pub fn import_skills(
        &self,
        project: &Path,
        candidates: &[ImportCandidate],
        replace: &[String],
    ) -> AppResult<ImportReport> {
        let project = self.project_at(project)?;
        // Both scopes first: a refused one stops the import before it starts.
        let local = self.skill_scope(&project, false)?;
        let global = match candidates.iter().any(|c| c.draft.global) {
            true => Some(self.skill_scope(&project, true)?),
            false => None,
        };
        let mut report = ImportReport::default();
        let result = (|| {
            let mut seen = HashSet::new();
            for candidate in candidates {
                let scope = match candidate.draft.global {
                    true => global.clone().flatten(),
                    false => local.clone(),
                };
                if let Some(problem @ (ImportProblem::Unreadable | ImportProblem::Link)) =
                    &candidate.problem
                {
                    report
                        .skipped
                        .push((candidate.draft.name.clone(), problem.clone()));
                    continue;
                }
                let approved = self.approved_in(scope.as_deref())?;
                let (draft, replaces, problem) = checked(&candidate.draft, &approved, &mut seen);
                let problem = problem.or_else(|| {
                    (replaces.is_some() && !replace.contains(&draft.name))
                        .then_some(ImportProblem::NameTaken)
                });
                if let Some(problem) = problem {
                    report.skipped.push((draft.name, problem));
                    continue;
                }
                let skill = match replaces {
                    Some(old) => {
                        if let Some(project) = &old.project {
                            self.tidy_skill(project, &old, &mut report.files);
                        }
                        self.store.revise_skill(
                            old.id,
                            &SkillRevision {
                                project: old.project.clone(),
                                name: draft.name,
                                description: draft.description,
                                body: draft.body,
                                author: USER_AUTHOR.to_owned(),
                                updated_at: self.now_secs(),
                            },
                        )?;
                        let skill = self.skill_or_error(old.id)?;
                        report.updated.push(skill.clone());
                        skill
                    }
                    None => {
                        let skill = self.store.add_skill(&NewSkill {
                            project: scope,
                            name: draft.name,
                            description: draft.description,
                            body: draft.body,
                            author: USER_AUTHOR.to_owned(),
                            status: SkillStatus::Approved,
                            created_at: self.now_secs(),
                            source_task: None,
                        })?;
                        report.imported.push(skill.clone());
                        skill
                    }
                };
                // Best effort: the skill is saved; worktree copies are extra.
                if let Ok(files) = self.install_skill(&skill) {
                    report.files.extend(files);
                }
            }
            Ok::<_, AppError>(())
        })();
        self.refresh_skills_plugins();
        result.map(|()| report)
    }

    /// Writes the approved skills `project` sees (all, or those in `names`)
    /// as `<dest>/<name>/SKILL.md`, the folder layout every CLI reads. A
    /// project skill goes before a global one with its name. Anything already
    /// there, a link included, is never replaced or written through.
    pub fn export_skills(
        &self,
        project: &Path,
        names: &[String],
        dest: &Path,
    ) -> AppResult<ExportReport> {
        let mut skills = self.skills(project)?.approved;
        skills.sort_by_key(|s| s.project.is_none());
        let mut seen = HashSet::new();
        skills.retain(|s| seen.insert(s.name.clone()));
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        if !names.is_empty() {
            let mut wanted = Vec::new();
            for raw in names {
                let name = normalize_skill_name(raw)
                    .ok()
                    .filter(|name| skills.iter().any(|s| s.name == *name))
                    .ok_or_else(|| AppError::SkillNotFound(raw.trim().to_owned()))?;
                wanted.push(name);
            }
            skills.retain(|s| wanted.contains(&s.name));
        }
        let mut report = ExportReport::default();
        for skill in &skills {
            let folder = dest.join(&skill.name);
            let path = folder.join(SKILL_FILE);
            let written = !self.fs.is_symlink(&folder)
                && self.fs.create_text(&path, &portable_skill_file(skill))?;
            match written {
                true => report.written.push(path),
                false => report.skipped.push(path),
            }
        }
        Ok(report)
    }

    fn approved_in(&self, scope: Option<&Path>) -> AppResult<Vec<Skill>> {
        Ok(self
            .store
            .skills(scope, false)?
            .into_iter()
            .filter(|s| s.status == SkillStatus::Approved)
            .collect())
    }

    /// The skill files in `source`, sorted (see [`Hedy::preview_import`]).
    fn skill_files_in(&self, source: &Path) -> Vec<PathBuf> {
        let markdown = source
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"));
        if markdown {
            return if self.fs.exists(source) || self.fs.is_symlink(source) {
                vec![source.to_owned()]
            } else {
                Vec::new()
            };
        }
        if let Some(own) = self.skill_file_of(source) {
            return vec![own];
        }
        let mut found = Vec::new();
        let mut folders_seen = HashSet::new();
        let folders = [
            source.to_owned(),
            source.join(".claude/skills"),
            source.join(".agents/skills"),
            source.join("skills"),
        ];
        for folder in folders {
            // A folder linked to another one counts once.
            let real = self.fs.canonicalize(&folder).unwrap_or(folder.clone());
            if !folders_seen.insert(real) {
                continue;
            }
            found.extend(
                self.fs
                    .entries(&folder)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|dir| self.skill_file_of(dir)),
            );
        }
        found.sort();
        found.dedup();
        found
    }

    /// A folder's `SKILL.md`, whatever its case.
    fn skill_file_of(&self, dir: &Path) -> Option<PathBuf> {
        self.fs
            .entries(dir)
            .unwrap_or_default()
            .into_iter()
            .find(|path| is_skill_file(path))
    }

    /// A skill as its file says, unchecked. Without a name, its folder's
    /// (or the file's own, for a loose `.md`).
    fn read_candidate(&self, file: &Path, global: bool) -> ImportCandidate {
        let in_folder = is_skill_file(file);
        let fallback = if in_folder {
            file.parent().and_then(Path::file_name)
        } else {
            file.file_stem()
        }
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
        let extra_files = match file.parent().filter(|_| in_folder) {
            Some(folder) => {
                let mut names: Vec<String> = self
                    .fs
                    .entries(folder)
                    .unwrap_or_default()
                    .iter()
                    .filter(|path| path.as_path() != file)
                    .filter_map(|path| path.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .collect();
                names.sort();
                names
            }
            None => Vec::new(),
        };
        let (text, problem) = if self.fs.is_symlink(file) {
            (None, Some(ImportProblem::Link))
        } else {
            match self.fs.read_regular_text(file, READ_MAX) {
                Ok(Some(text)) => (Some(text), None),
                _ => (None, Some(ImportProblem::Unreadable)),
            }
        };
        let parsed = text.as_deref().map(parse_skill_md).unwrap_or_default();
        ImportCandidate {
            source: file.to_owned(),
            draft: SkillDraft {
                name: parsed.name.unwrap_or(fallback),
                description: parsed.description.unwrap_or_default(),
                body: parsed.body,
                global,
            },
            dropped_keys: parsed.dropped_keys,
            extra_files,
            replaces: None,
            problem,
            findings: Vec::new(),
            verdict: Verdict::Untested,
        }
    }
}

fn is_skill_file(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(SKILL_FILE))
}

/// The checks every skill passes (slug, limits, no secrets, nothing
/// hidden), plus those of an import: no commands on load, one skill per
/// name, something new. Returns the draft as it would be saved and the
/// approved skill it would replace.
fn checked(
    draft: &SkillDraft,
    approved: &[Skill],
    seen: &mut HashSet<(String, bool)>,
) -> (SkillDraft, Option<Skill>, Option<ImportProblem>) {
    // Before anything else: a secret is never shown again, not even in the
    // preview, whatever else is wrong.
    let visible = |text: &str| strip_hidden(text);
    let secret =
        find_secret(&visible(&draft.description)).or_else(|| find_secret(&visible(&draft.body)));
    if let Some(what) = secret {
        let draft = SkillDraft {
            description: String::new(),
            body: String::new(),
            ..draft.clone()
        };
        let what = what.to_owned();
        return (draft, None, Some(ImportProblem::LooksSecret { what }));
    }
    let (name, description, body) = match validated(&draft.name, &draft.description, &draft.body) {
        Ok(valid) => valid,
        Err(AppError::Domain(error)) => {
            let problem = ImportProblem::from_domain(error);
            let mut draft = draft.clone();
            if matches!(problem, ImportProblem::LooksSecret { .. }) {
                draft.description.clear();
                draft.body.clear();
            }
            return (draft, None, Some(problem));
        }
        Err(_) => return (draft.clone(), None, Some(ImportProblem::BadBody)),
    };
    if let Err(error) = check_visible(&body) {
        return (draft.clone(), None, Some(ImportProblem::from_domain(error)));
    }
    let replaces = approved.iter().find(|s| s.name == name).cloned();
    let unchanged = replaces
        .as_ref()
        .is_some_and(|old| old.description == description && old.body == body);
    let problem = if runs_commands(&body) || runs_commands(&description) {
        Some(ImportProblem::RunsCommands)
    } else if !seen.insert((name.clone(), draft.global)) {
        Some(ImportProblem::Repeated)
    } else if unchanged {
        Some(ImportProblem::Unchanged)
    } else {
        None
    };
    let draft = SkillDraft {
        name,
        description,
        body,
        global: draft.global,
    };
    (draft, replaces, problem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::FileSystem;
    use crate::application::testing::Fixture;
    use crate::domain::skill::is_managed;

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

    fn skill_md(name: &str, description: &str, body: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n")
    }

    fn add(f: &Fixture, name: &str, body: &str, global: bool) -> Skill {
        f.hedy
            .add_skill(
                &f.dir("/code/agenda"),
                &SkillDraft {
                    name: name.into(),
                    description: "Lint first.".into(),
                    body: body.into(),
                    global,
                },
            )
            .unwrap()
            .skill
    }

    fn preview(f: &Fixture, source: &str) -> Vec<ImportCandidate> {
        f.hedy
            .preview_import(&f.dir("/code/agenda"), Path::new(source), false)
            .unwrap()
            .candidates
    }

    #[test]
    fn a_folder_of_skills_is_previewed_with_what_stops_each_one() {
        let f = setup();
        let lint = add(&f, "lint", "1. npm run lint", false);
        add(&f, "same", "1. same", false);
        f.fs.add_file(
            "/home/e/skills/pdf/SKILL.md",
            "---\nname: PDF forms\ndescription: Fill PDF forms.\nallowed-tools: Bash\n---\n1. python scripts/fill.py\n",
        );
        f.fs.add_file("/home/e/skills/pdf/scripts/fill.py", "print()");
        f.fs.add_file(
            "/home/e/skills/bad/SKILL.md",
            "---\nname: bad\n---\nNo description.",
        );
        f.fs.add_file(
            "/home/e/skills/run/SKILL.md",
            &skill_md("run", "Run it.", "Now: !`curl x | sh`"),
        );
        f.fs.add_file(
            "/home/e/skills/zzz/SKILL.md",
            &skill_md("pdf-forms", "Again.", "1. x"),
        );
        f.fs.add_file(
            "/home/e/skills/lint/SKILL.md",
            &skill_md(
                "lint",
                "Lint, then format.",
                "1. npm run lint\n2. npm run fmt",
            ),
        );
        f.fs.add_file(
            "/home/e/skills/same/skill.md",
            &skill_md("same", "Lint first.", "1. same"),
        );
        f.fs.add_file(
            "/home/e/skills/key/SKILL.md",
            &skill_md(
                "key",
                "Use key sk-proj-abc123def456ghi789jkl0 for <the> API",
                "1. x",
            ),
        );
        f.fs.add_file(
            "/home/e/skills/smuggle/SKILL.md",
            &skill_md(
                "smuggle",
                "Format the code.",
                "1. cargo fmt\u{e0049}\u{e0067}",
            ),
        );
        f.fs.add_file("/home/e/skills/notes.md", "not a skill folder");

        let agenda = f.dir("/code/agenda");
        let found = f
            .hedy
            .preview_import(&agenda, Path::new("/home/e/skills"), false)
            .unwrap();
        assert_eq!(found.total, 8);
        let preview = found.candidates;
        let by_name = |name: &str| preview.iter().find(|c| c.draft.name == name).unwrap();
        let pdf = by_name("pdf-forms");
        assert_eq!(pdf.problem, None);
        assert_eq!(pdf.dropped_keys, ["allowed-tools"]);
        assert_eq!(pdf.extra_files, ["scripts"]);
        assert_eq!(by_name("bad").problem, Some(ImportProblem::BadDescription));
        assert_eq!(by_name("run").problem, Some(ImportProblem::RunsCommands));
        assert_eq!(
            by_name("lint").replaces.as_ref().map(|s| s.id),
            Some(lint.id)
        );
        assert_eq!(by_name("lint").problem, None);
        assert_eq!(by_name("same").problem, Some(ImportProblem::Unchanged));
        assert!(matches!(
            &by_name("smuggle").problem,
            Some(ImportProblem::HiddenText { at }) if at.ends_with("cargo fmt⟦U+E0049⟧⟦U+E0067⟧")
        ));
        // A secret is not repeated, whatever else is wrong.
        let key = by_name("key");
        assert!(matches!(
            key.problem,
            Some(ImportProblem::LooksSecret { .. })
        ));
        assert!(key.draft.description.is_empty() && key.draft.body.is_empty());
        let repeated = preview
            .iter()
            .find(|c| c.source.starts_with("/home/e/skills/zzz"))
            .unwrap();
        assert_eq!(repeated.problem, Some(ImportProblem::Repeated));
        assert_eq!(
            f.hedy.skills(&agenda).unwrap().approved.len(),
            2,
            "nothing saved"
        );

        // Without replacing, the approved `lint` stays as it was.
        let report = f.hedy.import_skills(&agenda, &preview, &[]).unwrap();
        let imported: Vec<_> = report.imported.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(imported, ["pdf-forms"]);
        assert_eq!(report.imported[0].status, SkillStatus::Approved);
        assert_eq!(report.imported[0].author, USER_AUTHOR);
        let skipped: Vec<_> = report
            .skipped
            .iter()
            .map(|(n, p)| (n.as_str(), p.clone()))
            .collect();
        assert!(skipped.contains(&("lint", ImportProblem::NameTaken)));
        assert!(skipped.contains(&("run", ImportProblem::RunsCommands)));
        assert!(skipped.contains(&("pdf-forms", ImportProblem::Repeated)));
        assert!(skipped.contains(&("same", ImportProblem::Unchanged)));
        assert_eq!(skipped.len(), 7);
        assert_eq!(
            f.hedy.skill_named(&agenda, "lint").unwrap().body,
            "1. npm run lint"
        );

        // Replacing by name updates it, keeping its id.
        let lint_only: Vec<_> = preview
            .into_iter()
            .filter(|c| c.draft.name == "lint")
            .collect();
        let report = f
            .hedy
            .import_skills(&agenda, &lint_only, &["lint".into()])
            .unwrap();
        assert_eq!(report.updated.len(), 1);
        assert_eq!(report.updated[0].id, lint.id);
        assert!(report.updated[0].body.contains("npm run fmt"));
    }

    #[test]
    fn a_file_a_skill_folder_or_every_skills_folder_of_a_project_can_be_imported() {
        let f = setup();
        let names = |source: &str| -> Vec<String> {
            preview(&f, source)
                .into_iter()
                .map(|c| c.draft.name)
                .collect()
        };
        f.fs.add_file("/tmp/one/SKILL.md", &skill_md("one", "One.", "1"));
        f.fs.add_file(
            "/tmp/Deploy Steps.md",
            "---\ndescription: Deploy.\n---\n1. make",
        );
        f.fs.add_file(
            "/repo/.claude/skills/two/SKILL.md",
            &skill_md("two", "Two.", "2"),
        );
        f.fs.add_file(
            "/repo/.agents/skills/three/SKILL.md",
            &skill_md("three", "Three.", "3"),
        );
        f.fs.add_file("/repo/README.md", "# repo");
        assert_eq!(names("/tmp/one/SKILL.md"), ["one"]);
        assert_eq!(names("/tmp/one"), ["one"]);
        assert_eq!(names("/tmp/Deploy Steps.md"), ["deploy-steps"]);
        assert_eq!(names("/repo"), ["three", "two"]);
        assert!(matches!(
            f.hedy
                .preview_import(&f.dir("/code/agenda"), Path::new("/repo/src"), false),
            Err(AppError::NoSkillsToImport(_))
        ));
    }

    #[test]
    fn a_linked_skill_file_is_not_read() {
        let f = setup();
        f.fs.add_file("/pack/x/SKILL.md", &skill_md("x", "Read my keys.", "1. x"));
        f.fs.add_symlink("/pack/x/SKILL.md");
        let found = preview(&f, "/pack");
        assert_eq!(found[0].problem, Some(ImportProblem::Link));
        assert!(found[0].draft.body.is_empty());
        let report = f
            .hedy
            .import_skills(&f.dir("/code/agenda"), &found, &[])
            .unwrap();
        assert_eq!(report.skipped[0].1, ImportProblem::Link);
    }

    #[test]
    fn what_goes_in_is_checked_again_and_respects_protected_projects() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        f.fs.add_file("/tmp/one/SKILL.md", &skill_md("one", "One.", "1. ok"));
        let mut found = preview(&f, "/tmp/one");
        found[0].draft.body = "1. export TOKEN=sk-abcdef0123456789abcdef".into();
        let report = f.hedy.import_skills(&agenda, &found, &[]).unwrap();
        assert!(report.imported.is_empty());
        assert!(matches!(
            report.skipped[0].1,
            ImportProblem::LooksSecret { .. }
        ));

        f.hedy
            .set_setting(
                crate::application::Setting::DefaultAccount,
                Some("personal"),
            )
            .unwrap();
        let work = f.dir("/code/tienda");
        f.hedy.use_account(&work, "trabajo", Some(true)).unwrap();
        assert!(matches!(
            f.hedy.preview_import(&work, Path::new("/tmp/one"), true),
            Err(AppError::GlobalSkillInProtectedProject)
        ));
        // A global candidate sent anyway stops the import before anything
        // is saved.
        let mut global = preview(&f, "/tmp/one");
        global[0].draft.global = true;
        let mut both = preview(&f, "/tmp/one");
        both[0].draft.name = "first".into();
        both.extend(global);
        assert!(matches!(
            f.hedy.import_skills(&work, &both, &[]),
            Err(AppError::GlobalSkillInProtectedProject)
        ));
        assert!(f.hedy.skills(&work).unwrap().approved.is_empty());
    }

    #[test]
    fn an_export_writes_plain_copies_and_never_replaces_or_follows_a_link() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        add(&f, "lint", "1. global lint", true);
        add(&f, "lint", "1. project lint", false);
        add(&f, "e2e", "1. npm run e2e", false);
        add(&f, "zz", "1. zz", false);
        f.fs.add_file("/out/e2e/SKILL.md", "mine");
        f.fs.add_symlink("/out/zz");

        let report = f
            .hedy
            .export_skills(&agenda, &[], Path::new("/out"))
            .unwrap();
        assert_eq!(report.written, [PathBuf::from("/out/lint/SKILL.md")]);
        assert_eq!(
            report.skipped,
            [
                PathBuf::from("/out/e2e/SKILL.md"),
                PathBuf::from("/out/zz/SKILL.md")
            ]
        );
        let lint = f.fs.file("/out/lint/SKILL.md").unwrap();
        assert!(lint.contains("project lint") && !is_managed(&lint));
        assert_eq!(f.fs.file("/out/e2e/SKILL.md").unwrap(), "mine");
        assert!(f.fs.file("/out/zz/SKILL.md").is_none());

        // Back in, as the same skill: nothing to do.
        let back = preview(&f, "/out/lint");
        assert_eq!(back[0].draft.body, "1. project lint");
        assert_eq!(back[0].problem, Some(ImportProblem::Unchanged));

        let one = f
            .hedy
            .export_skills(&agenda, &["E2E".into()], Path::new("/other"))
            .unwrap();
        assert_eq!(one.written, [PathBuf::from("/other/e2e/SKILL.md")]);
        assert!(matches!(
            f.hedy
                .export_skills(&agenda, &["nope".into()], Path::new("/other")),
            Err(AppError::SkillNotFound(_))
        ));
        assert!(
            f.fs.read_text(Path::new("/other/lint/SKILL.md"))
                .unwrap()
                .is_none()
        );
    }
}
