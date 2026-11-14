//! End-to-end tests of the `hedy` binary against a fake `claude` script.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const FAKE_CLAUDE: &str = r#"#!/bin/sh
if [ "$1" = --version ]; then
  echo "${FAKE_CLAUDE_VERSION:-2.1.306} (Claude Code)"
  exit 0
fi
if [ "$1" = auth ] && [ "$2" = status ]; then
  echo '{"loggedIn":true,"subscriptionType":"max","email":"secret@example.com"}'
  exit 0
fi
echo "CONFIG=${CLAUDE_CONFIG_DIR:-unset}"
echo "APIKEY=${ANTHROPIC_API_KEY:-none}"
echo "OAUTH=${CLAUDE_CODE_OAUTH_TOKEN:-none}"
echo "ARGS=$*"
echo "PWD=$(pwd)"
"#;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        let env = Self {
            tmp: tempfile::tempdir().unwrap(),
        };
        let fake = env.path("bin/claude");
        fs::create_dir_all(fake.parent().unwrap()).unwrap();
        fs::write(&fake, FAKE_CLAUDE).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
        env.hedy(&["config", "set", "binary.claude", fake.to_str().unwrap()])
            .assert()
            .success();
        env
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    fn repo(&self, rel: &str) -> PathBuf {
        let dir = self.path(rel);
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        dir.canonicalize().unwrap()
    }

    /// `hedy` in Spanish (the language most tests check).
    fn hedy(&self, args: &[&str]) -> Command {
        self.hedy_lang("es", args)
    }

    fn hedy_lang(&self, language: &str, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("hedy").unwrap();
        cmd.env("HEDY_DATA_DIR", self.path("data"))
            .env("NO_COLOR", "1")
            .env("HEDY_LANGUAGE", language)
            .env_remove("CLAUDE_CONFIG_DIR")
            .args(args);
        cmd
    }

    /// `hedy doctor` that only finds the fake `claude` and git: a `PATH` with
    /// nothing else and a `HOME` without `~/.local/bin`, so the CLIs on this
    /// computer are not run.
    fn doctor(&self, language: &str, args: &[&str]) -> Command {
        let bin = self.path("doctor-bin");
        if !bin.exists() {
            fs::create_dir_all(&bin).unwrap();
            let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|dir| dir.join("git"))
                .find(|p| p.is_file())
                .expect("git is installed");
            std::os::unix::fs::symlink(git, bin.join("git")).unwrap();
        }
        let mut cmd = self.hedy_lang(language, args);
        cmd.env("PATH", &bin).env("HOME", self.path("home"));
        cmd
    }

    fn hedy_in(&self, dir: &Path, args: &[&str]) -> Command {
        let mut cmd = self.hedy(&["-C", dir.to_str().unwrap()]);
        cmd.args(args);
        cmd
    }
}

#[test]
fn run_uses_the_project_account_folder_and_strips_credentials() {
    let env = Env::new();
    let repo = env.repo("code/hedy");
    env.hedy(&[
        "account",
        "add",
        "Claude personal",
        "--claude",
        "--no-login",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("claude-personal"));
    env.hedy_in(&repo.join("src"), &["use", "claude-personal"])
        .assert()
        .success();

    let folder = env.path("data/accounts/claude-personal");
    assert!(folder.is_dir());
    env.hedy_in(&repo.join("src"), &["run", "--", "--resume", "abc"])
        .env("ANTHROPIC_API_KEY", "sk-should-not-leak")
        .env("CLAUDE_CODE_OAUTH_TOKEN", "should-not-leak")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "CONFIG={}",
            folder.display()
        )))
        .stdout(predicate::str::contains("APIKEY=none"))
        .stdout(predicate::str::contains("OAUTH=none"))
        .stdout(predicate::str::contains("ARGS=--resume abc"))
        .stdout(predicate::str::contains(format!(
            "PWD={}",
            repo.join("src").display()
        )));
}

#[test]
fn default_folder_account_runs_without_the_variable() {
    let env = Env::new();
    env.hedy(&[
        "account",
        "add",
        "Claude trabajo",
        "--claude",
        "--default-folder",
        "--no-login",
    ])
    .assert()
    .success();
    // First account becomes the default, so any folder can run it.
    env.hedy_in(env.tmp.path(), &["run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CONFIG=unset"));
}

#[test]
fn without_accounts_status_and_list_explain_how_to_add_one() {
    let env = Env::new();
    for args in [&["status"][..], &["account", "list"]] {
        env.hedy(args)
            .assert()
            .success()
            .stdout(predicate::str::contains("Todavía no hay cuentas"))
            .stdout(predicate::str::contains("hedy account add"));
    }
}

#[test]
fn status_shows_login_without_identifying_data() {
    let env = Env::new();
    let repo = env.repo("code/homming");
    env.hedy(&[
        "cuenta",
        "agregar",
        "Claude trabajo",
        "--claude",
        "--no-login",
    ])
    .assert()
    .success();
    env.hedy_in(&repo, &["usar", "claude-trabajo"])
        .assert()
        .success();
    env.hedy_in(&repo, &["estado"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Proyecto homming"))
        .stdout(predicate::str::contains("conectada · max"))
        .stdout(predicate::str::contains("secret@example.com").not());
}

#[test]
fn protected_projects_keep_their_account() {
    let env = Env::new();
    let work = env.repo("code/homming");
    let hobby = env.repo("code/hobby");
    // The first account becomes the default, and the default cannot be protected.
    for name in ["Claude personal", "Claude trabajo"] {
        env.hedy(&["account", "add", name, "--claude", "--no-login"])
            .assert()
            .success();
    }
    env.hedy_in(&work, &["use", "claude-trabajo", "--protect"])
        .assert()
        .success()
        .stdout(predicate::str::contains("protegido"));

    env.hedy_in(&hobby, &["use", "claude-trabajo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reservada"));
    env.hedy_in(&work, &["run", "-a", "claude-personal"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("solo puede usar"));

    // Protection is opt-in and can be removed.
    env.hedy_in(&work, &["use", "claude-trabajo", "--unprotect"])
        .assert()
        .success();
    env.hedy_in(&hobby, &["use", "claude-trabajo"])
        .assert()
        .success();
}

#[test]
fn api_key_accounts_need_the_api_flag_and_cannot_run() {
    let env = Env::new();
    env.hedy(&["account", "add", "Kimi", "--api", "claude"])
        .assert()
        .failure();
    env.hedy(&["account", "add", "Kimi", "--api", "kimi", "--no-login"])
        .assert()
        .success();
    env.hedy_in(env.tmp.path(), &["run", "-a", "kimi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("API key"));
}

#[test]
fn completions_for_every_shell() {
    let env = Env::new();
    for shell in ["bash", "zsh", "fish"] {
        env.hedy(&["completions", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("hedy"));
    }
}

#[test]
fn client_and_summary() {
    let env = Env::new();
    let repo = env.repo("code/moira");
    env.hedy(&["-C", repo.to_str().unwrap(), "cliente", "León Blanco"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no es un proyecto"));
    env.hedy(&["account", "add", "Personal", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy(&["-C", repo.to_str().unwrap(), "use", "personal"])
        .assert()
        .success();
    env.hedy(&["-C", repo.to_str().unwrap(), "cliente", "León Blanco"])
        .assert()
        .success()
        .stdout(predicate::str::contains("moira es para León Blanco"));
    env.hedy(&["resumen", "--semana"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sin uso registrado esta semana."));
}

#[test]
fn speaks_english_when_asked() {
    let env = Env::new();
    let en = |args: &[&str]| env.hedy_lang("en", args).assert().success();
    en(&["account", "list"])
        .stdout(predicate::str::contains("No accounts yet"))
        .stdout(predicate::str::contains("\"Personal Claude\" --claude"));
    en(&[
        "account",
        "add",
        "Claude personal",
        "--claude",
        "--no-login",
    ])
    .stdout(predicate::str::contains("Account created"))
    .stdout(predicate::str::contains("Claude Code · subscription"))
    .stdout(predicate::str::contains("It is the default account"));
    en(&["account", "add", "Kimi", "--api", "kimi", "--no-login"])
        .stdout(predicate::str::contains(" · API key"));
    en(&["account", "list"])
        .stdout(predicate::str::contains("system keyring"))
        .stdout(predicate::str::contains(" · default"));
    let repo = env.repo("code/moira");
    en(&["-C", repo.to_str().unwrap(), "use", "claude-personal"])
        .stdout(predicate::str::contains("moira"))
        .stdout(predicate::str::contains(" uses "));
    en(&["-C", repo.to_str().unwrap(), "status"])
        .stdout(predicate::str::contains("Project moira"))
        .stdout(predicate::str::contains("signed in · max"))
        .stdout(predicate::str::contains("Accounts"));
    en(&["resumen", "--semana"]).stdout(predicate::str::contains("No usage recorded this week."));
    en(&["summary", "--days", "3"]).stdout(predicate::str::contains(
        "No usage recorded in the last 3 days.",
    ));
}

#[test]
fn the_language_setting_wins_over_the_environment() {
    let env = Env::new();
    env.hedy(&["config", "set", "ui.language", "en"])
        .assert()
        .success();
    env.hedy(&["account", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No accounts yet"));
}

#[test]
fn help_follows_the_language() {
    let env = Env::new();
    for args in [&["--help"][..], &["account", "add", "--help"]] {
        env.hedy_lang("en", args)
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage: hedy"))
            .stdout(predicate::str::contains("Print help"))
            .stdout(predicate::str::contains("Options:"))
            .stdout(predicate::str::contains("Opciones").not())
            .stdout(predicate::str::contains("Muestra").not());
        env.hedy_lang("es", args)
            .assert()
            .success()
            .stdout(predicate::str::contains("Uso: hedy"))
            .stdout(predicate::str::contains("[OPCIONES]"))
            .stdout(predicate::str::contains("Muestra la ayuda"))
            .stdout(predicate::str::contains("Usage").not())
            .stdout(predicate::str::contains("Print").not());
    }
    env.hedy_lang("en", &["--help"])
        .assert()
        .stdout(predicate::str::contains("Manage accounts [alias: cuenta]"));
    env.hedy_lang("es", &["--help"])
        .assert()
        .stdout(predicate::str::contains("Gestiona cuentas [alias: cuenta]"));
    env.hedy_lang("en", &["account", "add", "--help"])
        .assert()
        .stdout(predicate::str::contains("<NAME>  Display name"));
    env.hedy_lang("es", &["cuenta", "agregar", "--help"])
        .assert()
        .stdout(predicate::str::contains("<NOMBRE>  Nombre visible"));
}

#[test]
fn doctor_checks_clis_accounts_and_the_environment() {
    let env = Env::new();
    env.hedy(&[
        "account",
        "add",
        "Claude trabajo",
        "--claude",
        "--default-folder",
        "--no-login",
    ])
    .assert()
    .success();
    env.doctor("es", &["diagnostico"])
        .env("ANTHROPIC_API_KEY", "sk-should-not-leak")
        .assert()
        .success()
        .stdout(predicate::str::contains("CLIs oficiales"))
        .stdout(predicate::str::contains("✔ Claude Code"))
        .stdout(predicate::str::contains("2.1.306 (verificada)"))
        .stdout(predicate::str::contains("i Codex"))
        .stdout(predicate::str::contains("no está instalada"))
        .stdout(predicate::str::contains("sesión iniciada · max"))
        .stdout(predicate::str::contains("✔ git"))
        .stdout(predicate::str::contains("i ANTHROPIC_API_KEY"))
        .stdout(predicate::str::contains("Todo en orden"))
        .stdout(predicate::str::contains("sk-should-not-leak").not())
        .stdout(predicate::str::contains("secret@example.com").not());

    // A new version since the last check is a warning, but not an error.
    env.doctor("es", &["doctor"])
        .env("FAKE_CLAUDE_VERSION", "2.1.310")
        .assert()
        .success()
        .stdout(predicate::str::contains("! Claude Code"))
        .stdout(predicate::str::contains(
            "2.1.306 → 2.1.310: cambió desde la última revisión. Hedy se verificó con 2.1.306",
        ))
        .stdout(predicate::str::contains("1 aviso"));
}

#[test]
fn doctor_fails_when_something_is_broken() {
    let env = Env::new();
    env.hedy(&[
        "account",
        "add",
        "Claude personal",
        "--claude",
        "--no-login",
    ])
    .assert()
    .success();
    fs::remove_dir_all(env.path("data/accounts/claude-personal")).unwrap();
    env.doctor("es", &["doctor"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("✗ Claude personal"))
        .stdout(predicate::str::contains("no existe"))
        .stdout(predicate::str::contains(
            "→ hedy account login claude-personal",
        ))
        .stdout(predicate::str::contains("1 error"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn doctor_speaks_english() {
    let env = Env::new();
    env.hedy(&[
        "account",
        "add",
        "Work",
        "--claude",
        "--default-folder",
        "--no-login",
    ])
    .assert()
    .success();
    env.doctor("en", &["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Official CLIs"))
        .stdout(predicate::str::contains("2.1.306 (verified)"))
        .stdout(predicate::str::contains("signed in · max"))
        .stdout(predicate::str::contains("Environment"))
        .stdout(predicate::str::contains("All good"))
        .stdout(predicate::str::contains("verificada").not())
        .stdout(predicate::str::contains("Cuentas").not());
}
