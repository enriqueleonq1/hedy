//! End-to-end tests of the `hedy` binary against a fake `claude` script.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const FAKE_CLAUDE: &str = r#"#!/bin/sh
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

    fn hedy(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("hedy").unwrap();
        cmd.env("HEDY_DATA_DIR", self.path("data"))
            .env("NO_COLOR", "1")
            .env_remove("CLAUDE_CONFIG_DIR")
            .args(args);
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
