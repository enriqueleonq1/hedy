use std::path::Path;

use super::ports::PortError;
use super::{AppError, AppResult, Hedy, LimitCheck};
use crate::domain::{Account, LaunchPurpose, LaunchSpec, Project, policy};

/// What `hedy run` is about to start, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub account: Account,
    pub project: Option<Project>,
    pub spec: LaunchSpec,
}

/// Before `hedy run`: its account is close to or at its limit. Hedy only
/// warns and offers; it never switches (rule 3).
#[derive(Debug, Clone, PartialEq)]
pub struct RunLimitWarning {
    pub check: LimitCheck,
    /// What `hedy run -a` could use here instead (another provider's
    /// subscription, see [`policy::limit_alternatives`]), each with the
    /// cached figure of its window closest to the limit.
    pub alternatives: Vec<(Account, Option<f64>)>,
    /// The project is protected: it only uses its own account.
    pub protected: bool,
}

impl Hedy {
    /// The account for `dir`: explicit override → project account → default
    /// account, allowed there by the protection rules.
    pub(super) fn resolve_account(
        &self,
        dir: &Path,
        account_override: Option<&str>,
    ) -> AppResult<(Option<Project>, Account)> {
        let project = self.project_for(dir)?;
        let account_id = match account_override {
            Some(id) => Some(self.account(id)?.id),
            None => project
                .as_ref()
                .and_then(|p| p.account.clone())
                .or(self.default_account_id()?),
        };
        let account = self.account(account_id.ok_or(AppError::NoAccountResolved)?.as_str())?;
        policy::check_launch(project.as_ref(), &account.id, &self.projects()?)?;
        Ok((project, account))
    }

    /// Picks the account for `dir` (explicit override → project account → default
    /// account), enforces the protection rules, and builds the launch.
    pub fn resolve_run(
        &self,
        dir: &Path,
        account_override: Option<&str>,
        args: Vec<String>,
    ) -> AppResult<Resolved> {
        let (project, account) = self.resolve_account(dir, account_override)?;
        let program = self.binary_for(account.provider)?;
        let spec = LaunchSpec::for_account(&account, LaunchPurpose::Run(args), program.as_deref())?
            .expect("run is always launchable for subscription accounts")
            .in_dir(self.fs.canonicalize(dir)?);
        Ok(Resolved {
            account,
            project,
            spec,
        })
    }

    /// Whether to warn before running `resolved`: its account is at 90 % or
    /// more of a window, or at its limit, as far as the cache knows (nothing
    /// is asked, so the launch is not delayed).
    pub fn run_limit_warning(&self, resolved: &Resolved) -> AppResult<Option<RunLimitWarning>> {
        let Some(check) = self.limit_check(&resolved.account)? else {
            return Ok(None);
        };
        let accounts = self.accounts()?;
        let projects = self.projects()?;
        let alternatives = policy::limit_alternatives(
            &resolved.account,
            resolved.project.as_ref(),
            &accounts,
            &projects,
        )
        .into_iter()
        // `hedy run` opens a CLI: only subscriptions.
        .filter(|a| a.is_subscription())
        .map(|a| {
            let percent = self
                .plan_usage_of(a)
                .ok()
                .and_then(|s| s.usage)
                .and_then(|u| u.headline().map(|w| w.percent));
            (a.clone(), percent)
        })
        .collect();
        Ok(Some(RunLimitWarning {
            check,
            alternatives,
            protected: resolved.project.as_ref().is_some_and(|p| p.protected),
        }))
    }

    /// The user's shell in `dir`, exporting the project account's folder variable.
    pub fn shell_spec(&self, dir: &Path, shell: &str) -> AppResult<LaunchSpec> {
        let account = match self.project_for(dir)?.and_then(|p| p.account) {
            Some(id) => Some(self.account(id.as_str())?),
            None => None,
        };
        Ok(LaunchSpec::shell(shell, account.as_ref()).in_dir(self.fs.canonicalize(dir)?))
    }

    /// Opens `path` in VS Code (`code <path>`).
    pub fn open_in_editor(&self, path: &Path) -> AppResult<()> {
        let path = self.fs.canonicalize(path)?;
        let spec = LaunchSpec::program("code", vec![path.to_string_lossy().into_owned()]);
        Ok(self.runner.spawn_detached(&spec)?)
    }

    /// Opens a web page in the user's browser. Only `https` links.
    pub fn open_link(&self, url: &str) -> AppResult<()> {
        if !url.starts_with("https://") {
            return Err(AppError::InvalidSettingValue {
                key: "url".into(),
                value: url.to_owned(),
            });
        }
        self.system_open(url)
    }

    /// Opens a file with the system's app for its type.
    pub(super) fn open_with_system(&self, path: &Path) -> AppResult<()> {
        self.system_open(&path.to_string_lossy())
    }

    fn system_open(&self, target: &str) -> AppResult<()> {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let spec = LaunchSpec::program(opener, vec![target.to_owned()]);
        Ok(self.runner.spawn_detached(&spec)?)
    }

    /// Replaces the current process with the resolved CLI. Only returns on error.
    pub fn exec(&self, resolved: &Resolved) -> PortError {
        self.runner.exec(&resolved.spec)
    }
}

#[cfg(test)]
mod tests {
    use crate::application::ports::{PlanUsageRecord, RateLimit, UsageRepository};
    use crate::application::testing::Fixture;
    use crate::application::{AppError, Setting};
    use crate::domain::{
        AccountId, DomainError, LimitLevel, PlanUsage, UsageSource, UsageWindow, WindowKind,
    };

    #[test]
    fn uses_the_project_account() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "trabajo", None)
            .unwrap();
        let r = f
            .hedy
            .resolve_run(&f.dir("/code/tienda/api"), None, vec!["-c".into()])
            .unwrap();
        assert_eq!(r.account.id.as_str(), "trabajo");
        assert_eq!(r.spec.args, vec!["-c"]);
        assert_eq!(r.spec.cwd, Some(f.dir("/code/tienda/api")));
        assert_eq!(
            r.spec.env,
            vec![("CLAUDE_CONFIG_DIR".into(), "/data/accounts/trabajo".into())]
        );
    }

    #[test]
    fn falls_back_to_the_default_account() {
        let f = Fixture::new();
        f.add_claude("Personal");
        assert!(matches!(
            f.hedy.resolve_run(&f.dir("/tmp/x"), None, vec![]),
            Err(AppError::NoAccountResolved)
        ));
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("personal"))
            .unwrap();
        let r = f.hedy.resolve_run(&f.dir("/tmp/x"), None, vec![]).unwrap();
        assert_eq!(r.account.id.as_str(), "personal");
        assert_eq!(r.project, None);
    }

    #[test]
    fn protected_projects_reject_other_accounts() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "trabajo", Some(true))
            .unwrap();
        assert!(matches!(
            f.hedy
                .resolve_run(&f.dir("/code/tienda"), Some("personal"), vec![]),
            Err(AppError::Domain(DomainError::ProtectedProject { .. }))
        ));
        // And the reserved account is not used outside its project.
        assert!(matches!(
            f.hedy
                .resolve_run(&f.dir("/code/hobby"), Some("trabajo"), vec![]),
            Err(AppError::Domain(DomainError::AccountReserved { .. }))
        ));
    }

    #[test]
    fn default_and_protected_accounts_exclude_each_other() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("trabajo"))
            .unwrap();
        assert!(matches!(
            f.hedy
                .use_account(&f.dir("/code/tienda"), "trabajo", Some(true)),
            Err(AppError::DefaultCannotBeProtected(_))
        ));

        f.hedy
            .set_setting(Setting::DefaultAccount, Some("personal"))
            .unwrap();
        f.hedy
            .use_account(&f.dir("/code/tienda"), "trabajo", Some(true))
            .unwrap();
        assert!(matches!(
            f.hedy.set_setting(Setting::DefaultAccount, Some("trabajo")),
            Err(AppError::Domain(DomainError::AccountReserved { .. }))
        ));
    }

    #[test]
    fn shell_and_editor() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .use_account(&f.dir("/code/hedy"), "personal", None)
            .unwrap();
        let shell = f.hedy.shell_spec(&f.dir("/code/hedy/src"), "fish").unwrap();
        assert_eq!(shell.program, "fish");
        assert_eq!(shell.cwd, Some(f.dir("/code/hedy/src")));
        assert_eq!(shell.env[0].0, "CLAUDE_CONFIG_DIR");
        assert!(
            f.hedy
                .shell_spec(&f.dir("/tmp"), "fish")
                .unwrap()
                .env
                .is_empty()
        );

        f.hedy.open_in_editor(&f.dir("/code/hedy")).unwrap();
        let spec = f.runner.detached().pop().unwrap();
        assert_eq!(
            (spec.program.as_str(), spec.args),
            ("code", vec!["/code/hedy".to_owned()])
        );
    }

    fn cache(f: &Fixture, id: &str, windows: &[(WindowKind, Option<&str>, f64)], ago: i64) {
        let now = f.hedy.now_secs();
        let usage = PlanUsage {
            windows: windows
                .iter()
                .map(|(kind, model, percent)| UsageWindow {
                    kind: *kind,
                    minutes: None,
                    model: model.map(str::to_owned),
                    percent: *percent,
                    resets_text: None,
                    resets_at: Some(now + 3_600),
                })
                .collect(),
            checked_at: now - ago,
            source: UsageSource::ClaudeUsage,
            balance: None,
        };
        f.store
            .set_plan_usage(
                &AccountId::new(id).unwrap(),
                &PlanUsageRecord {
                    usage: Some(usage),
                    attempted_at: now - ago,
                    problem: None,
                },
            )
            .unwrap();
    }

    #[test]
    fn warns_from_the_cache_and_offers_only_another_provider() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.add_claude("Trabajo");
        f.add_codex("ChatGPT");
        f.add_kimi("Kimi");
        f.hedy
            .use_account(&f.dir("/code/tienda"), "personal", None)
            .unwrap();
        let resolved = f
            .hedy
            .resolve_run(&f.dir("/code/tienda"), None, vec![])
            .unwrap();
        // Nothing cached: nothing to say, and nothing is asked.
        assert_eq!(f.hedy.run_limit_warning(&resolved).unwrap(), None);
        cache(&f, "personal", &[(WindowKind::Session, None, 89.0)], 120);
        assert_eq!(f.hedy.run_limit_warning(&resolved).unwrap(), None);

        cache(
            &f,
            "personal",
            &[
                (WindowKind::Session, None, 40.0),
                (WindowKind::Week, None, 92.0),
            ],
            120,
        );
        cache(&f, "chatgpt", &[(WindowKind::Session, None, 12.0)], 60);
        let warning = f.hedy.run_limit_warning(&resolved).unwrap().unwrap();
        assert_eq!(warning.check.level, LimitLevel::Warning);
        let window = warning.check.window.clone().unwrap();
        assert_eq!((window.kind, window.percent), (WindowKind::Week, 92.0));
        assert!(f.hedy.now_secs() - warning.check.checked_at >= 120);
        assert!(!warning.protected);
        // Rule 3: not another Claude subscription; `hedy run` cannot open an API key.
        let offered: Vec<(&str, Option<f64>)> = warning
            .alternatives
            .iter()
            .map(|(a, p)| (a.id.as_str(), *p))
            .collect();
        assert_eq!(offered, [("chatgpt", Some(12.0))]);
        assert!(f.runner.captured().is_empty() && f.probe.calls().is_empty());

        // A protected project only uses its own account.
        f.hedy
            .set_setting(Setting::DefaultAccount, Some("trabajo"))
            .unwrap();
        f.hedy
            .use_account(&f.dir("/code/tienda"), "personal", Some(true))
            .unwrap();
        let resolved = f
            .hedy
            .resolve_run(&f.dir("/code/tienda"), None, vec![])
            .unwrap();
        let warning = f.hedy.run_limit_warning(&resolved).unwrap().unwrap();
        assert!(warning.protected && warning.alternatives.is_empty());
    }

    #[test]
    fn a_full_window_or_a_rejected_report_is_the_limit() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let resolved = f
            .hedy
            .resolve_run(&f.dir("/x"), Some("personal"), vec![])
            .unwrap();
        cache(&f, "personal", &[(WindowKind::Session, None, 100.0)], 30);
        let check = f.hedy.run_limit_warning(&resolved).unwrap().unwrap().check;
        assert_eq!(check.level, LimitLevel::Reached);

        // Team plans report no figure, only that the limit was hit.
        let f = Fixture::new();
        f.add_claude("Personal");
        let id = AccountId::new("personal").unwrap();
        let now = f.hedy.now_secs();
        f.store
            .set_rate_limit(
                &id,
                &RateLimit {
                    status: "rejected".into(),
                    utilization: None,
                    resets_at: Some(now + 600),
                    at: now - 10,
                },
            )
            .unwrap();
        let check = f
            .hedy
            .limit_check(&f.hedy.account("personal").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            (check.level, check.window, check.resets_at),
            (LimitLevel::Reached, None, Some(now + 600))
        );
        // A newer reading that is fine wins over the old report.
        cache(&f, "personal", &[(WindowKind::Session, None, 3.0)], 0);
        assert_eq!(
            f.hedy
                .limit_check(&f.hedy.account("personal").unwrap())
                .unwrap(),
            None
        );
    }

    #[test]
    fn binary_setting_overrides_the_program() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.hedy
            .set_setting(
                Setting::Binary(crate::domain::Provider::Claude),
                Some("/opt/claude"),
            )
            .unwrap();
        let r = f
            .hedy
            .resolve_run(&f.dir("/x"), Some("personal"), vec![])
            .unwrap();
        assert_eq!(r.spec.program, "/opt/claude");
    }
}
