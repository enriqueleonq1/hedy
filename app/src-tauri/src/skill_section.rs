//! Tauri commands of the skills section: every skill in one place, "Usar
//! también fuera de Hedy", importing and exporting, the analysis (what Hedy
//! measures and what the AI thinks) and trying a skill on real tasks.

use std::path::{Path, PathBuf};

use hedy_core::application::{
    Hedy, ImportCandidate, ImportProblem, ImportReport, SkillOverview, SkillReview,
};
use hedy_core::domain::Skill;
use hedy_core::domain::skill_check::{Finding, SkillText, Verdict};
use hedy_core::domain::skill_trial::{TrialOutcome, fingerprint};
use serde::{Deserialize, Serialize};
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
    /// Of the text shown: what is imported must still say this.
    pub fingerprint: String,
}

impl From<ImportCandidate> for ImportCandidateDto {
    fn from(c: ImportCandidate) -> Self {
        Self {
            fingerprint: candidate_print(&c),
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

fn candidate_print(c: &ImportCandidate) -> String {
    fingerprint(&c.draft.name, &c.draft.description, &c.draft.body)
}

/// A skill the user picked to import: its file, and what its text was then.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickInput {
    pub file: String,
    pub fingerprint: String,
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
    picks: &[PickInput],
) -> hedy_core::application::AppResult<Vec<ImportCandidate>> {
    // A file whose text changed since the user saw it is left out.
    Ok(h.preview_import(path, source, global)?
        .candidates
        .into_iter()
        .filter(|c| {
            let file = c.source.display().to_string();
            picks
                .iter()
                .any(|p| p.file == file && p.fingerprint == candidate_print(c))
        })
        .collect())
}

/// Level 2 for one skill to import: what the AI thinks against the notes.
#[tauri::command]
pub async fn review_skill_import(
    state: State<'_, AppState>,
    path: PathBuf,
    source: PathBuf,
    global: bool,
    pick: PickInput,
) -> CommandResult<ReviewDto> {
    blocking(&state, move |h| {
        let file = pick.file.clone();
        let candidate = picked(h, &path, &source, global, std::slice::from_ref(&pick))?
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
    picks: Vec<PickInput>,
    replace: Vec<String>,
) -> CommandResult<ImportReportDto> {
    blocking(&state, move |h| {
        let candidates = picked(h, &path, &source, global, &picks)?;
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
        // Only a skill `path`'s agents see goes to `path`'s account.
        let view = h.skills(&path)?;
        let skill = view
            .approved
            .into_iter()
            .chain(view.proposed.into_iter().map(|p| p.skill))
            .find(|s| s.id == id)
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

#[cfg(test)]
mod tests {
    use std::process::Command;

    use hedy_core::adapters::{self, HedyPaths};
    use hedy_core::application::NewAccount;
    use hedy_core::domain::Provider;
    use serde_json::{Value, json};
    use tauri::Manager;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, noop_assets};
    use tauri::webview::InvokeRequest;

    use crate::AppState;

    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[test]
    fn the_skills_section_over_ipc_with_the_real_backend() {
        hedy_core::i18n::set_language(hedy_core::i18n::Language::Spanish);
        let tmp = tempfile::tempdir().unwrap();
        let paths = HedyPaths::at(std::path::absolute(tmp.path().join("data")).unwrap());
        let program = std::env::current_exe().unwrap();
        let hedy = adapters::system_for_app(&paths, &program).unwrap();
        let project = std::path::absolute(tmp.path().join("tienda")).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        git(&project, &["init", "-q"]);
        hedy.add_account(NewAccount {
            name: "Claude".into(),
            provider: Provider::Claude,
            api_key: false,
            default_folder: false,
            color: None,
        })
        .unwrap();
        hedy.use_account(&project, "claude", None).unwrap();
        let source = tmp.path().join("descargas/pr-checklist");
        std::fs::create_dir_all(&source).unwrap();
        let skill_md = source.join("SKILL.md");
        std::fs::write(
            &skill_md,
            "---\nname: pr-checklist\ndescription: Before opening a pull request.\n---\n\n1. Update `CHANGELOG.md`\n2. Run `npm test`\n",
        )
        .unwrap();

        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                super::skills_overview,
                super::set_skill_outside,
                super::preview_skill_import,
                super::import_skills,
                super::export_skills,
                super::trial_check,
                super::take_trial_outcome,
            ])
            .build(tauri::generate_context!(
                test = true,
                assets = noop_assets()
            ))
            .unwrap();
        app.manage(AppState {
            hedy: hedy.clone(),
            paths,
            terminals: std::sync::Arc::default(),
        });
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let call = |cmd: &str, args: Value| -> Result<Value, Value> {
            get_ipc_response(
                &webview,
                InvokeRequest {
                    cmd: cmd.into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: InvokeBody::Json(args),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
            .map(|body| body.deserialize::<Value>().unwrap())
        };

        // Preview: what the page shows, in the shape types.ts expects.
        let preview = call(
            "preview_skill_import",
            json!({ "path": project, "source": source, "global": false }),
        )
        .unwrap();
        assert_eq!(preview["total"], 1);
        let candidate = &preview["candidates"][0];
        assert_eq!(candidate["name"], "pr-checklist");
        assert_eq!(candidate["problem"], Value::Null);
        assert_eq!(candidate["verdict"], "useful");
        assert!(
            candidate["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["kind"] == "concreteSteps")
        );
        let file = candidate["source"].as_str().unwrap().to_owned();
        let print = candidate["fingerprint"].as_str().unwrap().to_owned();

        // A file changed since the preview stays out.
        let stale = call(
            "import_skills",
            json!({ "path": project, "source": source, "global": false,
                    "picks": [{ "file": file, "fingerprint": "something else" }], "replace": [] }),
        )
        .unwrap();
        assert_eq!(stale["imported"], json!([]));
        let report = call(
            "import_skills",
            json!({ "path": project, "source": source, "global": false,
                    "picks": [{ "file": file, "fingerprint": print }], "replace": [] }),
        )
        .unwrap();
        assert_eq!(report["imported"][0]["name"], "pr-checklist");
        assert_eq!(report["imported"][0]["status"], "approved");

        // The section's list.
        let all = call("skills_overview", json!({})).unwrap();
        let ours = &all[0];
        assert_eq!(ours["projectName"], "tienda");
        assert_eq!(ours["outside"], false);
        assert_eq!(ours["trial"], Value::Null);
        assert!(ours["tokens"].as_u64().unwrap() > 0);
        let id = ours["skill"]["id"].as_i64().unwrap();

        // Outside Hedy too: written where the user's CLIs read it, out of git.
        let on = call("set_skill_outside", json!({ "id": id, "on": true })).unwrap();
        assert_eq!(on["written"].as_array().unwrap().len(), 2, "{on}");
        let claude = project.join(".claude/skills/pr-checklist/SKILL.md");
        assert!(
            std::fs::read_to_string(&claude)
                .unwrap()
                .contains("npm test")
        );
        assert_eq!(
            git(&project, &["status", "--porcelain"]),
            "",
            "excluded from git"
        );
        assert_eq!(
            call("skills_overview", json!({})).unwrap()[0]["outside"],
            true
        );

        // Export, then off.
        let to = tmp.path().join("export");
        std::fs::create_dir_all(&to).unwrap();
        let exported = call("export_skills", json!({ "ids": [id], "to": to })).unwrap();
        assert_eq!(exported["written"].as_array().unwrap().len(), 1);
        assert!(to.join("pr-checklist/SKILL.md").exists());
        let off = call("set_skill_outside", json!({ "id": id, "on": false })).unwrap();
        assert_eq!(off["removed"].as_array().unwrap().len(), 2);
        assert!(!claude.exists());

        // Trial helpers: what the dialog reads before and while it runs.
        let check = call("trial_check", json!({ "path": project })).unwrap();
        assert!(check["isolated"].is_boolean());
        assert_eq!(
            call("take_trial_outcome", json!({ "trial": "k-unknown" })).unwrap(),
            Value::Null
        );
    }
}
