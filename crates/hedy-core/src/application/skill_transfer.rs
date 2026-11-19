//! Skills in and out of Hedy as `SKILL.md` files, the format every CLI
//! reads. An import is previewed first, with what is wrong with each skill,
//! and then only what the user saw goes in, checked again. An export writes
//! plain copies (without Hedy's marker) and never replaces a file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::ports::{NewSkill, SkillRevision};
use super::skills::{SkillDraft, SkillFiles, validated};
use super::{AppError, AppResult, Hedy};
use crate::domain::memory::USER_AUTHOR;
use crate::domain::skill::{normalize_skill_name, runs_commands};
use crate::domain::skill_import::{parse_skill_md, portable_skill_file};
use crate::domain::{Skill, SkillStatus};
use crate::tr;

/// Most skills one import looks at.
const IMPORT_MAX: usize = 100;

/// The file every skill folder has.
const SKILL_FILE: &str = "SKILL.md";

/// Why a skill cannot be imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportProblem {
    /// The file could not be read (not text, no permission).
    Unreadable,
    /// The name, description or body break a rule: the message says which.
    Invalid(String),
    /// It would run commands as soon as Claude Code loads it, without asking.
    RunsCommands,
    /// An approved skill already has its name (import replacing to update it).
    NameTaken,
    /// Another skill in the same import has its name.
    Repeated,
}

impl ImportProblem {
    pub fn message(&self) -> String {
        match self {
            Self::Unreadable => tr!("no se pudo leer el archivo", "the file could not be read"),
            Self::Invalid(why) => why.clone(),
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
        }
    }
}

/// One skill found in what the user wants to import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidate {
    /// Its `SKILL.md` (or `.md`) file.
    pub source: PathBuf,
    /// What would be saved: name, description and body as Hedy keeps them
    /// (the name as a slug), for the project or for every project.
    pub draft: SkillDraft,
    /// Frontmatter keys Hedy does not keep (`allowed-tools`, `hooks`…).
    pub dropped_keys: Vec<String>,
    /// Other files in its folder (scripts, references): Hedy keeps only the
    /// text, so steps that use them may not work.
    pub extra_files: Vec<String>,
    /// The approved skill with its name: importing replacing updates it.
    pub replaces: Option<Skill>,
    pub problem: Option<ImportProblem>,
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
    /// A file was already there: never replaced.
    pub skipped: Vec<PathBuf>,
}

impl Hedy {
    /// The skills in `source`, ready to import into `project` (or every
    /// project with `global`), each with what would stop it. `source` is a
    /// `SKILL.md`, a skill's folder, a folder of skill folders (such as
    /// `~/.claude/skills`) or a project with `.claude/skills`,
    /// `.agents/skills` or `skills`. Nothing is saved.
    pub fn preview_import(
        &self,
        project: &Path,
        source: &Path,
        global: bool,
    ) -> AppResult<Vec<ImportCandidate>> {
        let project = self.project_at(project)?;
        let scope = self.skill_scope(&project, global)?;
        let files = self.skill_files_in(source);
        if files.is_empty() {
            return Err(AppError::NoSkillsToImport(source.to_owned()));
        }
        let approved = self.approved_in(scope.as_deref())?;
        let mut seen = HashSet::new();
        Ok(files
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
            .collect())
    }

    /// Imports what the user saw in [`Hedy::preview_import`], checked again:
    /// approved right away (the user chose them), as with a skill they write.
    /// A skill with the name of an approved one updates it with `replace`,
    /// else it is skipped.
    pub fn import_skills(
        &self,
        project: &Path,
        candidates: &[ImportCandidate],
        replace: bool,
    ) -> AppResult<ImportReport> {
        let project = self.project_at(project)?;
        let mut report = ImportReport::default();
        let mut seen = HashSet::new();
        for candidate in candidates {
            let scope = self.skill_scope(&project, candidate.draft.global)?;
            let approved = self.approved_in(scope.as_deref())?;
            let (draft, replaces, problem) = checked(&candidate.draft, &approved, &mut seen);
            let problem = problem
                .or_else(|| (replaces.is_some() && !replace).then_some(ImportProblem::NameTaken));
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
            report.files.extend(self.install_skill(&skill)?);
        }
        self.refresh_skills_plugins();
        Ok(report)
    }

    /// Writes the approved skills `project` sees (all, or those in `names`)
    /// as `<dest>/<name>/SKILL.md`, the folder layout every CLI reads. A
    /// project skill goes before a global one with its name. A file already
    /// there is never replaced.
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
            let path = dest.join(&skill.name).join(SKILL_FILE);
            if self.fs.exists(&path) || self.fs.is_symlink(&path) {
                report.skipped.push(path);
                continue;
            }
            self.fs.write_text(&path, &portable_skill_file(skill))?;
            report.written.push(path);
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
            return if self.fs.exists(source) {
                vec![source.to_owned()]
            } else {
                Vec::new()
            };
        }
        let own = source.join(SKILL_FILE);
        if self.fs.exists(&own) {
            return vec![own];
        }
        let folders = [
            source.to_owned(),
            source.join(".claude/skills"),
            source.join(".agents/skills"),
            source.join("skills"),
        ];
        for folder in folders {
            let mut found: Vec<PathBuf> = self
                .fs
                .entries(&folder)
                .unwrap_or_default()
                .into_iter()
                .map(|dir| dir.join(SKILL_FILE))
                .filter(|file| self.fs.exists(file))
                .collect();
            if !found.is_empty() {
                found.sort();
                return found;
            }
        }
        Vec::new()
    }

    /// A skill as its file says, unchecked. Without a name, its folder's
    /// (or the file's own, for a loose `.md`).
    fn read_candidate(&self, file: &Path, global: bool) -> ImportCandidate {
        let in_folder = file.file_name().is_some_and(|n| n == SKILL_FILE);
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
        let text = self.fs.read_text(file).ok().flatten();
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
            problem: text.is_none().then_some(ImportProblem::Unreadable),
        }
    }
}

/// The checks every skill passes (slug, limits, no secrets), plus those of
/// an import: no commands on load, one skill per name. Returns the draft as
/// it would be saved and the approved skill it would replace.
fn checked(
    draft: &SkillDraft,
    approved: &[Skill],
    seen: &mut HashSet<(String, bool)>,
) -> (SkillDraft, Option<Skill>, Option<ImportProblem>) {
    let (name, description, body) = match validated(&draft.name, &draft.description, &draft.body) {
        Ok(valid) => valid,
        Err(error) => {
            return (
                draft.clone(),
                None,
                Some(ImportProblem::Invalid(error.to_string())),
            );
        }
    };
    let replaces = approved.iter().find(|s| s.name == name).cloned();
    let problem = if runs_commands(&body) {
        Some(ImportProblem::RunsCommands)
    } else if !seen.insert((name.clone(), draft.global)) {
        Some(ImportProblem::Repeated)
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

    #[test]
    fn a_folder_of_skills_is_previewed_with_what_stops_each_one() {
        let f = setup();
        let lint = add(&f, "lint", "1. npm run lint", false);
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
        f.fs.add_file("/home/e/skills/notes.md", "not a skill folder");

        let agenda = f.dir("/code/agenda");
        let preview = f
            .hedy
            .preview_import(&agenda, Path::new("/home/e/skills"), false)
            .unwrap();
        let by_name = |name: &str| preview.iter().find(|c| c.draft.name == name).unwrap();
        assert_eq!(preview.len(), 5);
        let pdf = by_name("pdf-forms");
        assert_eq!(pdf.problem, None);
        assert_eq!(pdf.dropped_keys, ["allowed-tools"]);
        assert_eq!(pdf.extra_files, ["scripts"]);
        assert!(matches!(
            by_name("bad").problem,
            Some(ImportProblem::Invalid(_))
        ));
        assert_eq!(by_name("run").problem, Some(ImportProblem::RunsCommands));
        assert_eq!(
            by_name("lint").replaces.as_ref().map(|s| s.id),
            Some(lint.id)
        );
        assert_eq!(by_name("lint").problem, None);
        let repeated = preview
            .iter()
            .find(|c| c.source.starts_with("/home/e/skills/zzz"))
            .unwrap();
        assert_eq!(repeated.problem, Some(ImportProblem::Repeated));
        assert!(
            f.hedy.skills(&agenda).unwrap().approved.len() == 1,
            "nothing saved"
        );

        // Without replacing, the approved `lint` stays as it was.
        let report = f.hedy.import_skills(&agenda, &preview, false).unwrap();
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
        assert_eq!(skipped.len(), 4);
        assert_eq!(
            f.hedy.skill_named(&agenda, "lint").unwrap().body,
            "1. npm run lint"
        );

        // Replacing updates it, keeping its id.
        let lint_only: Vec<_> = preview
            .into_iter()
            .filter(|c| c.draft.name == "lint")
            .collect();
        let report = f.hedy.import_skills(&agenda, &lint_only, true).unwrap();
        assert_eq!(report.updated.len(), 1);
        assert_eq!(report.updated[0].id, lint.id);
        assert!(report.updated[0].body.contains("npm run fmt"));
    }

    #[test]
    fn a_file_a_skill_folder_or_a_projects_skills_can_be_imported() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        let names = |source: &str| -> Vec<String> {
            f.hedy
                .preview_import(&agenda, Path::new(source), false)
                .unwrap()
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
        f.fs.add_file("/repo/README.md", "# repo");
        assert_eq!(names("/tmp/one/SKILL.md"), ["one"]);
        assert_eq!(names("/tmp/one"), ["one"]);
        assert_eq!(names("/tmp/Deploy Steps.md"), ["deploy-steps"]);
        assert_eq!(names("/repo"), ["two"]);
        assert!(matches!(
            f.hedy
                .preview_import(&agenda, Path::new("/repo/src"), false),
            Err(AppError::NoSkillsToImport(_))
        ));
    }

    #[test]
    fn what_goes_in_is_checked_again_and_respects_protected_projects() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        f.fs.add_file("/tmp/one/SKILL.md", &skill_md("one", "One.", "1. ok"));
        let mut preview = f
            .hedy
            .preview_import(&agenda, Path::new("/tmp/one"), false)
            .unwrap();
        preview[0].draft.body = "1. export TOKEN=sk-abcdef0123456789abcdef".into();
        let report = f.hedy.import_skills(&agenda, &preview, false).unwrap();
        assert!(report.imported.is_empty());
        assert!(matches!(report.skipped[0].1, ImportProblem::Invalid(_)));

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
    }

    #[test]
    fn an_export_writes_plain_copies_and_never_replaces_a_file() {
        let f = setup();
        let agenda = f.dir("/code/agenda");
        add(&f, "lint", "1. global lint", true);
        add(&f, "lint", "1. project lint", false);
        add(&f, "e2e", "1. npm run e2e", false);
        f.fs.add_file("/out/e2e/SKILL.md", "mine");

        let report = f
            .hedy
            .export_skills(&agenda, &[], Path::new("/out"))
            .unwrap();
        assert_eq!(report.written, [PathBuf::from("/out/lint/SKILL.md")]);
        assert_eq!(report.skipped, [PathBuf::from("/out/e2e/SKILL.md")]);
        let lint = f.fs.file("/out/lint/SKILL.md").unwrap();
        assert!(lint.contains("project lint") && !is_managed(&lint));
        assert_eq!(f.fs.file("/out/e2e/SKILL.md").unwrap(), "mine");

        // Back in, as the same skill.
        let preview = f
            .hedy
            .preview_import(&agenda, Path::new("/out/lint"), false)
            .unwrap();
        assert_eq!(preview[0].draft.body, "1. project lint");
        assert!(preview[0].replaces.is_some());

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
