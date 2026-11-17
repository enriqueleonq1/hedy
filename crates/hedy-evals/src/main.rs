//! `hedy-evals`: runs the eval suite against a real account. Each run spends
//! tokens of that account.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use hedy_core::adapters::{self, CODE_MAP_PROGRAM, HedyPaths, find_program};
use hedy_core::application::Setting;
use hedy_core::domain::Provider;
use hedy_evals::{
    Case, Layer, RemoveOnDrop, RunResult, Runner, RunnerOptions, Variant, Verdict, compare,
    load_suite, select_cases, summarize,
};

#[derive(Debug, Parser)]
#[command(
    name = "hedy-evals",
    about = "Corre tareas reales con y sin cada capa de Hedy (memoria, mapa del código) y cada experimento, y compara los resultados",
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
    /// Variantes a comparar, separadas por comas: base (sin experimentos),
    /// un experimento (read-guard, briefing, trim-guidance, trim-tools,
    /// search-8, rank-usage) o varios unidos con + (trim = los dos recortes).
    #[arg(long, value_delimiter = ',', default_value = "base")]
    variants: Vec<Variant>,
    /// Corridas por caso, capa y variante.
    #[arg(long, default_value_t = 1)]
    repeat: u32,
    /// Archivo JSONL de resultados (por defecto, <suite>/results/run-<fecha>.jsonl).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Solo lista los casos.
    #[arg(long)]
    list: bool,
    /// No corre nada: resume estos archivos de resultados.
    #[arg(long, value_name = "ARCHIVO", num_args = 1..)]
    report: Vec<PathBuf>,
}

fn main() -> Result<()> {
    // Claude Code runs this binary as its PreToolUse hook and as the memory
    // MCP server, like the app.
    if let Some(code) = hedy_memory::serve_helper_mode() {
        std::process::exit(code);
    }
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    if !args.report.is_empty() {
        let mut results = Vec::new();
        for path in &args.report {
            results.extend(read_results(path)?);
        }
        print_summary(&results);
        return Ok(());
    }
    let cases = select_cases(load_suite(&args.suite)?, &args.cases)
        .with_context(|| format!("en {}", args.suite.display()))?;
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
    let code_map = find_program(CODE_MAP_PROGRAM).map(|p| p.to_string_lossy().into_owned());
    if args.layers.iter().any(|l| l.code_map()) && code_map.is_none() {
        bail!("las capas code-map y full necesitan {CODE_MAP_PROGRAM} (instálalo desde Ajustes)");
    }

    // Runs go to their own data folder, so they do not fill the user's
    // projects, tasks and memory. It is removed on every way out.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let work = std::env::temp_dir().join(format!("hedy-evals-{stamp}"));
    let _work = RemoveOnDrop(work.clone());
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
    let total = cases.len() * args.layers.len() * args.variants.len() * args.repeat as usize;
    eprintln!(
        "{total} corridas con {} ({}). Resultados en {}",
        account.name,
        args.model.as_deref().unwrap_or("modelo por defecto"),
        out.display()
    );

    let mut results = Vec::new();
    let finished = run_all(&runner, &cases, &args, &mut file, &mut results);
    print_summary(&results);
    finished
}

/// Every attempt, case, layer and variant, in that order: the variants of a
/// case run back to back, so a slow hour or a busy account weighs on all of
/// them alike. `results` keeps the runs done even when writing one fails.
fn run_all(
    runner: &Runner,
    cases: &[Case],
    args: &Args,
    file: &mut File,
    results: &mut Vec<RunResult>,
) -> Result<()> {
    for attempt in 1..=args.repeat {
        for case in cases {
            for &layer in &args.layers {
                for variant in &args.variants {
                    eprint!("· {} [{layer}] {} #{attempt}… ", case.name, variant.name);
                    let result = runner.run(case, layer, variant, attempt);
                    eprintln!(
                        "{} · {} tokens · {} lecturas · {:.0} s{}",
                        if result.passed { "pasó" } else { "falló" },
                        result.tokens,
                        result.files_read,
                        result.duration_ms as f64 / 1000.0,
                        result
                            .error
                            .as_deref()
                            .map(|e| format!(" · {e}"))
                            .unwrap_or_default()
                    );
                    let line = serde_json::to_string(&result)?;
                    results.push(result);
                    writeln!(file, "{line}")?;
                    file.flush()?;
                }
            }
        }
    }
    Ok(())
}

fn read_results(path: &PathBuf) -> Result<Vec<RunResult>> {
    let file = File::open(path).with_context(|| format!("no se pudo abrir {}", path.display()))?;
    let mut results = Vec::new();
    for (n, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        results.push(
            serde_json::from_str(&line)
                .with_context(|| format!("{}:{} no es válido", path.display(), n + 1))?,
        );
    }
    Ok(results)
}

fn print_summary(results: &[RunResult]) {
    if results.is_empty() {
        return;
    }
    let summaries = summarize(results);
    println!(
        "\n{:<22} {:<9} {:<26} {:>7} {:>9} {:>9} {:>8} {:>8} {:>7}",
        "caso", "capa", "variante", "pasaron", "tokens", "entrada", "lecturas", "costo", "seg"
    );
    for s in &summaries {
        println!(
            "{:<22} {:<9} {:<26} {:>7} {:>9} {:>9} {:>8} {:>8} {:>7.0}",
            s.case,
            s.layer.as_str(),
            s.variant,
            format!("{}/{}", s.passed, s.runs),
            s.median_tokens,
            s.median_input,
            s.median_files_read,
            s.avg_cost_usd
                .map_or_else(|| "—".into(), |c| format!("${c:.3}")),
            s.avg_secs
        );
    }
    println!(
        "\nMedianas por corrida (costo y segundos: promedios). El costo es la estimación de la CLI, no lo que cobra una suscripción."
    );
    let comparisons = compare(&summaries);
    if comparisons.is_empty() {
        return;
    }
    println!(
        "\n{:<26} {:<9} {:>5} {:>8} {:>8} {:>13} {:>9} {:>9}  veredicto",
        "variante", "capa", "casos", "tokens", "costo", "costo/resuelta", "pasaron", "base"
    );
    for c in &comparisons {
        let verdict = match c.verdict() {
            Verdict::Keep => "se queda",
            Verdict::Maybe => "opcional",
            Verdict::Drop => "se descarta",
        };
        println!(
            "{:<26} {:<9} {:>5} {:>7.1}% {:>7.1}% {:>13} {:>9} {:>9}  {verdict}",
            c.variant,
            c.layer.as_str(),
            c.cases,
            c.token_change_pct,
            c.cost_change_pct,
            c.cost_per_pass_change_pct
                .map_or_else(|| "—".into(), |p| format!("{p:.1}%")),
            format!("{}/{}", c.passed, c.runs),
            format!("{}/{}", c.base_passed, c.base_runs),
        );
    }
    println!(
        "\nCambios: mediana por caso frente a la base; costo/resuelta, sobre todos los casos. Regla: se queda con 10 % menos de costo o más y sin bajar la tasa de aprobación."
    );
}
