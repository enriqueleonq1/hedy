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
if [ "$1" = -p ] && [ "$2" = /usage ] && [ "$3" = --output-format ] && [ "$4" = json ]; then
  [ -n "$ANTHROPIC_API_KEY" ] && { echo '{"num_turns":1,"total_cost_usd":0.5}'; exit 0; }
  echo '{"type":"result","subtype":"success","is_error":false,"num_turns":0,"total_cost_usd":0,"local_command":"usage","result":"You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 34% used · resets Nov 14, 7pm (America/Caracas)\nCurrent week (all models): 29% used · resets Nov 20, 12am (America/Caracas)\nCurrent week (Fable): 0% used · resets Nov 20, 12am (America/Caracas)\n\nWhat is contributing to your limits usage?\n  98%  Give subagents a cheaper model\n"}'
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
fn renaming_an_account_keeps_its_id() {
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
    env.hedy(&["account", "rename", "claude-personal", "Claude casa"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cuenta renombrada"))
        .stdout(predicate::str::contains("Claude casa (claude-personal)"));
    env.hedy_lang("en", &["account", "rename", "claude-personal", "Home"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Account renamed"));
    env.hedy(&["account", "rename", "claude-personal", "  "])
        .assert()
        .failure();
    env.hedy(&["account", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Home"));
}

#[test]
fn status_shows_login_without_identifying_data() {
    let env = Env::new();
    let repo = env.repo("code/homming");
    env.hedy(&["account", "add", "Claude trabajo", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy_in(&repo, &["use", "claude-trabajo"])
        .assert()
        .success();
    env.hedy_in(&repo, &["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Proyecto homming"))
        .stdout(predicate::str::contains("conectada · max"))
        .stdout(predicate::str::contains("secret@example.com").not());
}

#[test]
fn status_shows_plan_usage_from_claude_usage() {
    let env = Env::new();
    env.hedy(&["account", "add", "Claude", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy(&["account", "add", "Kimi Code", "--kimi", "--no-login"])
        .assert()
        .success();
    let line = |text: &str, needle: &str| {
        text.lines()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no line with {needle} in:\n{text}"))
            .to_owned()
    };
    // The fake answers as if it spent tokens when an API key gets through.
    let out = env
        .hedy(&["status"])
        .env("ANTHROPIC_API_KEY", "sk-should-be-stripped")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    assert!(line(&out, "Sesión (5 h)").contains("34%"), "{out}");
    assert!(line(&out, "Esta semana").contains("29%"), "{out}");
    assert!(line(&out, "Esta semana").contains("se renueva"), "{out}");
    assert!(
        out.contains("según /usage de Claude Code · hace un momento"),
        "{out}"
    );
    // A per-model window at 0 % is not worth a line.
    assert!(!out.contains("Fable"), "{out}");
    assert!(
        out.contains("Kimi aún no informa el límite del plan a Hedy"),
        "{out}"
    );

    // The cached reading is shown again (no new ask within 3 minutes), in English too.
    env.hedy_lang("en", &["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Session (5 h)"))
        .stdout(predicate::str::contains("This week"))
        .stdout(predicate::str::contains("from Claude Code's /usage"))
        .stdout(predicate::str::contains(
            "Kimi does not report its plan limit to Hedy yet",
        ));
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
    env.hedy(&["-C", repo.to_str().unwrap(), "client", "León Blanco"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no es un proyecto"));
    env.hedy(&["account", "add", "Personal", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy(&["-C", repo.to_str().unwrap(), "use", "personal"])
        .assert()
        .success();
    env.hedy(&["-C", repo.to_str().unwrap(), "client", "León Blanco"])
        .assert()
        .success()
        .stdout(predicate::str::contains("moira es para León Blanco"));
    env.hedy(&["summary", "--week"])
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
    en(&["summary", "--week"]).stdout(predicate::str::contains("No usage recorded this week."));
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
        .stdout(predicate::str::contains("Manage accounts"))
        .stdout(predicate::str::contains("alias").not());
    env.hedy_lang("es", &["--help"])
        .assert()
        .stdout(predicate::str::contains("Gestiona cuentas"))
        .stdout(predicate::str::contains("alias").not());
    env.hedy_lang("en", &["account", "add", "--help"])
        .assert()
        .stdout(predicate::str::contains("<NAME>  Display name"));
    env.hedy_lang("es", &["account", "add", "--help"])
        .assert()
        .stdout(predicate::str::contains("<NOMBRE>  Nombre visible"));
    env.hedy_lang("en", &["memory", "--help"])
        .assert()
        .stdout(predicate::str::contains("Project memory"))
        .stdout(predicate::str::contains("approve"));
    env.hedy_lang("es", &["memory", "--help"])
        .assert()
        .stdout(predicate::str::contains("Memoria del proyecto"))
        .stdout(predicate::str::contains("Rechaza una propuesta"));
}

/// Notes as the app and the agents leave them, written straight to the database.
fn seed_memory(env: &Env, repo: &Path) -> (i64, i64, i64) {
    use hedy_core::adapters::{self, HedyPaths};
    use hedy_core::domain::Category;
    let hedy = adapters::system(&HedyPaths::at(env.path("data"))).unwrap();
    let topic = hedy
        .propose_note(
            repo,
            "Los recordatorios se envían por WhatsApp",
            Category::Decision,
            "Claude",
            Some("recordatorios/canal"),
        )
        .unwrap();
    let old = hedy.approve_note(topic.id).unwrap();
    hedy.add_note(
        Some(repo),
        "Los precios se guardan en USD y se muestran también en Bs",
        Category::Decision,
    )
    .unwrap();
    let change = hedy
        .propose_note(
            repo,
            "Los recordatorios se envían por WhatsApp y, si falla, por SMS",
            Category::Decision,
            "Codex",
            Some("recordatorios/canal"),
        )
        .unwrap();
    let similar = hedy
        .propose_note(
            repo,
            "Los precios se guardan en USD",
            Category::Decision,
            "Codex",
            None,
        )
        .unwrap();
    (old.id, change.id, similar.id)
}

#[test]
fn memory_lists_approves_and_rejects_proposals() {
    let env = Env::new();
    let repo = env.repo("code/moira");
    env.hedy(&["account", "add", "Claude", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy_in(&repo, &["use", "claude"]).assert().success();
    env.hedy_in(&repo, &["memory"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Todavía no hay notas"));

    let (old, change, similar) = seed_memory(&env, &repo);
    env.hedy_in(&repo, &["memory", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Propuestas · para aprobar"))
        .stdout(predicate::str::contains(format!("#{change}")))
        .stdout(predicate::str::contains("recordatorios/canal · Codex"))
        .stdout(predicate::str::contains(format!(
            "cambia #{old}, que dice:"
        )))
        .stdout(predicate::str::contains("parecida a #"))
        .stdout(predicate::str::contains("Aprobadas"))
        .stdout(predicate::str::contains("hedy memory approve <id>"));
    env.hedy_lang("en", &["-C", repo.to_str().unwrap(), "memory", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Proposals · awaiting approval"))
        .stdout(predicate::str::contains(format!(
            "changes #{old}, which says:"
        )))
        .stdout(predicate::str::contains("similar to #"));

    // Rejecting changes nothing; approving a change updates the old note.
    env.hedy(&["memory", "reject", &similar.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("rechazada"));
    env.hedy(&["memory", "approve", &change.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Nota #{old} actualizada")));
    env.hedy_in(&repo, &["memory"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Propuestas").not())
        .stdout(predicate::str::contains("si falla, por SMS"));

    env.hedy(&["memory", "reject", &old.to_string()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no es una propuesta"));
    env.hedy_lang("en", &["memory", "approve", "999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("note 999 does not exist"));
}

/// A real git repository (skill files are excluded through `.git/info/exclude`).
fn git_repo(env: &Env, rel: &str) -> PathBuf {
    let dir = env.path(rel);
    fs::create_dir_all(&dir).unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&dir)
        .status()
        .unwrap();
    assert!(init.success());
    dir.canonicalize().unwrap()
}

/// Skills as agents leave them, written straight to the database: a new
/// proposal and an approved skill with a proposed update.
fn seed_skills(env: &Env, repo: &Path) -> (i64, i64) {
    use hedy_core::adapters::{self, HedyPaths};
    let hedy = adapters::system(&HedyPaths::at(env.path("data"))).unwrap();
    let tests = hedy
        .propose_skill(
            repo,
            "run-e2e-tests",
            "Run the end-to-end tests before a release.",
            "1. npm run build\n2. npm run test:e2e",
            "Claude",
            None,
        )
        .unwrap();
    hedy.approve_skill(tests.id).unwrap();
    let update = hedy
        .propose_skill(
            repo,
            "run-e2e-tests",
            "Run the end-to-end tests, headless.",
            "1. npm run test:e2e -- --headless",
            "Codex",
            None,
        )
        .unwrap();
    let lint = hedy
        .propose_skill(
            repo,
            "lint",
            "Lint before committing.",
            "1. npm run lint",
            "Codex",
            None,
        )
        .unwrap();
    (update.id, lint.id)
}

#[test]
fn skills_list_show_approve_reject_and_remove() {
    let env = Env::new();
    let repo = git_repo(&env, "code/moira");
    env.hedy(&["account", "add", "Claude", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy_in(&repo, &["use", "claude"]).assert().success();
    env.hedy_in(&repo, &["skills"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Todavía no hay habilidades"));

    let (update, lint) = seed_skills(&env, &repo);
    let skill_file = repo.join(".claude/skills/run-e2e-tests/SKILL.md");
    assert!(
        fs::read_to_string(&skill_file)
            .unwrap()
            .contains("npm run build")
    );
    let exclude = fs::read_to_string(repo.join(".git/info/exclude")).unwrap();
    assert!(exclude.contains("/.claude/skills/run-e2e-tests/"));
    assert!(exclude.contains("/.agents/skills/run-e2e-tests/"));

    env.hedy_in(&repo, &["skills", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Propuestas · para aprobar"))
        .stdout(predicate::str::contains(format!("#{lint}")))
        .stdout(predicate::str::contains("hedy skills show run-e2e-tests"))
        .stdout(predicate::str::contains("Aprobadas"))
        .stdout(predicate::str::contains("hedy skills approve <id>"));
    env.hedy_in(&repo, &["skills", "show", "run-e2e-tests"])
        .assert()
        .success()
        .stdout(predicate::str::contains("npm run build"))
        .stdout(predicate::str::contains(format!(
            "Actualización propuesta #{update} · Codex"
        )))
        .stdout(predicate::str::contains("--headless"));

    env.hedy(&["skills", "reject", &lint.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("rechazada"));
    env.hedy(&["skills", "approve", &update.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Habilidad run-e2e-tests actualizada",
        ));
    assert!(
        fs::read_to_string(&skill_file)
            .unwrap()
            .contains("--headless")
    );

    // A skill of the user's own with the same name is left alone.
    let mine = repo.join(".agents/skills/run-e2e-tests/SKILL.md");
    fs::write(&mine, "---\nname: run-e2e-tests\n---\nMine").unwrap();
    env.hedy_in(&repo, &["skills", "remove", "run-e2e-tests"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Habilidad run-e2e-tests borrada"));
    assert!(!skill_file.exists() && !repo.join(".claude").exists());
    assert_eq!(
        fs::read_to_string(&mine).unwrap(),
        "---\nname: run-e2e-tests\n---\nMine"
    );
    env.hedy_in(&repo, &["skills", "show", "run-e2e-tests"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no existe la habilidad «run-e2e-tests»",
        ));
}

#[test]
fn skills_speak_english() {
    let env = Env::new();
    let repo = git_repo(&env, "code/moira");
    env.hedy(&["account", "add", "Claude", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy_in(&repo, &["use", "claude"]).assert().success();
    let dir = repo.to_str().unwrap();
    env.hedy_lang("en", &["-C", dir, "skills"])
        .assert()
        .success()
        .stdout(predicate::str::contains("has no skills yet"));
    let (update, lint) = seed_skills(&env, &repo);
    env.hedy_lang("en", &["-C", dir, "skills", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Proposals · awaiting approval"))
        .stdout(predicate::str::contains("updates #"))
        .stdout(predicate::str::contains("Approved"));
    env.hedy_lang("en", &["skills", "approve", &lint.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skill lint approved"))
        .stdout(predicate::str::contains("wrote "));
    env.hedy_lang("en", &["skills", "reject", &update.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("the skills did not change"));
    env.hedy_lang("en", &["skills", "reject", "999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("there is no skill «999»"));
    env.hedy_lang("en", &["skills", "--help"])
        .assert()
        .stdout(predicate::str::contains("Learned skills"));
}

#[test]
fn templates_are_added_listed_and_removed_per_scope() {
    let env = Env::new();
    let repo = env.repo("code/moira");
    env.hedy(&["account", "add", "Claude", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy_in(&repo, &["use", "claude"]).assert().success();

    env.hedy(&["template"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Para todos los proyectos"))
        .stdout(predicate::str::contains(
            "Escribir pruebas  ejemplo · {archivo}",
        ))
        .stdout(predicate::str::contains("escribe / al empezar"));
    env.hedy_in(
        &repo,
        &[
            "template",
            "add",
            "Migración",
            "--project",
            "--text",
            "Escribe la migración de {tabla}",
        ],
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("guardada para moira"))
    .stdout(predicate::str::contains("te pedirá: {tabla}"));
    env.hedy(&[
        "template",
        "add",
        "Desplegar",
        "--text",
        "Despliega a {entorno}",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("para todos los proyectos"));
    env.hedy(&["template", "add", "desplegar", "--text", "otra"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ya hay una plantilla"));

    env.hedy_in(&repo, &["template", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Del proyecto moira"))
        .stdout(predicate::str::contains("Migración  {tabla}"))
        .stdout(predicate::str::contains("Desplegar"));
    // Outside the project only the global ones show.
    env.hedy(&["template", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Migración").not());

    // Without --project, remove looks for a global template.
    env.hedy_in(&repo, &["template", "remove", "Migración"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no existe la plantilla"));
    env.hedy_in(&repo, &["template", "remove", "migración", "--project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("borrada"));
}

#[test]
fn templates_speak_english() {
    let env = Env::new();
    env.hedy_lang(
        "en",
        &["template", "add", "Deploy", "--text", "Deploy to {target}"],
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("saved for every project"))
    .stdout(predicate::str::contains("ask you for: {target}"));
    env.hedy_lang("en", &["template", "remove", "Write tests"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Template Write tests deleted."));
    env.hedy_lang("en", &["template"])
        .assert()
        .success()
        .stdout(predicate::str::contains("For every project"))
        .stdout(predicate::str::contains("Explain code  example · {file}"))
        .stdout(predicate::str::contains("Write tests").not())
        .stdout(predicate::str::contains("Para todos").not());
    env.hedy_lang("en", &["template", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Only for the current project"));
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
    env.doctor("es", &["doctor"])
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

/// Saves tasks the way the app does, with the time of their events.
fn seed_tasks(env: &Env, repo: &Path) {
    use hedy_core::adapters::{HedyPaths, SqliteStore};
    use hedy_core::application::ports::TaskRepository;
    use hedy_core::domain::{
        AccountId, AgentEvent, LimitLevel, Phase, Stage, Task, TaskId, TaskMode, TaskStatus,
    };

    let store = SqliteStore::open(&HedyPaths::at(env.path("data")).database()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let task = |id: &str, title: &str, status: TaskStatus, ago: i64| Task {
        images: Vec::new(),
        id: TaskId::new(id),
        project: repo.to_owned(),
        mode: TaskMode::Project,
        title: title.into(),
        prompt: title.into(),
        account: AccountId::new("personal").unwrap(),
        model: Some("opus".into()),
        workspace: None,
        session_id: None,
        status,
        created_at: now - ago,
        stages: vec![],
        stage: 0,
        plan: None,
        merge: None,
        wait: None,
        conversation: false,
        ship: None,
    };
    let mut plan = task(
        "t1",
        "Agrega recordatorios",
        TaskStatus::WaitingApproval,
        600,
    );
    plan.stages = vec![Stage {
        phase: Phase::Plan,
        account: plan.account.clone(),
        model: plan.model.clone(),
    }];
    let working = task("t2", "Arregla el carrito", TaskStatus::Running, 120);
    let limited = task("t3", "Exporta el reporte", TaskStatus::Running, 900);
    let failed = task("t4", "Migra los tests", TaskStatus::Failed, 7_200);
    let old = task(
        "t5",
        "Tarea de la semana pasada",
        TaskStatus::Accepted,
        8 * 86_400,
    );
    for t in [&plan, &working, &limited, &failed, &old] {
        store.save_task(t).unwrap();
    }
    let event = |id: &TaskId, ago: i64, event: AgentEvent| {
        store.append_event(id, now - ago, &event).unwrap();
    };
    event(
        &plan.id,
        500,
        AgentEvent::Plan {
            text: "1. Leer .env y copiar la clave".into(),
            request_id: Some("p1".into()),
        },
    );
    event(
        &limited.id,
        800,
        AgentEvent::LimitWarning {
            account: "personal".into(),
            level: LimitLevel::Reached,
            percent: Some(100.0),
            resets_at: None,
        },
    );
    event(
        &failed.id,
        3_600,
        AgentEvent::Error {
            message: "boom".into(),
        },
    );
    event(
        &old.id,
        8 * 86_400,
        AgentEvent::Message {
            text: "listo".into(),
        },
    );
}

#[test]
fn tasks_lists_what_needs_you_first() {
    let env = Env::new();
    env.hedy(&["tasks"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No hay tareas en curso"));

    let repo = env.repo("code/tienda-online");
    env.hedy(&["account", "add", "Personal", "--claude", "--no-login"])
        .assert()
        .success();
    env.hedy_in(&repo, &["use", "personal"]).assert().success();
    seed_tasks(&env, &repo);

    let output = env
        .hedy(&["tasks"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(output).unwrap();
    let position = |text: &str| {
        out.find(text)
            .unwrap_or_else(|| panic!("{text:?} missing in:\n{out}"))
    };
    assert!(position("Te necesitan (2)") < position("Agrega recordatorios"));
    assert!(position("Agrega recordatorios") < position("Exporta el reporte"));
    assert!(position("Exporta el reporte") < position("En curso (1)"));
    assert!(position("En curso (1)") < position("Arregla el carrito"));
    assert!(position("Últimas 24 h (1)") < position("Migra los tests"));
    position("plan por aprobar");
    position("en su límite");
    position("tienda-online · ● Personal · Claude Opus · plan · 10 min en marcha");
    position("falló");
    position("→ Respóndeles desde la app de Hedy");
    // Nothing from the plan or the history, and nothing older than a day.
    assert!(!out.contains(".env") && !out.contains("boom"));
    assert!(!out.contains("semana pasada"));

    env.hedy_lang("en", &["tasks"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Need you (2)"))
        .stdout(predicate::str::contains("plan to approve"))
        .stdout(predicate::str::contains("at its limit"))
        .stdout(predicate::str::contains("Last 24 h (1)"))
        .stdout(predicate::str::contains("running for 10 min"))
        .stdout(predicate::str::contains("Answer them in the Hedy app"))
        .stdout(predicate::str::contains("Te necesitan").not());
}

#[test]
fn tasks_without_tasks_in_english() {
    let env = Env::new();
    env.hedy_lang("en", &["tasks"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No running tasks, and none in the last 24 h.",
        ));
}

/// `hedy` with its own git identity and a `HOME` inside the test, so `hedy
/// new` neither depends on nor touches this computer's settings.
fn hedy_new(env: &Env, language: &str, args: &[&str]) -> Command {
    let config = env.path("gitconfig");
    if !config.exists() {
        fs::write(
            &config,
            "[user]\n\tname = Test\n\temail = test@hedy.local\n[commit]\n\tgpgsign = false\n",
        )
        .unwrap();
    }
    let mut cmd = env.hedy_lang(language, args);
    cmd.env("GIT_CONFIG_GLOBAL", &config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", env.path("home"));
    cmd
}

#[test]
fn new_creates_a_repository_and_registers_the_project() {
    let env = Env::new();
    env.hedy(&["account", "add", "Personal", "--claude", "--no-login"])
        .assert()
        .success();
    let parent = env.path("Proyectos");
    hedy_new(
        &env,
        "es",
        &[
            "new",
            "Mi tienda",
            "--in",
            parent.to_str().unwrap(),
            "-a",
            "personal",
        ],
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("Mi tienda creado en"))
    .stdout(predicate::str::contains("usa"));

    let project = parent.join("mi-tienda");
    assert!(project.join(".git").is_dir());
    let log = std::process::Command::new("git")
        .arg("-C")
        .arg(&project)
        .args(["log", "--format=%s"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).trim(),
        "Initial commit"
    );
    // Only git's folder: Hedy writes no files the user did not ask for.
    assert_eq!(fs::read_dir(&project).unwrap().count(), 1);

    env.hedy_in(&project, &["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Mi tienda"));

    // An existing folder is never reused.
    hedy_new(
        &env,
        "en",
        &["new", "Mi tienda", "--in", parent.to_str().unwrap()],
    )
    .assert()
    .failure()
    .stderr(predicate::str::contains("already exists"));
}

#[test]
fn new_without_a_folder_goes_to_the_usual_place_and_has_help_in_english() {
    let env = Env::new();
    hedy_new(&env, "en", &["new", "Notes app"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Notes app created in"))
        .stdout(predicate::str::contains("no account yet"));
    assert!(env.path("home/Projects/notes-app/.git").is_dir());

    hedy_new(&env, "en", &["new", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Start a project from scratch"))
        .stdout(predicate::str::contains("--in <FOLDER>"));
    hedy_new(&env, "es", &["new", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Empieza un proyecto desde cero"));
}
