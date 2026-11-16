use std::path::Path;

use super::ports::PortError;
use super::{AppError, AppResult, Hedy};
use crate::domain::{Account, LaunchPurpose, LaunchSpec, Project, policy};

/// What `hedy run` is about to start, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub account: Account,
    pub project: Option<Project>,
    pub spec: LaunchSpec,
}

impl Hedy {
    /// Picks the account for `dir` (explicit override → project account → default
    /// account), enforces the protection rules, and builds the launch.
    pub fn resolve_run(
        &self,
        dir: &Path,
        account_override: Option<&str>,
        args: Vec<String>,
    ) -> AppResult<Resolved> {
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
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let spec = LaunchSpec::program(opener, vec![url.to_owned()]);
        Ok(self.runner.spawn_detached(&spec)?)
    }

    /// Replaces the current process with the resolved CLI. Only returns on error.
    pub fn exec(&self, resolved: &Resolved) -> PortError {
        self.runner.exec(&resolved.spec)
    }
}

#[cfg(test)]
mod tests {
    use crate::application::testing::Fixture;
    use crate::application::{AppError, Setting};
    use crate::domain::DomainError;

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
