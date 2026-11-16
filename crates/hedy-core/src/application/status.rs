use std::path::Path;

use super::ports::RateLimit;
use super::{AppResult, Hedy, PlanUsageState};
use crate::domain::{Account, AccountId, LoginState, Project, UsageSample, policy};

/// What Hedy recorded for an account. Subscription limits come only from what
/// the CLI reports; token counts are Hedy's own tally.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountUsage {
    /// Last 7 days.
    pub week: UsageSample,
    pub rate_limit: Option<RateLimit>,
    /// The plan's limit windows, from the cache (never starts a process).
    pub plan: PlanUsageState,
}

const WEEK_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    pub account: Account,
    pub login: LoginState,
    /// Projects assigned to this account.
    pub projects: Vec<Project>,
    /// Whether it may be used from the current directory (protection rules).
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub current_project: Option<Project>,
    pub default_account: Option<AccountId>,
    pub accounts: Vec<AccountStatus>,
}

impl Hedy {
    pub fn account_usage(&self, account: &AccountId) -> AppResult<AccountUsage> {
        let since = self.clock.now_ms() / 1000 - WEEK_SECS;
        Ok(AccountUsage {
            week: self.store.usage_since(account, since)?,
            rate_limit: self.store.rate_limit(account)?,
            plan: self.plan_usage(account.as_str())?,
        })
    }

    /// Accounts with their login state, as seen from `dir`. Login checks run in
    /// parallel because each one starts a CLI process.
    pub fn status(&self, dir: &Path) -> AppResult<Status> {
        let accounts = self.accounts()?;
        let projects = self.projects()?;
        let current_project = self.project_for(dir)?;

        let logins: Vec<AppResult<LoginState>> = std::thread::scope(|scope| {
            let handles: Vec<_> = accounts
                .iter()
                .map(|a| scope.spawn(move || self.login_state(a)))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("login check panicked"))
                .collect()
        });

        let accounts = accounts
            .into_iter()
            .zip(logins)
            .map(|(account, login)| {
                let available =
                    policy::check_launch(current_project.as_ref(), &account.id, &projects).is_ok();
                Ok(AccountStatus {
                    projects: projects
                        .iter()
                        .filter(|p| p.account.as_ref() == Some(&account.id))
                        .cloned()
                        .collect(),
                    login: login?,
                    available,
                    account,
                })
            })
            .collect::<AppResult<_>>()?;

        Ok(Status {
            current_project,
            default_account: self.default_account_id()?,
            accounts,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::application::testing::Fixture;
    use crate::domain::LoginState;

    #[test]
    fn reports_accounts_projects_and_reservations() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "trabajo", Some(true))
            .unwrap();
        f.runner.reply(r#"{"loggedIn":false}"#, 1);

        let outside = f.hedy.status(&f.dir("/code/hobby")).unwrap();
        assert_eq!(outside.current_project, None);
        let trabajo = &outside.accounts[1];
        assert_eq!(trabajo.projects.len(), 1);
        assert!(!trabajo.available);
        assert_eq!(trabajo.login, LoginState::LoggedOut);
        assert!(outside.accounts[0].available);

        let inside = f.hedy.status(&f.dir("/code/tienda/src")).unwrap();
        assert_eq!(inside.current_project.unwrap().name, "tienda");
        assert!(inside.accounts[1].available);
        assert!(
            !inside.accounts[0].available,
            "protected project only uses its account"
        );
    }
}
