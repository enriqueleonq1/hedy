//! Tauri commands of the skills section: every skill in one place, "Usar
//! también fuera de Hedy", importing and exporting, the analysis (what Hedy
//! measures and what the AI thinks) and trying a skill on real tasks.

use std::path::{Path, PathBuf};

use hedy_core::application::{
    Hedy, ImportCandidate, ImportProblem, ImportReport, SkillOverview, SkillReview,
};
use hedy_core::domain::Skill;
use hedy_core::domain::skill_check::{Finding, SkillText, Verdict};
use hedy_core::domain::skill_trial::TrialOutcome;
use serde::Serialize;
use tauri::State;

use crate::AppState;
use crate::commands::{CommandResult, blocking};
use crate::dto::SkillFilesDto;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDto {
    #[serde(flatten)]
    pub problem: ImportProblem,
    pub message: String,
}

impl From<ImportProblem> for ProblemDto {
    fn from(problem: ImportProblem) -> Self {
        Self {
            message: problem.message(),
            problem,
        }
    }
}

/// One skill found in what the user wants to import.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidateDto {
    /// Its file: what the user picks to import.
    pub source: String,
    pub name: String,
    pub description: String,
    pub body: String,
    pub dropped_keys: Vec<String>,
    pub extra_files: Vec<String>,
    pub replaces: Option<Skill>,
    pub problem: Option<ProblemDto>,
    pub findings: Vec<Finding>,
    pub verdict: Verdict,
}

impl From<ImportCandidate> for ImportCandidateDto {
    fn from(c: ImportCandidate) -> Self {
        Self {
            source: c.source.display().to_string(),
            name: c.draft.name,
            description: c.draft.description,
            body: c.draft.body,
            dropped_keys: c.dropped_keys,
            extra_files: c.extra_files,
            replaces: c.replaces,
            problem: c.problem.map(Into::into),
            findings: c.findings,
            verdict: c.verdict,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewDto {
    pub candidates: Vec<ImportCandidateDto>,
    /// How many skill files there were (more than shown past the limit).
    pub total: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedDto {
    pub name: String,
    pub problem: ProblemDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReportDto {
    pub imported: Vec<Skill>,
    pub updated: Vec<Skill>,
    pub skipped: Vec<SkippedDto>,
    pub files: SkillFilesDto,
}

impl From<ImportReport> for ImportReportDto {
    fn from(r: ImportReport) -> Self {
        Self {
            imported: r.imported,
            updated: r.updated,
            skipped: r
                .skipped
                .into_iter()
                .map(|(name, problem)| SkippedDto {
                    name,
                    problem: problem.into(),
                })
                .collect(),
            files: r.files.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDto {
    #[serde(flatten)]
    pub review: SkillReview,
    /// Whose account read it.
    pub account: Option<String>,
}

impl From<SkillReview> for ReviewDto {
    fn from(review: SkillReview) -> Self {
        Self {
            account: review.account.as_ref().map(|a| a.name.clone()),
            review,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReportDto {
    pub written: Vec<String>,
    /// Something was already there: never replaced.
    pub skipped: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialCheckDto {
    /// The project's test command, guessed from its files.
    pub check: Option<String>,
    /// Checks run isolated on this machine (bubblewrap).
    pub isolated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedTrialDto {
    pub id: String,
    /// Its runs' task ids: with, without, with, …
    pub tasks: Vec<String>,
}

#[tauri::command]
pub async fn skills_overview(state: State<'_, AppState>) -> CommandResult<Vec<SkillOverview>> {
    blocking(&state, move |h| h.skills_overview()).await
}

#[tauri::command]
pub async fn set_skill_outside(
    state: State<'_, AppState>,
    id: i64,
    on: bool,
) -> CommandResult<SkillFilesDto> {
    blocking(&state, move |h| Ok(h.set_skill_outside(id, on)?.into())).await
}

#[tauri::command]
pub async fn preview_skill_import(
    state: State<'_, AppState>,
    path: PathBuf,
    source: PathBuf,
    global: bool,
) -> CommandResult<ImportPreviewDto> {
    blocking(&state, move |h| {
        let preview = h.preview_import(&path, &source, global)?;
        Ok(ImportPreviewDto {
            total: preview.total,
            candidates: preview.candidates.into_iter().map(Into::into).collect(),
        })
    })
    .await
}

/// The candidates of `source` the user picked (by file), read again: what
/// is imported is what Hedy reads now, never text sent back by the page.
fn picked(
    h: &Hedy,
    path: &Path,
    source: &Path,
    global: bool,
    files: &[String],
) -> hedy_core::application::AppResult<Vec<ImportCandidate>> {
    Ok(h.preview_import(path, source, global)?
        .candidates
        .into_iter()
        .filter(|c| files.contains(&c.source.display().to_string()))
        .collect())
}

/// Level 2 for one skill to import: what the AI thinks against the notes.
#[tauri::command]
pub async fn review_skill_import(
    state: State<'_, AppState>,
    path: PathBuf,
    source: PathBuf,
    global: bool,
    file: String,
) -> CommandResult<ReviewDto> {
    blocking(&state, move |h| {
        let candidate = picked(h, &path, &source, global, std::slice::from_ref(&file))?
            .into_iter()
            .next()
            .ok_or_else(|| {
                hedy_core::application::AppError::NoSkillsToImport(PathBuf::from(&file))
            })?;
        let review = h.review_skill(&path, &candidate.text(), &candidate.findings, None, None)?;
        Ok(review.into())
    })
    .await
}

#[tauri::command]
pub async fn import_skills(
    state: State<'_, AppState>,
    path: PathBuf,
    source: PathBuf,
    global: bool,
    files: Vec<String>,
    replace: Vec<String>,
) -> CommandResult<ImportReportDto> {
    blocking(&state, move |h| {
        let candidates = picked(h, &path, &source, global, &files)?;
        Ok(h.import_skills(&path, &candidates, &replace)?.into())
    })
    .await
}

/// Level 2 for a skill Hedy has, seen from `path` (its project's notes).
#[tauri::command]
pub async fn review_skill(
    state: State<'_, AppState>,
    id: i64,
    path: PathBuf,
) -> CommandResult<ReviewDto> {
    blocking(&state, move |h| {
        let skill = h
            .skills_overview()?
            .into_iter()
            .find(|o| o.skill.id == id)
            .map(|o| o.skill)
            .ok_or_else(|| hedy_core::application::AppError::SkillNotFound(id.to_string()))?;
        let (measured, _) = h.measure_approved_skill(&path, &skill)?;
        let text = SkillText {
            name: &skill.name,
            description: &skill.description,
            body: &skill.body,
            extra_files: &[],
        };
        Ok(h.review_skill(&path, &text, &measured, None, None)?.into())
    })
    .await
}

#[tauri::command]
pub async fn export_skills(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    to: PathBuf,
) -> CommandResult<ExportReportDto> {
    blocking(&state, move |h| {
        let report = h.export_skills_by_id(&ids, &to)?;
        let show =
            |paths: Vec<PathBuf>| paths.into_iter().map(|p| p.display().to_string()).collect();
        Ok(ExportReportDto {
            written: show(report.written),
            skipped: show(report.skipped),
        })
    })
    .await
}

#[tauri::command]
pub async fn propose_trial_tasks(
    state: State<'_, AppState>,
    path: PathBuf,
    id: i64,
) -> CommandResult<Vec<String>> {
    blocking(&state, move |h| {
        h.propose_trial_tasks(&path, id, None, None)
    })
    .await
}

#[tauri::command]
pub async fn trial_check(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CommandResult<TrialCheckDto> {
    blocking(&state, move |h| {
        Ok(TrialCheckDto {
            check: h.trial_check(&path)?,
            isolated: h.trial_checks_isolated(),
        })
    })
    .await
}

#[tauri::command]
pub async fn start_skill_trial(
    state: State<'_, AppState>,
    path: PathBuf,
    id: i64,
    tasks: Vec<String>,
    check: Option<String>,
) -> CommandResult<StartedTrialDto> {
    blocking(&state, move |h| {
        let trial = h.start_skill_trial(&path, id, &tasks, None, check.as_deref())?;
        Ok(StartedTrialDto {
            id: trial.id,
            tasks: trial
                .tasks
                .into_iter()
                .map(|t| t.id.as_str().to_owned())
                .collect(),
        })
    })
    .await
}

#[tauri::command]
pub async fn cancel_skill_trial(
    state: State<'_, AppState>,
    path: PathBuf,
    trial: String,
) -> CommandResult<()> {
    blocking(&state, move |h| {
        h.cancel_skill_trial(&path, &trial);
        Ok(())
    })
    .await
}

/// How a trial ended, once (`null` while it runs).
#[tauri::command]
pub async fn take_trial_outcome(
    state: State<'_, AppState>,
    trial: String,
) -> CommandResult<Option<TrialOutcome>> {
    blocking(&state, move |h| h.take_trial_outcome(&trial)).await
}
