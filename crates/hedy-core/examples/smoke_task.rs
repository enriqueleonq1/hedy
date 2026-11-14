//! End-to-end check against the real Claude Code CLI (spends a few tokens).
//!
//! Creates a throwaway git repo and Hedy data folder, adopts the Claude account
//! of the default folder (`~/.claude`), runs a tiny task with plan approval and
//! permission answers, then accepts it and checks the merge.
//!
//! ```fish
//! cargo run -p hedy-core --example smoke_task -- [model]
//! ```

use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use hedy_core::adapters::{self, HedyPaths};
use hedy_core::application::ports::TaskEvents;
use hedy_core::application::{NewAccount, NewTask};
use hedy_core::domain::{AgentEvent, Decision, Provider, Task, TaskId, TaskMode};

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
    let model = std::env::args().nth(1).unwrap_or_else(|| "haiku".into());
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
    git(&repo, &["config", "user.name", "Hedy smoke"]);
    git(&repo, &["config", "user.email", "hedy@local"]);

    let (tx, rx) = mpsc::channel();
    let hedy = adapters::system(&HedyPaths::at(tmp.join("data")))?
        .with_task_events(Arc::new(Forward(Mutex::new(tx))));
    hedy.add_account(NewAccount {
        name: "Claude".into(),
        provider: Provider::Claude,
        api_key: false,
        default_folder: true,
        color: None,
    })?;
    hedy.use_account(&repo, "claude", None)?;

    let task = hedy.start_task(NewTask {
        project: repo.clone(),
        prompt: "Crea un archivo hola.txt cuyo contenido sea exactamente: hola. No hagas nada más."
            .into(),
        mode: TaskMode::Project,
        model: Some(model),
        plan_first: true,
    })?;
    println!("Tarea {} en {}", task.id, task.work_dir().display());

    loop {
        let event = rx.recv_timeout(Duration::from_secs(240))?;
        println!("{}", serde_json::to_string(&event)?);
        match event {
            AgentEvent::Plan {
                request_id: Some(id),
                ..
            } => hedy.answer_plan(&task.id, &id, true, None)?,
            AgentEvent::PermissionRequest { request_id, .. } => {
                // Rules may have answered already; only open requests need us.
                let _ = hedy.answer_permission(&task.id, &request_id, Decision::Allow);
            }
            AgentEvent::Finished { .. } | AgentEvent::Error { .. } => break,
            _ => {}
        }
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
