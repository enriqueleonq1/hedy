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
    env.hedy(&["tareas"])
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
        .hedy(&["tareas"])
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
