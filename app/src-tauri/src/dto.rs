//! Data sent to the web UI. Domain types that already have the shape the UI
//! needs (tasks, notes, agent events, diffs, commits, phase plans) derive
//! serde and are sent as they are; the DTOs here are for the shapes that
//! combine or reshape domain data (overview, accounts, usage, memory screen…).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hedy_core::adapters::{CODE_MAP_PROGRAM, find_program};
use hedy_core::application::{
    AcceptPreview, AccountStatus, AccountUsage, AppResult, CheckLevel, DoctorReport, Hedy,
    MemoryView, NewImage, PlanUsageState, ProjectStatus, Proposal, Setting, SkillChange,
    SkillDraft, SkillFiles, SkillsView, TaskActivity, TestRun, UsageSummary,
};
use hedy_core::domain::{
    Account, ConfigFolder, Credentials, LoginState, Note, Phase, PhasePlan, PlanUsage, Project,
    PromptTemplate, Provider, Skill, Task, TaskMode, TaskState, UsageProblem, UsageSample,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDto {
    pub user_name: String,
    pub theme: String,
    /// `system`, `es` or `en`.
    pub language: String,
    /// The onboarding was finished or skipped.
    pub onboarded: bool,
    /// Desktop notifications about tasks (`ui.notifications`, on by default).
    pub notifications: bool,
    pub data_dir: String,
    pub default_account: Option<String>,
    /// Custom binary path per CLI provider (`None` = found on `PATH`).
    pub binaries: BTreeMap<&'static str, Option<String>>,
    /// Custom API endpoint per API-key provider (`None` = the provider's default).
    pub api_urls: BTreeMap<&'static str, Option<String>>,
    /// Price per API-key provider, `input/output` USD per million tokens.
    pub api_prices: BTreeMap<&'static str, Option<String>>,
    pub accounts: Vec<AccountDto>,
    pub projects: Vec<ProjectDto>,
    pub code_map: CodeMapDto,
}

/// The code-map MCP server (codebase-memory-mcp): Hedy only installs and registers it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeMapDto {
    /// Program registered in every run; `None`: off.
    pub enabled: Option<String>,
    /// Where it is installed, if found.
    pub found: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDto {
    pub notes: Vec<Note>,
    pub proposed: Vec<ProposalDto>,
    pub contradictions: Vec<[Note; 2]>,
    pub stale: Vec<Note>,
    pub used_last_task: Vec<Note>,
    pub readers: Vec<ReaderDto>,
    pub protected: bool,
}

/// A proposal with the approved note it would change (same topic) and the
/// approved notes that look like it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDto {
    pub note: Note,
    pub replaces: Option<Note>,
    pub similar: Vec<Note>,
}

impl From<Proposal> for ProposalDto {
    fn from(p: Proposal) -> Self {
        Self {
            note: p.note,
            replaces: p.replaces,
            similar: p.similar,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderDto {
    pub account: String,
    pub reads: bool,
}

impl From<MemoryView> for MemoryDto {
    fn from(view: MemoryView) -> Self {
        Self {
            notes: view.notes,
            proposed: view.proposed.into_iter().map(Into::into).collect(),
            contradictions: view
                .contradictions
                .into_iter()
                .map(|(a, b)| [a, b])
                .collect(),
            stale: view.stale,
            used_last_task: view.used_last_task,
            readers: view
                .readers
                .into_iter()
                .map(|(account, reads)| ReaderDto {
                    account: account.id.to_string(),
                    reads,
                })
                .collect(),
            protected: view.protected,
        }
    }
}

/// The skills screen: approved skills and proposals (with the approved
/// skill an update would replace).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsDto {
    pub approved: Vec<Skill>,
    pub proposed: Vec<SkillProposalDto>,
    pub protected: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProposalDto {
    pub skill: Skill,
    pub replaces: Option<Skill>,
}

impl From<SkillsView> for SkillsDto {
    fn from(view: SkillsView) -> Self {
        Self {
            approved: view.approved,
            proposed: view
                .proposed
                .into_iter()
                .map(|p| SkillProposalDto {
                    skill: p.skill,
                    replaces: p.replaces,
                })
                .collect(),
            protected: view.protected,
        }
    }
}

/// What happened to the CLIs' skill files, as paths to show.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillFilesDto {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    /// The user's own files with that name, left alone.
    pub skipped: Vec<String>,
}

impl From<SkillFiles> for SkillFilesDto {
    fn from(files: SkillFiles) -> Self {
        let show =
            |paths: Vec<PathBuf>| paths.into_iter().map(|p| p.display().to_string()).collect();
        Self {
            written: show(files.written),
            removed: show(files.removed),
            skipped: show(files.skipped),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillChangeDto {
    pub skill: Skill,
    pub files: SkillFilesDto,
}

impl From<SkillChange> for SkillChangeDto {
    fn from(change: SkillChange) -> Self {
        Self {
            skill: change.skill,
            files: change.files.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraftInput {
    pub name: String,
    pub description: String,
    pub body: String,
    pub global: bool,
}

impl From<SkillDraftInput> for SkillDraft {
    fn from(input: SkillDraftInput) -> Self {
        Self {
            name: input.name,
            description: input.description,
            body: input.body,
            global: input.global,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    pub id: String,
    pub name: String,
    pub provider: &'static str,
    pub provider_name: &'static str,
    /// `subscription` or `apiKey`.
    pub kind: &'static str,
    pub default_folder: bool,
    pub color: String,
    pub login: LoginDto,
    /// Paths of the projects assigned to this account.
    pub projects: Vec<String>,
    pub usage: UsageDto,
}

/// What Hedy recorded: its own token tally for the last 7 days, and the last
/// rate-limit status the CLI reported (if any).
#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageDto {
    pub week_tokens: u64,
    /// The CLI's estimate; not what a subscription bills.
    pub week_cost_usd: Option<f64>,
    pub limit: Option<LimitDto>,
    /// The plan's limit windows (cached reading).
    pub plan: PlanDto,
}

/// What is known about the plan's limits: the last reading (domain type, as
/// is), when Hedy last asked, and why it gave nothing if it did.
#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanDto {
    pub usage: Option<PlanUsage>,
    pub attempted_at: Option<i64>,
    pub problem: Option<UsageProblem>,
    /// Whether Hedy has a source for this account at all.
    pub supported: bool,
}

impl From<PlanUsageState> for PlanDto {
    fn from(state: PlanUsageState) -> Self {
        Self {
            usage: state.usage,
            attempted_at: state.attempted_at,
            problem: state.problem,
            supported: state.supported,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LimitDto {
    pub status: String,
    /// 0–100.
    pub percent: Option<f64>,
    pub resets_at: Option<i64>,
}

impl From<AccountUsage> for UsageDto {
    fn from(usage: AccountUsage) -> Self {
        Self {
            week_tokens: usage.week.total_tokens(),
            week_cost_usd: usage.week.cost_usd,
            limit: usage.rate_limit.map(|l| LimitDto {
                status: l.status,
                percent: l.utilization,
                resets_at: l.resets_at,
            }),
            plan: usage.plan.into(),
        }
    }
}

/// A task plus whether its agent is running right now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    #[serde(flatten)]
    pub task: Task,
    pub live: bool,
}

/// A row of the task board.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDto {
    pub task: TaskDto,
    pub state: TaskState,
    pub phase: Phase,
    pub agent: String,
    pub project_name: String,
    /// Unix seconds of the task's latest event.
    pub updated_at: i64,
    /// Messages waiting for the end of the agent's turn.
    pub queued: usize,
}

impl ActivityDto {
    pub fn new(item: TaskActivity, hedy: &Hedy) -> Self {
        Self {
            task: TaskDto {
                live: hedy.is_running(&item.task.id),
                task: item.task,
            },
            state: item.state,
            phase: item.phase,
            agent: item.agent,
            project_name: item.project_name,
            updated_at: item.updated_at,
            queued: item.queued,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTaskInput {
    pub project: PathBuf,
    pub prompt: String,
    pub mode: TaskMode,
    pub model: Option<String>,
    pub plan_first: bool,
    #[serde(default)]
    pub images: Vec<ImageInput>,
}

/// An image attached in the composer: its file name and its bytes in
/// base64. The backend checks the real type from the bytes.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    pub name: String,
    pub data: String,
}

/// The images as the use cases take them.
pub fn decode_images(images: Vec<ImageInput>) -> Result<Vec<NewImage>, String> {
    use base64::Engine as _;
    images
        .into_iter()
        .map(|image| {
            let data = base64::engine::general_purpose::STANDARD
                .decode(image.data.as_bytes())
                .map_err(|_| {
                    let name = &image.name;
                    hedy_core::tr!(
                        "«{name}» no se pudo leer como imagen",
                        "«{name}» could not be read as an image"
                    )
                })?;
            Ok(NewImage {
                name: image.name,
                data,
            })
        })
        .collect()
}

/// "Nuevo proyecto": a project started from scratch.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProjectInput {
    pub name: String,
    pub parent: PathBuf,
    pub account: Option<String>,
    pub protect: bool,
    pub client: Option<String>,
}

/// "Pregunta rápida": a question or research outside any project.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewConversationInput {
    pub prompt: String,
    pub mode: TaskMode,
    pub account: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub images: Vec<ImageInput>,
}

/// A prompt template with its placeholders (`{archivo}` → `archivo`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDto {
    #[serde(flatten)]
    pub template: PromptTemplate,
    pub placeholders: Vec<String>,
}

impl From<PromptTemplate> for TemplateDto {
    fn from(template: PromptTemplate) -> Self {
        Self {
            placeholders: template.placeholders(),
            template,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTemplateInput {
    /// `None`: every project.
    pub project: Option<PathBuf>,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoginDto {
    /// `loggedIn`, `loggedOut` or `unknown`.
    pub state: &'static str,
    pub plan: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub path: String,
    pub name: String,
    pub account: Option<String>,
    pub protected: bool,
    pub color: Option<String>,
    /// Accounts that may be assigned here (protection rules applied).
    pub offerable_accounts: Vec<String>,
    /// Who plans, implements, writes tests and reviews.
    pub phases: PhasePlan,
    pub client: Option<String>,
}

/// Tokens and estimated cost.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpendDto {
    pub tokens: u64,
    pub cost_usd: Option<f64>,
}

impl From<&UsageSample> for SpendDto {
    fn from(u: &UsageSample) -> Self {
        Self {
            tokens: u.total_tokens(),
            cost_usd: u.cost_usd,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummaryDto {
    pub projects: Vec<ProjectSpendDto>,
    pub clients: Vec<ClientSpendDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSpendDto {
    pub path: String,
    pub name: String,
    pub client: Option<String>,
    #[serde(flatten)]
    pub spend: SpendDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSpendDto {
    pub client: Option<String>,
    pub projects: usize,
    #[serde(flatten)]
    pub spend: SpendDto,
}

impl From<UsageSummary> for UsageSummaryDto {
    fn from(s: UsageSummary) -> Self {
        Self {
            projects: s
                .projects
                .iter()
                .map(|p| ProjectSpendDto {
                    path: path_string(&p.project),
                    name: p.name.clone(),
                    client: p.client.clone(),
                    spend: (&p.usage).into(),
                })
                .collect(),
            clients: s
                .clients
                .iter()
                .map(|c| ClientSpendDto {
                    client: c.client.clone(),
                    projects: c.projects,
                    spend: (&c.usage).into(),
                })
                .collect(),
        }
    }
}

/// One usage record of a project, for the per-day chart.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePointDto {
    pub at: i64,
    #[serde(flatten)]
    pub spend: SpendDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatusDto {
    pub branch: Option<String>,
    pub uncommitted: Option<usize>,
    pub last_tests: Option<TestRunDto>,
    /// Ids of the accounts that work in the project.
    pub agents: Vec<String>,
    pub code_map: bool,
    pub memory_notes: usize,
    pub memory_stale: usize,
    pub memory_proposed: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRunDto {
    pub command: String,
    pub passed: bool,
    pub at: i64,
}

impl From<TestRun> for TestRunDto {
    fn from(
        TestRun {
            command,
            passed,
            at,
        }: TestRun,
    ) -> Self {
        Self {
            command,
            passed,
            at,
        }
    }
}

/// What "Aceptar e integrar" is about to do, for the review sheet.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptPreviewDto {
    pub branch: String,
    pub target_branch: String,
    pub checkout_branch: Option<String>,
    pub files: usize,
    pub additions: u64,
    pub deletions: u64,
    pub message: String,
    /// The task's latest test run; `None` when it never ran tests.
    pub tests: Option<TestRunDto>,
}

impl From<AcceptPreview> for AcceptPreviewDto {
    fn from(p: AcceptPreview) -> Self {
        Self {
            branch: p.branch,
            target_branch: p.target_branch,
            checkout_branch: p.checkout_branch,
            files: p.files,
            additions: p.additions,
            deletions: p.deletions,
            message: p.message,
            tests: p.tests.map(Into::into),
        }
    }
}

impl From<ProjectStatus> for ProjectStatusDto {
    fn from(s: ProjectStatus) -> Self {
        Self {
            branch: s.branch,
            uncommitted: s.uncommitted,
            last_tests: s.last_tests.map(
                |TestRun {
                     command,
                     passed,
                     at,
                 }| TestRunDto {
                    command,
                    passed,
                    at,
                },
            ),
            agents: s.agents.iter().map(|a| a.id.to_string()).collect(),
            code_map: s.code_map,
            memory_notes: s.memory_notes,
            memory_stale: s.memory_stale,
            memory_proposed: s.memory_proposed,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAccountInput {
    pub name: String,
    pub provider: String,
    pub api_key: bool,
    pub default_folder: bool,
    pub color: Option<String>,
}

/// An official CLI found (or not) on this computer, for the onboarding.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedCliDto {
    pub provider: &'static str,
    pub installed: bool,
    pub login: LoginDto,
    /// The Hedy account already using its default folder.
    pub account: Option<String>,
}

impl From<&hedy_core::application::DetectedCli> for DetectedCliDto {
    fn from(d: &hedy_core::application::DetectedCli) -> Self {
        Self {
            provider: d.provider.as_str(),
            installed: d.installed,
            login: (&d.login).into(),
            account: d.account.as_ref().map(ToString::to_string),
        }
    }
}

/// The result of `doctor`: a list of checks, grouped by area.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReportDto {
    pub checks: Vec<CheckDto>,
    pub warnings: usize,
    pub errors: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckDto {
    /// `cli`, `account`, `hedy` or `environment`.
    pub area: &'static str,
    pub subject: String,
    /// `ok`, `info`, `warning` or `error`.
    pub level: &'static str,
    pub message: String,
    /// A command (or a short instruction) that fixes it.
    pub fix: Option<String>,
}

impl From<&DoctorReport> for DoctorReportDto {
    fn from(report: &DoctorReport) -> Self {
        Self {
            checks: report
                .checks
                .iter()
                .map(|c| CheckDto {
                    area: c.area.as_str(),
                    subject: c.subject.clone(),
                    level: c.level.as_str(),
                    message: c.message.clone(),
                    fix: c.fix.clone(),
                })
                .collect(),
            warnings: report.count(CheckLevel::Warning),
            errors: report.count(CheckLevel::Error),
        }
    }
}

impl From<&LoginState> for LoginDto {
    fn from(login: &LoginState) -> Self {
        match login {
            LoginState::LoggedIn { plan } => Self {
                state: "loggedIn",
                plan: plan.clone(),
            },
            LoginState::LoggedOut => Self {
                state: "loggedOut",
                plan: None,
            },
            LoginState::Unknown => Self {
                state: "unknown",
                plan: None,
            },
        }
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl AccountDto {
    pub fn new(account: &Account, login: &LoginState, projects: &[Project]) -> Self {
        let (kind, default_folder) = match &account.credentials {
            Credentials::ApiKey => ("apiKey", false),
            Credentials::Subscription(ConfigFolder::Default) => ("subscription", true),
            Credentials::Subscription(ConfigFolder::Managed(_)) => ("subscription", false),
        };
        Self {
            id: account.id.to_string(),
            name: account.name.clone(),
            provider: account.provider.as_str(),
            provider_name: account.provider.display_name(),
            kind,
            default_folder,
            color: account.color.to_string(),
            login: login.into(),
            projects: projects.iter().map(|p| path_string(&p.path)).collect(),
            usage: UsageDto::default(),
        }
    }
}

impl From<&AccountStatus> for AccountDto {
    fn from(status: &AccountStatus) -> Self {
        Self::new(&status.account, &status.login, &status.projects)
    }
}

impl ProjectDto {
    pub fn new(project: &Project, offerable_accounts: Vec<String>) -> Self {
        Self {
            path: path_string(&project.path),
            name: project.name.clone(),
            account: project.account.as_ref().map(ToString::to_string),
            protected: project.protected,
            color: project.color.map(|c| c.to_string()),
            offerable_accounts,
            phases: project.phases.clone(),
            client: project.client.clone(),
        }
    }
}

/// First name from the OS account, used until the user sets one.
fn system_first_name() -> String {
    whoami::realname()
        .ok()
        .and_then(|n| n.split_whitespace().next().map(str::to_owned))
        .filter(|n| !n.is_empty())
        .or_else(|| whoami::username().ok())
        .unwrap_or_default()
}

pub fn overview(hedy: &Hedy, data_dir: &Path) -> AppResult<OverviewDto> {
    // Seen from the home folder: no project is "current" in the app.
    let home = std::env::var_os("HOME")
        .map(Into::into)
        .unwrap_or(data_dir.to_owned());
    let status = hedy.status(&home)?;
    let projects = hedy
        .projects()?
        .iter()
        .map(|p| {
            let offerable = hedy.offerable_accounts(&p.path)?;
            Ok(ProjectDto::new(
                p,
                offerable.iter().map(ToString::to_string).collect(),
            ))
        })
        .collect::<AppResult<_>>()?;
    let binaries = Provider::ALL
        .into_iter()
        .filter(|p| p.program().is_some())
        .map(|p| Ok((p.as_str(), hedy.setting(Setting::Binary(p))?)))
        .collect::<AppResult<_>>()?;

    let api_urls = Provider::ALL
        .into_iter()
        .filter(|p| p.supports_api_key())
        .map(|p| Ok((p.as_str(), hedy.setting(Setting::ApiUrl(p))?)))
        .collect::<AppResult<_>>()?;
    let api_prices = Provider::ALL
        .into_iter()
        .filter(|p| p.supports_api_key())
        .map(|p| Ok((p.as_str(), hedy.setting(Setting::ApiPrice(p))?)))
        .collect::<AppResult<_>>()?;

    Ok(OverviewDto {
        user_name: hedy
            .setting(Setting::UserName)?
            .unwrap_or_else(system_first_name),
        theme: hedy
            .setting(Setting::Theme)?
            .unwrap_or_else(|| "system".into()),
        language: hedy
            .setting(Setting::Language)?
            .unwrap_or_else(|| "system".into()),
        onboarded: hedy.setting(Setting::Onboarded)?.as_deref() == Some("true"),
        notifications: hedy.notifications_on()?,
        data_dir: path_string(data_dir),
        default_account: status.default_account.as_ref().map(ToString::to_string),
        binaries,
        api_urls,
        api_prices,
        accounts: status
            .accounts
            .iter()
            .map(|s| {
                let mut dto = AccountDto::from(s);
                dto.usage = hedy.account_usage(&s.account.id)?.into();
                Ok(dto)
            })
            .collect::<AppResult<_>>()?,
        projects,
        code_map: CodeMapDto {
            enabled: hedy.setting(Setting::CodeMap)?,
            found: find_program(CODE_MAP_PROGRAM).map(|p| path_string(&p)),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hedy_core::domain::{AccountId, Color};

    #[test]
    fn account_dto_shape() {
        let account = Account::new(
            AccountId::new("claude-trabajo").unwrap(),
            "Claude trabajo",
            Provider::Claude,
            Credentials::Subscription(ConfigFolder::Default),
            Color::nth_in_palette(0),
        )
        .unwrap();
        let project = Project::new("/code/homming".into());
        let dto = AccountDto::new(
            &account,
            &LoginState::LoggedIn {
                plan: Some("team".into()),
            },
            &[project],
        );
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["providerName"], "Claude Code");
        assert_eq!(json["kind"], "subscription");
        assert_eq!(json["defaultFolder"], true);
        assert_eq!(json["color"], "#7AA2F7");
        assert_eq!(json["login"]["state"], "loggedIn");
        assert_eq!(json["login"]["plan"], "team");
        assert_eq!(json["projects"][0], "/code/homming");
    }

    #[test]
    fn new_account_input_from_camel_case() {
        let input: NewAccountInput = serde_json::from_str(
            r#"{"name":"Kimi","provider":"kimi","apiKey":true,"defaultFolder":false,"color":null}"#,
        )
        .unwrap();
        assert!(input.api_key);
        assert_eq!(input.provider, "kimi");
    }
}
