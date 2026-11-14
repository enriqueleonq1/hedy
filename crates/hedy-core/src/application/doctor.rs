//! `hedy doctor`: a health check of the official CLIs, the accounts, Hedy
//! itself and the environment. It only runs official commands (`--version`,
//! the CLIs' own login status) and never reads a login or prints a key. The
//! one thing it writes is the last CLI version seen, to warn when it changes.

use std::time::Duration;

use super::{AppResult, Hedy, Setting};
use crate::domain::env;
use crate::domain::version::parse_version;
use crate::domain::{
    Account, ConfigFolder, Credentials, LaunchPurpose, LaunchSpec, LoginState, Provider,
    UsageProblem, policy,
};
use crate::tr;

/// What a check is about. Reports list areas in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckArea {
    Cli,
    Account,
    Hedy,
    Environment,
}

impl CheckArea {
    pub const ALL: [CheckArea; 4] = [Self::Cli, Self::Account, Self::Hedy, Self::Environment];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Account => "account",
            Self::Hedy => "hedy",
            Self::Environment => "environment",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckLevel {
    Ok,
    /// Worth knowing, nothing to fix.
    Info,
    /// Works, but something deserves a look.
    Warning,
    /// Something Hedy needs is broken.
    Error,
}

impl CheckLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub area: CheckArea,
    /// What was checked: "Claude Code", an account, "git".
    pub subject: String,
    pub level: CheckLevel,
    /// One short sentence, in the current language.
    pub message: String,
    /// A command (or a short instruction) that fixes it.
    pub fix: Option<String>,
}

impl Check {
    fn new(
        area: CheckArea,
        subject: impl Into<String>,
        level: CheckLevel,
        message: impl Into<String>,
    ) -> Self {
        Self {
            area,
            subject: subject.into(),
            level,
            message: message.into(),
            fix: None,
        }
    }

    fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DoctorReport {
    /// Grouped by area, in [`CheckArea::ALL`] order.
    pub checks: Vec<Check>,
}

impl DoctorReport {
    pub fn count(&self, level: CheckLevel) -> usize {
        self.checks.iter().filter(|c| c.level == level).count()
    }

    pub fn has_errors(&self) -> bool {
        self.count(CheckLevel::Error) > 0
    }

    pub fn in_area(&self, area: CheckArea) -> impl Iterator<Item = &Check> {
        self.checks.iter().filter(move |c| c.area == area)
    }
}

/// `--version` should answer at once; a CLI that hangs must not hang Hedy.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// The desktop app, which also serves the memory MCP server and Claude
/// Code's `PreToolUse` hook (`app/src-tauri/src/main.rs`).
const APP_PROGRAM: &str = "hedy-app";

/// Internal state, not a user setting: the last version `doctor` saw.
fn seen_version_key(provider: Provider) -> String {
    format!("doctor.cli_version.{}", provider.as_str())
}

impl Hedy {
    /// Runs every check. CLIs and accounts start processes, so those run in
    /// parallel, like [`Hedy::detect_clis`].
    pub fn doctor(&self) -> AppResult<DoctorReport> {
        let accounts = self.accounts()?;
        let programs: Vec<(Provider, Option<String>, String)> = Provider::ALL
            .into_iter()
            .map(|p| {
                let configured = self.binary_for(p)?;
                let program = configured
                    .clone()
                    .or_else(|| p.program().map(str::to_owned))
                    .unwrap_or_default();
                Ok((p, configured, program))
            })
            .collect::<AppResult<_>>()?;
        let installed = |provider: Provider| {
            programs
                .iter()
                .find(|(p, ..)| *p == provider)
                .is_some_and(|(_, _, program)| self.runner.is_installed(program))
        };

        let (clis, account_checks) = std::thread::scope(|scope| {
            let clis: Vec<_> = programs
                .iter()
                .map(|(provider, configured, program)| {
                    let accounts = &accounts;
                    scope.spawn(move || {
                        self.check_cli(*provider, configured.as_deref(), program, accounts)
                    })
                })
                .collect();
            let account_checks: Vec<_> = accounts
                .iter()
                .map(|account| {
                    let cli_installed = installed(account.provider);
                    scope.spawn(move || self.check_account(account, cli_installed))
                })
                .collect();
            (
                clis.into_iter()
                    .map(|h| h.join().expect("CLI check panicked"))
                    .collect::<Vec<_>>(),
                account_checks
                    .into_iter()
                    .map(|h| h.join().expect("account check panicked"))
                    .collect::<Vec<_>>(),
            )
        });

        let mut checks = clis.into_iter().collect::<AppResult<Vec<_>>>()?;
        for account in account_checks {
            checks.extend(account?);
        }
        checks.extend(self.check_default_account(&accounts)?);
        checks.extend(self.check_hedy()?);
        checks.extend(self.check_environment());
        checks.sort_by_key(|c| c.area);
        Ok(DoctorReport { checks })
    }

    fn check_cli(
        &self,
        provider: Provider,
        configured: Option<&str>,
        program: &str,
        accounts: &[Account],
    ) -> AppResult<Check> {
        let check = |level, message: String| {
            Check::new(CheckArea::Cli, provider.cli_name(), level, message)
        };
        let key = Setting::Binary(provider).key();
        if !self.runner.is_installed(program) {
            if let Some(path) = configured {
                return Ok(check(
                    CheckLevel::Error,
                    tr!(
                        "`{key}` apunta a {path}, que no existe",
                        "`{key}` points to {path}, which does not exist"
                    ),
                )
                .fix(format!("hedy config unset {key}")));
            }
            let used = accounts
                .iter()
                .any(|a| a.provider == provider && a.is_subscription());
            return Ok(if used {
                check(
                    CheckLevel::Error,
                    tr!(
                        "no está instalada y una de tus cuentas la usa",
                        "not installed, and one of your accounts uses it"
                    ),
                )
                .fix(tr!(
                    "hedy config set {key} <ruta>",
                    "hedy config set {key} <path>"
                ))
            } else {
                check(CheckLevel::Info, tr!("no está instalada", "not installed"))
            });
        }

        let spec = LaunchSpec::program(program, vec!["--version".into()]);
        let version = self
            .runner
            .run_captured_within(&spec, VERSION_TIMEOUT)
            .ok()
            .filter(|out| out.exit_code == Some(0))
            .and_then(|out| parse_version(&out.stdout));
        let Some(version) = version else {
            return Ok(check(
                CheckLevel::Warning,
                tr!(
                    "instalada, pero `{program} --version` no dijo su versión",
                    "installed, but `{program} --version` did not tell its version"
                ),
            ));
        };

        let seen_key = seen_version_key(provider);
        let seen = self.store.get_setting(&seen_key)?;
        if seen.as_deref() != Some(version.as_str()) {
            self.store.set_setting(&seen_key, Some(&version))?;
        }
        let verified = provider.verified_version();
        Ok(match (seen, verified) {
            (Some(old), Some(v)) if old != version && v != version => check(
                CheckLevel::Warning,
                tr!(
                    "{old} → {version}: cambió desde la última revisión. Hedy se verificó con {v}",
                    "{old} → {version}: changed since the last check. Hedy was verified with {v}"
                ),
            ),
            (Some(old), Some(_)) if old != version => check(
                CheckLevel::Info,
                tr!(
                    "{old} → {version}, la versión con la que se verificó Hedy",
                    "{old} → {version}, the version Hedy was verified with"
                ),
            ),
            (Some(old), None) if old != version => check(
                CheckLevel::Warning,
                tr!(
                    "{old} → {version}: cambió desde la última revisión",
                    "{old} → {version}: changed since the last check"
                ),
            ),
            (_, Some(v)) if v != version => check(
                CheckLevel::Info,
                tr!(
                    "{version} · Hedy se verificó con {v}",
                    "{version} · Hedy was verified with {v}"
                ),
            ),
            (_, Some(_)) => check(
                CheckLevel::Ok,
                tr!("{version} (verificada)", "{version} (verified)"),
            ),
            (_, None) => check(
                CheckLevel::Info,
                tr!(
                    "{version} · aún no verificada con Hedy",
                    "{version} · not verified with Hedy yet"
                ),
            ),
        })
    }

    fn check_account(&self, account: &Account, cli_installed: bool) -> AppResult<Vec<Check>> {
        let subject = format!("{} ({})", account.name, account.id);
        let check = |level, message: String| {
            Check::new(CheckArea::Account, subject.clone(), level, message)
        };
        let login_fix = format!("hedy account login {}", account.id);
        let cli = account.provider.cli_name();

        let folder = match &account.credentials {
            Credentials::ApiKey => {
                // Only whether a key exists: the key itself never reaches the report.
                return Ok(vec![match self.secrets.has_api_key(&account.id) {
                    Ok(true) => check(
                        CheckLevel::Ok,
                        tr!(
                            "API key en el llavero del sistema",
                            "API key in the system keyring"
                        ),
                    ),
                    Ok(false) => check(CheckLevel::Warning, tr!("no tiene API key", "no API key"))
                        .fix(login_fix),
                    Err(e) => check(
                        CheckLevel::Warning,
                        tr!(
                            "no se pudo leer el llavero del sistema: {e}",
                            "the system keyring could not be read: {e}"
                        ),
                    ),
                }]);
            }
            Credentials::Subscription(folder) => folder,
        };
        if let ConfigFolder::Managed(path) = folder
            && !self.fs.exists(path)
        {
            let path = path.display();
            return Ok(vec![
                check(
                    CheckLevel::Error,
                    tr!(
                        "su carpeta {path} no existe",
                        "its folder {path} does not exist"
                    ),
                )
                .fix(login_fix),
            ]);
        }
        if !cli_installed {
            return Ok(vec![check(
                CheckLevel::Warning,
                tr!(
                    "no se puede revisar la sesión: {cli} no está instalada",
                    "the login cannot be checked: {cli} is not installed"
                ),
            )]);
        }
        let has_status_command = LaunchSpec::for_account(account, LaunchPurpose::LoginStatus, None)
            .ok()
            .flatten()
            .is_some();
        let mut checks = vec![match self.login_state(account)? {
            LoginState::LoggedIn { plan: Some(plan) } => check(
                CheckLevel::Ok,
                tr!("sesión iniciada · {plan}", "signed in · {plan}"),
            ),
            LoginState::LoggedIn { plan: None } => {
                check(CheckLevel::Ok, tr!("sesión iniciada", "signed in"))
            }
            LoginState::LoggedOut => {
                check(CheckLevel::Warning, tr!("sin sesión", "signed out")).fix(login_fix)
            }
            LoginState::Unknown if has_status_command => check(
                CheckLevel::Warning,
                tr!(
                    "{cli} no respondió a su comando de sesión",
                    "{cli} did not answer its login status command"
                ),
            ),
            LoginState::Unknown => check(
                CheckLevel::Info,
                tr!(
                    "{cli} no tiene un comando oficial para revisar la sesión",
                    "{cli} has no official command to check the login"
                ),
            ),
        }];
        checks.extend(
            self.plan_usage_check(account)?
                .map(|(level, message)| check(level, message)),
        );
        Ok(checks)
    }

    /// Whether the plan usage could be read the last time Hedy asked, from
    /// the cache only: `doctor` starts no extra process for it.
    fn plan_usage_check(&self, account: &Account) -> AppResult<Option<(CheckLevel, String)>> {
        let state = self.plan_usage_of(account)?;
        if !state.supported || state.attempted_at.is_none() {
            return Ok(None);
        }
        Ok(Some(match state.problem {
            Some(UsageProblem::SpendsTokens) => (
                CheckLevel::Warning,
                tr!(
                    "esta versión gastaría tokens para leer el límite del plan; Hedy no lo pregunta",
                    "this version would spend tokens to read the plan limit; Hedy does not ask"
                ),
            ),
            Some(_) => (
                CheckLevel::Info,
                tr!(
                    "el límite del plan no se pudo leer la última vez",
                    "the plan limit could not be read last time"
                ),
            ),
            None => (
                CheckLevel::Ok,
                tr!(
                    "el límite del plan se puede leer",
                    "the plan limit can be read"
                ),
            ),
        }))
    }

    /// No accounts yet, or a default account that is gone or reserved.
    fn check_default_account(&self, accounts: &[Account]) -> AppResult<Vec<Check>> {
        let subject = tr!("Cuenta por defecto", "Default account");
        let check = |level, message: String| {
            Check::new(CheckArea::Account, subject.clone(), level, message)
        };
        if accounts.is_empty() {
            let name = tr!("Claude personal", "Personal Claude");
            return Ok(vec![
                Check::new(
                    CheckArea::Account,
                    tr!("Cuentas", "Accounts"),
                    CheckLevel::Info,
                    tr!("todavía no hay cuentas", "no accounts yet"),
                )
                .fix(format!("hedy account add \"{name}\" --claude")),
            ]);
        }
        let Some(id) = self.setting(Setting::DefaultAccount)? else {
            return Ok(vec![
                check(
                    CheckLevel::Info,
                    tr!(
                        "no hay; los proyectos sin cuenta no se pueden abrir",
                        "none; projects without an account cannot be opened"
                    ),
                )
                .fix(tr!(
                    "hedy account default <cuenta>",
                    "hedy account default <account>"
                )),
            ]);
        };
        let Some(account) = accounts.iter().find(|a| a.id.as_str() == id) else {
            return Ok(vec![
                check(
                    CheckLevel::Warning,
                    tr!("«{id}» ya no existe", "\"{id}\" no longer exists"),
                )
                .fix("hedy account default --clear"),
            ]);
        };
        let name = &account.name;
        Ok(vec![
            match policy::check_launch(None, &account.id, &self.projects()?) {
                Ok(()) => check(CheckLevel::Ok, name.clone()),
                Err(e) => check(CheckLevel::Warning, format!("{name}: {e}"))
                    .fix("hedy account default --clear"),
            },
        ])
    }

    fn check_hedy(&self) -> AppResult<Vec<Check>> {
        let hedy = |subject: String, level, message: String| {
            Check::new(CheckArea::Hedy, subject, level, message)
        };
        let mut checks = Vec::new();

        if let Some(dir) = self.store.data_dir() {
            let subject = tr!("Carpeta de datos", "Data folder");
            let shown = dir.display();
            checks.push(match self.fs.check_writable(&dir) {
                Ok(()) => hedy(subject, CheckLevel::Ok, shown.to_string()),
                Err(e) => hedy(
                    subject,
                    CheckLevel::Error,
                    tr!("no se puede escribir: {e}", "cannot be written: {e}"),
                ),
            });
        }

        let subject = tr!("Base de datos", "Database");
        checks.push(match self.store.integrity_problems() {
            Ok(problems) if problems.is_empty() => hedy(
                subject,
                CheckLevel::Ok,
                tr!("íntegra (quick_check)", "intact (quick_check)"),
            ),
            Ok(problems) => {
                let first = &problems[0];
                let more = problems.len() - 1;
                let message = if more == 0 {
                    tr!("dañada: {first}", "damaged: {first}")
                } else {
                    tr!(
                        "dañada: {first} (y {more} más)",
                        "damaged: {first} (and {more} more)"
                    )
                };
                hedy(subject, CheckLevel::Error, message)
            }
            Err(e) => hedy(
                subject,
                CheckLevel::Error,
                tr!("no se pudo revisar: {e}", "could not be checked: {e}"),
            ),
        });

        checks.push(if self.runner.is_installed("git") {
            hedy("git".into(), CheckLevel::Ok, tr!("instalado", "installed"))
        } else {
            hedy(
                "git".into(),
                CheckLevel::Error,
                tr!(
                    "no está instalado: las tareas trabajan en worktrees de git",
                    "not installed: tasks work in git worktrees"
                ),
            )
        });

        let subject = tr!("Memoria y hook de Claude", "Memory and Claude hook");
        checks.push(match &self.memory_server {
            // The app: its own binary serves both.
            Some(server) if self.runner.is_installed(&server.program) => {
                let program = &server.program;
                hedy(
                    subject,
                    CheckLevel::Ok,
                    tr!("los sirve {program}", "served by {program}"),
                )
            }
            Some(server) => {
                let program = &server.program;
                hedy(
                    subject,
                    CheckLevel::Error,
                    tr!("no se encuentra {program}", "{program} is not found"),
                )
            }
            // The `hedy` command runs no tasks: only the app needs them.
            None if self.runner.is_installed(APP_PROGRAM) => hedy(
                subject,
                CheckLevel::Ok,
                tr!(
                    "los sirve la app ({APP_PROGRAM}), que está instalada",
                    "served by the app ({APP_PROGRAM}), which is installed"
                ),
            ),
            None => hedy(
                subject,
                CheckLevel::Info,
                tr!(
                    "los sirve la app ({APP_PROGRAM}), que no está en el PATH; el comando `hedy` no los necesita",
                    "served by the app ({APP_PROGRAM}), which is not on PATH; the `hedy` command does not need them"
                ),
            ),
        });

        if let Some(program) = self
            .setting(Setting::CodeMap)?
            .filter(|p| !p.trim().is_empty())
        {
            let key = Setting::CodeMap.key();
            let subject = tr!("Mapa de código", "Code map");
            checks.push(if self.runner.is_installed(&program) {
                hedy(subject, CheckLevel::Ok, program)
            } else {
                hedy(
                    subject,
                    CheckLevel::Warning,
                    tr!("no se encuentra {program}", "{program} is not found"),
                )
                .fix(format!("hedy config unset {key}"))
            });
        }

        for project in self.projects()? {
            if !self.fs.exists(&project.path) {
                let path = project.path.display();
                checks.push(hedy(
                    project.name.clone(),
                    CheckLevel::Warning,
                    tr!(
                        "la carpeta del proyecto ya no existe: {path}",
                        "the project folder no longer exists: {path}"
                    ),
                ));
            }
        }
        Ok(checks)
    }

    /// Names only: a value may be a key. What a Claude Code session leaves
    /// behind goes on one line, so the variables that matter stand out.
    fn check_environment(&self) -> Vec<Check> {
        let (leftovers, names): (Vec<String>, Vec<String>) = self
            .runner
            .blocked_env_vars()
            .into_iter()
            .partition(|name| env::is_session_leftover(name));
        let mut checks: Vec<Check> = names
            .into_iter()
            .map(|name| {
                Check::new(
                    CheckArea::Environment,
                    name,
                    CheckLevel::Info,
                    tr!(
                        "está en tu shell y cambiaría la cuenta o el cobro; Hedy la quita al lanzar las CLIs",
                        "set in your shell and would change the account or the billing; Hedy removes it when launching CLIs"
                    ),
                )
            })
            .collect();
        if checks.is_empty() {
            checks.push(Check::new(
                CheckArea::Environment,
                tr!("Variables de entorno", "Environment variables"),
                CheckLevel::Ok,
                tr!(
                    "ninguna cambia la cuenta o la facturación",
                    "none changes the account or the billing"
                ),
            ));
        }
        if !leftovers.is_empty() {
            let n = leftovers.len();
            checks.push(Check::new(
                CheckArea::Environment,
                tr!("Sesión de Claude Code", "Claude Code session"),
                CheckLevel::Info,
                if n == 1 {
                    tr!(
                        "1 variable interna, inofensiva; Hedy la quita al lanzar las CLIs",
                        "1 internal variable, harmless; Hedy removes it when launching CLIs"
                    )
                } else {
                    tr!(
                        "{n} variables internas, inofensivas; Hedy las quita al lanzar las CLIs",
                        "{n} internal variables, harmless; Hedy removes them when launching CLIs"
                    )
                },
            ));
        }
        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::NewAccount;
    use crate::application::ports::{ProjectRepository, SettingsRepository};
    use crate::application::testing::Fixture;
    use crate::domain::{Project, Provider};

    fn find<'a>(report: &'a DoctorReport, subject: &str) -> &'a Check {
        report
            .checks
            .iter()
            .find(|c| c.subject == subject)
            .unwrap_or_else(|| panic!("no check for {subject}: {report:#?}"))
    }

    #[test]
    fn reports_whether_plan_usage_was_readable_from_the_cache() {
        let f = Fixture::new();
        f.add_claude("Personal");
        f.fs.add_existing("/data/accounts/personal");
        f.runner.install("claude");
        f.runner
            .reply(r#"{"loggedIn":true,"subscriptionType":"team"}"#, 0);
        let subject = "Personal (personal)";
        let messages = |f: &Fixture| -> Vec<(CheckLevel, String)> {
            let report = f.hedy.doctor().unwrap();
            report
                .checks
                .iter()
                .filter(|c| c.subject == subject)
                .map(|c| (c.level, c.message.clone()))
                .collect()
        };
        // Never asked: nothing to say, and doctor does not ask.
        assert_eq!(messages(&f).len(), 1);
        assert!(
            !f.runner
                .captured()
                .iter()
                .any(|s| s.args.contains(&"/usage".to_owned()))
        );

        f.runner.reply_to(
            "claude -p /usage --output-format json --no-session-persistence --safe-mode",
            r#"{"num_turns":0,"total_cost_usd":0,"result":"Current session: 3% used"}"#,
            0,
        );
        f.hedy.refresh_plan_usage("personal").unwrap();
        let checks = messages(&f);
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[1].0, CheckLevel::Ok);
    }

    #[test]
    fn missing_clis_are_info_unless_an_account_needs_them() {
        let f = Fixture::new();
        f.add_codex("ChatGPT");
        f.fs.add_existing("/data/accounts/chatgpt");
        let report = f.hedy.doctor().unwrap();
        assert_eq!(find(&report, "Gemini CLI").level, CheckLevel::Info);
        let codex = find(&report, "Codex");
        assert_eq!(codex.level, CheckLevel::Error);
        assert_eq!(
            codex.fix.as_deref(),
            Some("hedy config set binary.codex <ruta>")
        );
        assert!(report.has_errors());
        // The account cannot be checked without its CLI.
        assert_eq!(
            find(&report, "ChatGPT (chatgpt)").level,
            CheckLevel::Warning
        );
    }

    #[test]
    fn a_configured_binary_that_is_gone_is_an_error() {
        let f = Fixture::new();
        f.hedy
            .set_setting(Setting::Binary(Provider::Claude), Some("/opt/claude"))
            .unwrap();
        let report = f.hedy.doctor().unwrap();
        let claude = find(&report, "Claude Code");
        assert_eq!(claude.level, CheckLevel::Error);
        assert_eq!(
            claude.fix.as_deref(),
            Some("hedy config unset binary.claude")
        );
    }

    #[test]
    fn versions_are_remembered_and_changes_warned() {
        let f = Fixture::new();
        f.runner.install("claude");
        f.runner
            .reply_to("claude --version", "2.1.306 (Claude Code)\n", 0);
        let first = f.hedy.doctor().unwrap();
        assert_eq!(find(&first, "Claude Code").level, CheckLevel::Ok);
        assert_eq!(
            f.store
                .get_setting("doctor.cli_version.claude")
                .unwrap()
                .as_deref(),
            Some("2.1.306")
        );
        // Only `--version`, with no account folder.
        let probe = f.runner.captured().pop().unwrap();
        assert!(probe.env.is_empty());

        f.runner
            .reply_to("claude --version", "2.1.310 (Claude Code)\n", 0);
        let changed = f.hedy.doctor().unwrap();
        let claude = find(&changed, "Claude Code");
        assert_eq!(claude.level, CheckLevel::Warning);
        assert!(
            claude.message.contains("2.1.306 → 2.1.310"),
            "{}",
            claude.message
        );

        // Seen once: from now on only the verified version is noted.
        let again = f.hedy.doctor().unwrap();
        assert_eq!(find(&again, "Claude Code").level, CheckLevel::Info);
    }

    #[test]
    fn a_cli_that_does_not_tell_its_version_is_a_warning() {
        let f = Fixture::new();
        f.runner.install("kimi");
        f.runner.fail("kimi --version");
        f.runner.install("agy");
        f.runner.reply_to("agy --version", "agy 1.4.0", 0);
        let report = f.hedy.doctor().unwrap();
        assert_eq!(find(&report, "Kimi Code").level, CheckLevel::Warning);
        // Installed but never verified with Hedy.
        assert_eq!(find(&report, "Antigravity").level, CheckLevel::Info);
    }

    #[test]
    fn accounts_report_their_folder_and_login() {
        let f = Fixture::new();
        f.runner.install("claude");
        f.runner.install("codex");
        let claude = f.add_claude("Personal");
        f.add_codex("ChatGPT");
        f.fs.add_existing("/data/accounts/personal");
        f.runner.reply_to(
            "claude auth status --json",
            r#"{"loggedIn":true,"subscriptionType":"max"}"#,
            0,
        );
        let report = f.hedy.doctor().unwrap();
        let personal = find(&report, "Personal (personal)");
        assert_eq!(personal.level, CheckLevel::Ok);
        assert!(personal.message.contains("max"));
        // Codex's folder is missing.
        let chatgpt = find(&report, "ChatGPT (chatgpt)");
        assert_eq!(chatgpt.level, CheckLevel::Error);
        assert_eq!(chatgpt.fix.as_deref(), Some("hedy account login chatgpt"));

        f.runner
            .reply_to("claude auth status --json", r#"{"loggedIn":false}"#, 1);
        let report = f.hedy.doctor().unwrap();
        let personal = find(&report, "Personal (personal)");
        assert_eq!(personal.level, CheckLevel::Warning);
        assert_eq!(
            personal.fix.as_deref(),
            Some(format!("hedy account login {}", claude.id).as_str())
        );
    }

    #[test]
    fn clis_without_a_status_command_are_info() {
        let f = Fixture::new();
        f.runner.install("gemini");
        f.hedy
            .add_account(NewAccount {
                name: "Code Assist".into(),
                provider: Provider::Gemini,
                api_key: false,
                default_folder: true,
                color: None,
            })
            .unwrap();
        let report = f.hedy.doctor().unwrap();
        assert_eq!(
            find(&report, "Code Assist (code-assist)").level,
            CheckLevel::Info
        );
    }

    #[test]
    fn api_keys_are_only_checked_for_presence() {
        let f = Fixture::new();
        f.add_kimi("Kimi");
        let report = f.hedy.doctor().unwrap();
        assert_eq!(find(&report, "Kimi (kimi)").level, CheckLevel::Warning);

        f.hedy.set_api_key("kimi", "sk-secret-123").unwrap();
        let report = f.hedy.doctor().unwrap();
        assert_eq!(find(&report, "Kimi (kimi)").level, CheckLevel::Ok);
        let everything = format!("{report:?}");
        assert!(!everything.contains("sk-secret-123"));
    }

    #[test]
    fn default_account_sanity() {
        let f = Fixture::new();
        let report = f.hedy.doctor().unwrap();
        assert_eq!(find(&report, "Cuentas").level, CheckLevel::Info);

        f.add_claude("Personal");
        // An id left behind by an older version or a manual edit.
        f.store
            .set_setting("default_account", Some("ghost"))
            .unwrap();
        let report = f.hedy.doctor().unwrap();
        let default = find(&report, "Cuenta por defecto");
        assert_eq!(default.level, CheckLevel::Warning);
        assert_eq!(default.fix.as_deref(), Some("hedy account default --clear"));

        // Reserved by a protected project.
        f.store
            .set_setting("default_account", Some("personal"))
            .unwrap();
        let mut project = Project::new("/code/secret".into());
        project.account = Some(crate::domain::AccountId::new("personal").unwrap());
        project.protected = true;
        f.store.save_project(&project).unwrap();
        let report = f.hedy.doctor().unwrap();
        assert_eq!(
            find(&report, "Cuenta por defecto").level,
            CheckLevel::Warning
        );
        // Its folder is gone too.
        assert_eq!(find(&report, &project.name).level, CheckLevel::Warning);
    }

    #[test]
    fn hedy_checks_data_folder_database_git_and_helpers() {
        let f = Fixture::new();
        let report = f.hedy.doctor().unwrap();
        assert_eq!(find(&report, "Carpeta de datos").level, CheckLevel::Ok);
        assert_eq!(find(&report, "Base de datos").level, CheckLevel::Ok);
        assert_eq!(find(&report, "git").level, CheckLevel::Error);
        assert_eq!(
            find(&report, "Memoria y hook de Claude").level,
            CheckLevel::Info
        );

        f.fs.make_read_only("/data");
        f.store.corrupt("row 3 missing from index");
        f.runner.install("git");
        f.runner.install("hedy-app");
        f.hedy
            .set_setting(Setting::CodeMap, Some("codebase-memory-mcp"))
            .unwrap();
        let report = f.hedy.doctor().unwrap();
        assert_eq!(find(&report, "Carpeta de datos").level, CheckLevel::Error);
        let db = find(&report, "Base de datos");
        assert_eq!(db.level, CheckLevel::Error);
        assert!(db.message.contains("row 3"));
        assert_eq!(find(&report, "git").level, CheckLevel::Ok);
        assert_eq!(
            find(&report, "Memoria y hook de Claude").level,
            CheckLevel::Ok
        );
        assert_eq!(find(&report, "Mapa de código").level, CheckLevel::Warning);
    }

    #[test]
    fn the_app_checks_its_own_memory_server() {
        let f = Fixture::new();
        let app = f
            .hedy
            .clone()
            .with_memory_server(crate::application::MemoryServerCommand {
                program: "/usr/bin/hedy-app".into(),
                args: vec!["memory-mcp".into()],
            });
        let report = app.doctor().unwrap();
        assert_eq!(
            find(&report, "Memoria y hook de Claude").level,
            CheckLevel::Error
        );
        f.runner.install("/usr/bin/hedy-app");
        let report = app.doctor().unwrap();
        assert_eq!(
            find(&report, "Memoria y hook de Claude").level,
            CheckLevel::Ok
        );
    }

    #[test]
    fn only_names_of_blocked_variables_are_reported() {
        let f = Fixture::new();
        let report = f.hedy.doctor().unwrap();
        assert_eq!(find(&report, "Variables de entorno").level, CheckLevel::Ok);

        f.runner.set_env(&["ANTHROPIC_API_KEY", "CODEX_HOME"]);
        let report = f.hedy.doctor().unwrap();
        let env: Vec<_> = report.in_area(CheckArea::Environment).collect();
        assert_eq!(env.len(), 2);
        assert!(env.iter().all(|c| c.level == CheckLevel::Info));
        assert_eq!(env[0].subject, "ANTHROPIC_API_KEY");
    }

    #[test]
    fn claude_code_leftovers_share_one_line() {
        let f = Fixture::new();
        f.runner.set_env(&[
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_SESSION_ID",
        ]);
        let report = f.hedy.doctor().unwrap();
        let env: Vec<_> = report.in_area(CheckArea::Environment).collect();
        assert_eq!(env.len(), 2);
        assert_eq!(env[0].level, CheckLevel::Ok);
        assert_eq!(env[1].subject, "Sesión de Claude Code");
        assert!(env[1].message.starts_with("3 variables"));

        f.runner
            .set_env(&["CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CODE_ENTRYPOINT"]);
        let report = f.hedy.doctor().unwrap();
        let env: Vec<_> = report.in_area(CheckArea::Environment).collect();
        assert_eq!(env.len(), 2);
        assert_eq!(env[0].subject, "CLAUDE_CODE_OAUTH_TOKEN");
        assert!(env[1].message.starts_with("1 variable "));
    }

    #[test]
    fn checks_come_grouped_by_area() {
        let f = Fixture::new();
        f.add_claude("Personal");
        let report = f.hedy.doctor().unwrap();
        let areas: Vec<CheckArea> = report.checks.iter().map(|c| c.area).collect();
        let mut sorted = areas.clone();
        sorted.sort();
        assert_eq!(areas, sorted);
        assert_eq!(report.in_area(CheckArea::Cli).count(), Provider::ALL.len());
    }
}
