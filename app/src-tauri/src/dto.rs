//! Data sent to the web UI. The domain knows nothing about serde: mapping
//! lives here, in the driving adapter.

use std::collections::BTreeMap;
use std::path::Path;

use hedy_core::application::{AccountStatus, AppResult, Hedy, Setting};
use hedy_core::domain::{Account, ConfigFolder, Credentials, LoginState, Project, Provider};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDto {
    pub user_name: String,
    pub theme: String,
    pub data_dir: String,
    pub default_account: Option<String>,
    /// Custom binary path per CLI provider (`None` = found on `PATH`).
    pub binaries: BTreeMap<&'static str, Option<String>>,
    pub accounts: Vec<AccountDto>,
    pub projects: Vec<ProjectDto>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    pub id: String,
    pub name: String,
    pub provider: &'static str,
    pub provider_name: &'static str,
    /// `subscription` or `apiKey`.
    pub kind: &'static str,
    /// Hedy-managed folder; `None` for API keys and the CLI's default folder.
    pub folder: Option<String>,
    pub default_folder: bool,
    pub color: String,
    pub login: LoginDto,
    /// Paths of the projects assigned to this account.
    pub projects: Vec<String>,
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
        let (kind, folder, default_folder) = match &account.credentials {
            Credentials::ApiKey => ("apiKey", None, false),
            Credentials::Subscription(ConfigFolder::Default) => ("subscription", None, true),
            Credentials::Subscription(ConfigFolder::Managed(p)) => {
                ("subscription", Some(path_string(p)), false)
            }
        };
        Self {
            id: account.id.to_string(),
            name: account.name.clone(),
            provider: account.provider.as_str(),
            provider_name: account.provider.display_name(),
            kind,
            folder,
            default_folder,
            color: account.color.to_string(),
            login: login.into(),
            projects: projects.iter().map(|p| path_string(&p.path)).collect(),
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

    Ok(OverviewDto {
        user_name: hedy
            .setting(Setting::UserName)?
            .unwrap_or_else(system_first_name),
        theme: hedy
            .setting(Setting::Theme)?
            .unwrap_or_else(|| "system".into()),
        data_dir: path_string(data_dir),
        default_account: status.default_account.as_ref().map(ToString::to_string),
        binaries,
        accounts: status.accounts.iter().map(AccountDto::from).collect(),
        projects,
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
        assert_eq!(json["folder"], serde_json::Value::Null);
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
