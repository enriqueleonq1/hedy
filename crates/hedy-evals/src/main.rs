//! `hedy-evals`: runs the eval suite against a real account. Each run spends
//! tokens of that account.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use hedy_core::adapters::{self, HedyPaths, MEMORY_SERVER_ARG, find_program, hook};
use hedy_core::application::Setting;
use hedy_core::domain::Provider;
use hedy_evals::{Layer, Runner, RunnerOptions, load_suite, summarize};

#[derive(Debug, Parser)]
#[command(
    name = "hedy-evals",
    about = "Corre tareas reales con y sin cada capa de Hedy (memoria, mapa del código) y compara los resultados",
    after_help = "Cada corrida es una sesión real del agente y gasta tokens de la cuenta."
)]
struct Args {
    /// Carpeta con los casos.
    #[arg(long, default_value = "evals")]
    suite: PathBuf,
    /// Solo estos casos (por defecto, todos).
    #[arg(long = "case", value_name = "CASO")]
    cases: Vec<String>,
    /// Capas a comparar, separadas por comas: plain, memory, code-map, full.
    #[arg(long, value_delimiter = ',', default_value = "plain,memory")]
    layers: Vec<Layer>,
    /// Cuenta de Hedy (por defecto, la cuenta por defecto).
    #[arg(long)]
    account: Option<String>,
    /// Modelo, por ejemplo haiku o sonnet.
    #[arg(long)]
    model: Option<String>,
    /// Corridas por caso y capa.
    #[arg(long, default_value_t = 1)]
    repeat: u32,
    /// Archivo JSONL de resultados (por defecto, <suite>/results/run-<fecha>.jsonl).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Solo lista los casos.
    #[arg(long)]
    list: bool,
}

fn main() -> Result<()> {
    // Claude Code runs this binary as its PreToolUse hook and as the memory
    // MCP server, like the app.
    if std::env::args().skip(1).eq(hook::HOOK_ARGS) {
        std::process::exit(hook::run());
    }
    if std::env::args().nth(1).as_deref() == Some(MEMORY_SERVER_ARG) {
        return hedy_memory::serve(hedy_memory::Options::parse(std::env::args().skip(2))?);
    }
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    let mut cases = load_suite(&args.suite)?;
    if !args.cases.is_empty() {
        cases.retain(|c| args.cases.contains(&c.name));
        if cases.len() != args.cases.len() {
            bail!(
                "no encontré todos los casos pedidos en {}",
                args.suite.display()
            );
        }
    }
    if args.list {
        for case in &cases {
            let measures = case.spec.measures.as_deref().unwrap_or("-");
            println!("{:<22} {:<10} {}", case.name, measures, case.spec.title);
        }
        return Ok(());
    }

    // The user's Hedy: only read, for the account and its settings.
    let user = adapters::system(&HedyPaths::detect()?)?;
    let account = match &args.account {
        Some(id) => user.account(id)?,
        None => {
            let id = user
                .setting(Setting::DefaultAccount)?
                .context("no hay cuenta por defecto; usa --account <cuenta>")?;
            user.account(&id)?
        }
    };

    // Runs go to their own data folder, so they do not fill the user's
    // projects, tasks and memory.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let work = std::env::temp_dir().join(format!("hedy-evals-{stamp}"));
    let program = std::env::current_exe()?;
    let hedy = adapters::system_for_app(&HedyPaths::at(work.join("data")), &program)?;
    hedy.adopt_account(&account)?;
    for provider in Provider::ALL {
        for setting in [
            Setting::Binary(provider),
            Setting::ApiUrl(provider),
            Setting::ApiPrice(provider),
        ] {
            if let Ok(Some(value)) = user.setting(setting) {
                hedy.set_setting(setting, Some(&value))?;
            }
        }
    }
    let code_map = find_program("codebase-memory-mcp").map(|p| p.to_string_lossy().into_owned());
    if args.layers.iter().any(|l| l.code_map()) && code_map.is_none() {
        bail!("las capas code-map y full necesitan codebase-memory-mcp (instálalo desde Ajustes)");
    }

    let out = args.out.clone().unwrap_or_else(|| {
        args.suite
            .join("results")
            .join(format!("run-{stamp}.jsonl"))
    });
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out)?;

    let runner = Runner::new(
        hedy,
        RunnerOptions {
            account: account.id.clone(),
            model: args.model.clone(),
            code_map,
            work_dir: work.clone(),
        },
    );
    let total = cases.len() * args.layers.len() * args.repeat as usize;
    eprintln!(
        "{total} corridas con {} ({}). Resultados en {}",
        account.name,
        args.model.as_deref().unwrap_or("modelo por defecto"),
        out.display()
    );

    let mut results = Vec::new();
    for case in &cases {
        for &layer in &args.layers {
            for attempt in 1..=args.repeat {
                eprint!("· {} [{layer}] #{attempt}… ", case.name);
                let result = runner.run(case, layer, attempt)?;
                eprintln!(
                    "{} · {} tokens · {:.0} s{}",
                    if result.passed { "pasó" } else { "falló" },
                    result.tokens,
                    result.duration_ms as f64 / 1000.0,
                    result
                        .error
                        .as_deref()
                        .map(|e| format!(" · {e}"))
                        .unwrap_or_default()
                );
                writeln!(file, "{}", serde_json::to_string(&result)?)?;
                results.push(result);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&work);

    println!(
        "\n{:<22} {:<9} {:>7} {:>9} {:>8} {:>7}",
        "caso", "capa", "pasaron", "tokens", "costo", "seg"
    );
    for s in summarize(&results) {
        println!(
            "{:<22} {:<9} {:>7} {:>9} {:>8} {:>7.0}",
            s.case,
            s.layer.as_str(),
            format!("{}/{}", s.passed, s.runs),
            s.avg_tokens,
            s.avg_cost_usd
                .map_or_else(|| "—".into(), |c| format!("${c:.3}")),
            s.avg_secs
        );
    }
    println!(
        "\nPromedios por corrida. El costo es la estimación de la CLI, no lo que cobra una suscripción."
    );
    Ok(())
}
