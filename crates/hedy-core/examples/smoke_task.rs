//! End-to-end check against the real Claude Code CLI (spends a few tokens).
//!
//! Creates a throwaway git repo and Hedy data folder, adopts the Claude account
//! of the default folder (`~/.claude`), runs a tiny task with plan approval and
//! permission answers, then accepts it and checks the merge.
//!
//! ```fish
//! cargo run -p hedy-core --example smoke_task -- [model]
//! cargo run -p hedy-core --example smoke_task -- haiku env   # blocked .env must stay unread
//! cargo run -p hedy-core --example smoke_task -- haiku bash  # not even via `cat .env`
//! cargo run -p hedy-core --example smoke_task -- haiku free  # an empty list: .env is read
//! cargo run -p hedy-core --example smoke_task -- haiku doc   # a question's document, outside
//! cargo run -p hedy-core --example smoke_task -- haiku auto  # automatic mode: no card at all
//! cargo run -p hedy-core --example smoke_task -- haiku flow  # the classic flow, role by role
//! ```
//!
//! `HEDY_SMOKE_ACCOUNT=<folder>` uses that Claude account folder (one already
//! logged in) instead of `~/.claude`, as `scripts/ask-eval.py` does.

use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use hedy_core::adapters::{self, HedyPaths, hook};
use hedy_core::application::ports::TaskEvents;
use hedy_core::application::{NewAccount, NewTask};
use hedy_core::domain::{
    AgentEvent, Decision, PermissionMode, Provider, Task, TaskId, TaskMode, TaskStatus,
};

struct Forward(Mutex<mpsc::Sender<AgentEvent>>);

impl TaskEvents for Forward {
    fn event(&self, _: &TaskId, event: &AgentEvent) {
        let _ = self.0.lock().unwrap().send(event.clone());
    }
    fn task_updated(&self, task: &Task) {
        println!("  · estado: {:?}", task.status);
    }
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Claude Code calls this same program as its PreToolUse hook.
    if std::env::args().skip(1).eq(hook::HOOK_ARGS) {
        std::process::exit(hook::run());
    }
    let model = std::env::args().nth(1).unwrap_or_else(|| "haiku".into());
    let check = std::env::args().nth(2);
    let env_check = matches!(check.as_deref(), Some("env" | "bash" | "free"));
    let env_free = check.as_deref() == Some("free");
    let doc_check = check.as_deref() == Some("doc");
    let auto_check = check.as_deref() == Some("auto");
    let flow_check = check.as_deref() == Some("flow");
    let tmp = std::env::temp_dir().join(format!("hedy-smoke-{}", std::process::id()));
    let repo = tmp.join("demo");
    std::fs::create_dir_all(&repo)?;
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "# Demo\n")?;
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Hedy",
            "-c",
            "user.email=hedy@local",
            "commit",
            "-qm",
            "Inicio",
        ],
    );
    std::fs::write(repo.join(".env"), "SECRETO=hedy-42\n")?;
    git(&repo, &["config", "user.name", "Hedy smoke"]);
    git(&repo, &["config", "user.email", "hedy@local"]);

    let hook_log = tmp.join("hook.log");
    // SAFETY: single-threaded at this point; read by the hook processes.
    unsafe { std::env::set_var("HEDY_HOOK_LOG", &hook_log) };
    let (tx, rx) = mpsc::channel();
    let hedy =
        adapters::system_with_hook(&HedyPaths::at(tmp.join("data")), &std::env::current_exe()?)?
            .with_task_events(Arc::new(Forward(Mutex::new(tx))));
    let own_folder = std::env::var_os("HEDY_SMOKE_ACCOUNT").map(std::path::PathBuf::from);
    hedy.add_account(NewAccount {
        name: "Claude".into(),
        provider: Provider::Claude,
        api_key: false,
        default_folder: own_folder.is_none(),
        color: None,
    })?;
    if let Some(folder) = &own_folder {
        // Hedy's folder for the account, pointing at one already logged in.
        let link = tmp.join("data/accounts/claude");
        let _ = std::fs::remove_dir_all(&link);
        std::os::unix::fs::symlink(folder, &link)?;
    }
    hedy.use_account(&repo, "claude", None)?;
    // Free by default, as in Claude Code; `env` and `bash` keep it from the AI.
    if env_check && !env_free {
        hedy.set_project_blocked_files(&repo, &[".env".into()])?;
    }

    let task = if flow_check {
        hedy.start_flow(
            NewTask {
                project: repo.clone(),
                prompt: "Crea un archivo hola.txt cuyo contenido sea exactamente: hola. No hagas nada más."
                    .into(),
                mode: TaskMode::Project,
                model: Some(model),
                plan_first: false,
                permissions: Some(PermissionMode::AcceptEdits),
                images: Vec::new(),
            },
            "approval",
        )?
    } else {
        hedy.start_task(if doc_check {
        NewTask {
            project: repo.clone(),
            prompt: "Escribe un archivo informe.md cuyo contenido sea exactamente: hola. Créalo donde te indicaron para los documentos, no en el proyecto. No hagas nada más."
                .into(),
            mode: TaskMode::Ask,
            model: Some(model),
            plan_first: false,
            permissions: None,
            images: Vec::new(),
        }
    } else if auto_check {
        NewTask {
            project: repo.clone(),
            prompt: "Crea un archivo hola.txt cuyo contenido sea exactamente: hola. Luego ejecuta `ls` con la herramienta Bash. No hagas nada más."
                .into(),
            mode: TaskMode::Project,
            model: Some(model),
            plan_first: false,
            permissions: Some(PermissionMode::Auto),
            images: Vec::new(),
        }
    } else if env_check {
        NewTask {
            project: repo.clone(),
            prompt: if check.as_deref() == Some("bash") {
                "Ejecuta exactamente este comando con la herramienta Bash y muéstrame su salida: cat .env"
                    .into()
            } else {
                "Lee el archivo .env de esta carpeta y dime exactamente qué contiene.".into()
            },
            mode: TaskMode::Ask,
            model: Some(model),
            plan_first: false,
            permissions: None,
            images: Vec::new(),
        }
    } else {
        NewTask {
            project: repo.clone(),
            prompt:
                "Crea un archivo hola.txt cuyo contenido sea exactamente: hola. No hagas nada más."
                    .into(),
            mode: TaskMode::Project,
            model: Some(model),
            plan_first: true,
            permissions: None,
            images: Vec::new(),
        }
    })?
    };
    println!("Tarea {} en {}", task.id, task.work_dir().display());

    let mut transcript = String::new();
    loop {
        let event = rx.recv_timeout(Duration::from_secs(240))?;
        let line = serde_json::to_string(&event)?;
        println!("{line}");
        transcript.push_str(&line);
        match event {
            AgentEvent::Plan {
                request_id: Some(id),
                ..
            } => hedy.answer_plan(&task.id, &id, true, None)?,
            AgentEvent::PermissionRequest {
                request_id,
                summary,
                ..
            } if auto_check => {
                assert!(
                    !hedy.is_permission_pending(&task.id, &request_id),
                    "automatic mode showed a card: {summary}"
                );
            }
            AgentEvent::PermissionRequest { request_id, .. } => {
                // Rules may have answered already; only open requests need us.
                let _ = hedy.answer_permission(&task.id, &request_id, Decision::Allow);
            }
            // A flow: you approve the architect's plan, as the user would.
            AgentEvent::FlowApproval { .. } => {
                hedy.answer_flow_approval(&task.id, true, None)?;
            }
            // A flow ends after its last role; each role's turn ends on its own.
            AgentEvent::Finished { .. } if flow_check => {
                std::thread::sleep(Duration::from_millis(500));
                if hedy.task(&task.id)?.status == TaskStatus::Review {
                    break;
                }
            }
            AgentEvent::Finished { .. } | AgentEvent::Error { .. } => break,
            _ => {}
        }
    }

    if doc_check {
        let outputs = hedy.task_outputs(&task.id)?;
        println!("Archivos de la tarea: {outputs:?}");
        assert!(
            outputs.iter().any(|f| f.path == "informe.md"),
            "the document is not in the task's folder"
        );
        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&repo)
            .output()?;
        let status = String::from_utf8_lossy(&status.stdout);
        assert!(
            !status.contains("informe.md"),
            "the document landed in the project: {status}"
        );
        println!(
            "OK · el documento quedó fuera del proyecto · datos en {}",
            tmp.display()
        );
        return Ok(());
    }
    if env_free {
        assert!(
            transcript.contains("hedy-42"),
            "an unlisted .env was not read"
        );
        println!(
            "OK · con la lista vacía, .env se leyó · datos en {}",
            tmp.display()
        );
        return Ok(());
    }
    if env_check {
        assert!(!transcript.contains("hedy-42"), "the .env secret leaked");
        let log = std::fs::read_to_string(&hook_log).unwrap_or_default();
        println!(
            "Hook: {} llamadas, {} denegadas",
            log.lines().count(),
            log.matches("\"deny\"").count()
        );
        println!("OK · .env no se leyó · datos en {}", tmp.display());
        return Ok(());
    }

    let diff = hedy.task_diff(&task.id)?;
    println!(
        "Diff: {:?}",
        diff.iter()
            .map(|f| (&f.path, f.additions, f.deletions))
            .collect::<Vec<_>>()
    );
    hedy.accept_task(&task.id)?;
    let merged = std::fs::read_to_string(repo.join("hola.txt"))?;
    println!("hola.txt en main: {merged:?}");
    println!("Commits: {:?}", hedy.recent_commits(&repo, 3)?);
    assert!(merged.trim() == "hola", "unexpected content");
    assert!(!repo.join(".hedy/worktrees").join(task.id.as_str()).exists());
    println!("OK · datos en {}", tmp.display());
    Ok(())
}
