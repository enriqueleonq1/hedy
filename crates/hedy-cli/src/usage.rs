//! `hedy usage` (how much of each plan is used, from the cache the app
//! shares) and the warning before `hedy run` when the account is close to
//! its limit. Hedy only warns and offers: it never switches accounts.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::Result;
use chrono::{Datelike, TimeZone};
use hedy_core::application::{Hedy, LimitCheck, PlanUsageState, Resolved, RunLimitWarning};
use hedy_core::domain::{Account, LimitLevel, UsageProblem, UsageWindow, WindowKind, short_names};
use hedy_core::tr;

use crate::commands::age_label;
use crate::spinner;
use crate::style::{Style, pad};

/// What `hedy usage` was asked for.
#[derive(Debug, Clone, Copy, Default)]
pub struct UsageOptions {
    /// Never ask the providers: only what is cached.
    pub cached: bool,
    /// Ask now (still at most every 3 minutes per account, as the app).
    pub refresh: bool,
    /// One compact line for a prompt or a status bar.
    pub short: bool,
    pub json: bool,
}

/// One account's figures, or why they could not be read.
struct Row {
    account: Account,
    state: Option<PlanUsageState>,
    error: Option<String>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn usage(hedy: &Hedy, options: UsageOptions) -> Result<()> {
    let accounts = hedy.accounts()?;
    // A status bar must not wait: `--short` reads the cache unless told.
    let cached = options.cached || (options.short && !options.refresh);
    let rows = if cached {
        accounts
            .into_iter()
            .map(|account| {
                let read = hedy.plan_usage(account.id.as_str());
                let (state, error) = match read {
                    Ok(state) => (Some(state), None),
                    Err(e) => (None, Some(e.to_string())),
                };
                Row {
                    account,
                    state,
                    error,
                }
            })
            .collect()
    } else {
        refreshed(hedy, accounts)
    };
    let labels = short_names(
        &rows
            .iter()
            .map(|r| r.account.name.as_str())
            .collect::<Vec<_>>(),
    );
    if options.json {
        print_json(&rows, &labels)?;
    } else if options.short {
        let line = short_line(&Style::for_stdout(), &rows, &labels);
        if !line.is_empty() {
            println!("{line}");
        }
    } else {
        print_rows(&Style::for_stdout(), &rows, cached);
    }
    Ok(())
}

/// Every account asked in parallel (each at most every 3 minutes, as the
/// app); a failure only affects its own account.
fn refreshed(hedy: &Hedy, accounts: Vec<Account>) -> Vec<Row> {
    spinner::while_running(
        &tr!(
            "Revisando el uso de tus planes…",
            "Checking your plans' usage…"
        ),
        || {
            std::thread::scope(|scope| {
                let handles: Vec<_> = accounts
                    .iter()
                    .map(|a| scope.spawn(move || hedy.refresh_plan_usage(a.id.as_str())))
                    .collect();
                accounts
                    .iter()
                    .zip(handles)
                    .map(|(account, handle)| {
                        let refreshed = handle
                            .join()
                            .unwrap_or_else(|_| hedy.plan_usage(account.id.as_str()));
                        let (state, error) = match refreshed {
                            Ok(state) => (Some(state), None),
                            Err(e) => (
                                hedy.plan_usage(account.id.as_str()).ok(),
                                Some(e.to_string()),
                            ),
                        };
                        Row {
                            account: account.clone(),
                            state,
                            error,
                        }
                    })
                    .collect()
            })
        },
    )
}

/// "sesión 5 h", "semana (Fable)", "día", "60 min".
fn window_name(w: &UsageWindow) -> String {
    let name = match w.kind {
        WindowKind::Session => tr!("sesión 5 h", "5-h session"),
        WindowKind::Week => tr!("semana", "week"),
        WindowKind::Day => tr!("día", "day"),
        WindowKind::Other => match w.minutes {
            Some(m) if m >= 60 && m % 60 == 0 => format!("{} h", m / 60),
            Some(m) => format!("{m} min"),
            None => tr!("otro límite", "other limit"),
        },
    };
    match &w.model {
        Some(model) => format!("{name} ({model})"),
        None => name,
    }
}

const WEEKDAYS_ES: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];
const WEEKDAYS_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS_ES: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];
const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// When a window renews, short and in local time: "se renueva 18:40",
/// "se renueva mañana 09:00", "se renueva jue 20, 00:00"; the source's own
/// text when the time is unknown.
fn renews(w: &UsageWindow) -> Option<String> {
    renews_at(w.resets_at, w.resets_text.as_deref())
}

fn renews_at(resets_at: Option<i64>, resets_text: Option<&str>) -> Option<String> {
    let Some(at) = resets_at.and_then(|t| chrono::Local.timestamp_opt(t, 0).single()) else {
        return resets_text.map(|text| tr!("se renueva {text}", "renews {text}"));
    };
    let time = at.format("%H:%M").to_string();
    let days = (at.date_naive() - chrono::Local::now().date_naive()).num_days();
    let english = hedy_core::i18n::language() == hedy_core::i18n::Language::English;
    let day = at.day();
    let when = match days {
        ..=0 => time,
        1 => tr!("mañana {time}", "tomorrow {time}"),
        2..=6 => {
            let i = at.weekday().num_days_from_monday() as usize;
            let weekday = if english {
                WEEKDAYS_EN[i]
            } else {
                WEEKDAYS_ES[i]
            };
            format!("{weekday} {day}, {time}")
        }
        _ => {
            let i = at.month0() as usize;
            if english {
                format!("{} {day}, {time}", MONTHS_EN[i])
            } else {
                format!("{day} {}, {time}", MONTHS_ES[i])
            }
        }
    };
    Some(tr!("se renueva {when}", "renews {when}"))
}

/// The percentage, warning from 80 % and danger from 95 %, as in the app.
fn percent(s: &Style, value: f64, width: usize) -> String {
    let text = format!("{:>width$}", format!("{value:.0}%"));
    if value >= 95.0 {
        s.error(text)
    } else if value >= 80.0 {
        s.warn(text)
    } else {
        text
    }
}

/// Why an account has no figure, honestly.
fn no_figure(row: &Row, cached: bool) -> String {
    if let Some(error) = &row.error {
        return tr!("no disponible: {error}", "unavailable: {error}");
    }
    let Some(state) = &row.state else {
        return tr!("no disponible", "unavailable");
    };
    let provider = row.account.provider.display_name();
    if !state.supported {
        return if row.account.is_subscription() {
            tr!(
                "{provider} aún no informa el límite a Hedy",
                "{provider} does not report its limit to Hedy yet"
            )
        } else {
            tr!("sin plan: se paga por uso", "no plan: pay per use")
        };
    }
    if let Some(balance) = state.usage.as_ref().and_then(|u| u.balance.as_ref()) {
        let amount = format!("{:.2} {}", balance.available, balance.currency);
        return tr!(
            "saldo {amount} · sin plan: se paga por uso",
            "balance {amount} · no plan: pay per use"
        );
    }
    match state.problem {
        Some(UsageProblem::SpendsTokens) => tr!(
            "no disponible: esta versión gastaría tokens para leerlo",
            "unavailable: this version would spend tokens to read it"
        ),
        Some(UsageProblem::Unavailable) => tr!(
            "no disponible: no se pudo leer",
            "unavailable: could not be read"
        ),
        _ if cached => tr!(
            "sin datos aún (`hedy usage` los pide)",
            "no data yet (`hedy usage` asks for it)"
        ),
        _ => tr!("sin datos aún", "no data yet"),
    }
}

fn print_rows(s: &Style, rows: &[Row], cached: bool) {
    if rows.is_empty() {
        println!(
            "{}",
            tr!(
                "Todavía no hay cuentas: añade una con `hedy account add`.",
                "No accounts yet: add one with `hedy account add`."
            )
        );
        return;
    }
    let now = now_secs();
    let width = rows
        .iter()
        .map(|r| r.account.name.chars().count())
        .max()
        .unwrap_or(0);
    // Where the other windows start: under the percentage.
    let indent = " ".repeat(width + 4);
    for row in rows {
        let a = &row.account;
        let name = pad(&s.bold(&a.name), a.name.chars().count(), width);
        let usage = row.state.as_ref().and_then(|st| st.usage.as_ref());
        let Some((usage, main)) = usage.and_then(|u| u.headline().map(|w| (u, w))) else {
            let reason = no_figure(row, cached);
            println!(
                "{} {name}  {:>4}  {}",
                s.chip(a.color),
                "—",
                s.muted(reason)
            );
            continue;
        };
        let mut line = format!(
            "{} {name}  {}  {}",
            s.chip(a.color),
            percent(s, main.percent, 4),
            window_name(main)
        );
        if let Some(renews) = renews(main) {
            line.push_str(&format!(" · {renews}"));
        }
        let mut age = age_label(now - usage.checked_at);
        if row.error.is_some()
            || row
                .state
                .as_ref()
                .is_some_and(|st| st.problem == Some(UsageProblem::Unavailable))
        {
            age.push_str(&tr!(" · no se pudo actualizar", " · could not refresh"));
        }
        println!("{line}  {}", s.muted(format!("({age})")));
        for w in usage.windows.iter().filter(|w| *w != main) {
            let mut other = format!("{}  {}", percent(s, w.percent, 4), window_name(w));
            if let Some(renews) = renews(w) {
                other.push_str(&format!(" · {renews}"));
            }
            println!("{indent}{}", s.muted(other));
        }
    }
}

/// `CP 83% · CT 34% · GPT 12%`: accounts with a plan figure only.
fn short_line(s: &Style, rows: &[Row], labels: &[String]) -> String {
    rows.iter()
        .zip(labels)
        .filter_map(|(row, label)| {
            let w = row.state.as_ref()?.usage.as_ref()?.headline()?;
            Some(format!("{label} {}", percent(s, w.percent, 0)))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn print_json(rows: &[Row], labels: &[String]) -> Result<()> {
    let now = now_secs();
    let list: Vec<serde_json::Value> = rows
        .iter()
        .zip(labels)
        .map(|(row, label)| {
            let a = &row.account;
            let state = row.state.as_ref();
            let usage = state.and_then(|st| st.usage.as_ref());
            serde_json::json!({
                "id": a.id.as_str(),
                "name": a.name,
                "short": label,
                "provider": a.provider.as_str(),
                "subscription": a.is_subscription(),
                "color": a.color.to_string(),
                "supported": state.map(|st| st.supported),
                "headline": usage.and_then(|u| u.headline()),
                "usage": usage,
                "ageSecs": usage.map(|u| now - u.checked_at),
                "attemptedAt": state.and_then(|st| st.attempted_at),
                "problem": state.and_then(|st| st.problem).map(UsageProblem::as_str),
                "error": row.error,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&list)?);
    Ok(())
}

/// The sentence about the account's limit: "Claude trabajo va al 92 %
/// (semana) · se renueva jue 20, 00:00 · dato de hace 2 min".
fn limit_sentence(account: &Account, check: &LimitCheck) -> String {
    let name = &account.name;
    let mut text = match (&check.window, check.level) {
        (Some(w), LimitLevel::Reached) => {
            let (window, figure) = (window_name(w), w.percent.round());
            tr!(
                "{name} llegó a su límite ({window}, {figure}%)",
                "{name} reached its limit ({window}, {figure}%)"
            )
        }
        (Some(w), LimitLevel::Warning) => {
            let (window, figure) = (window_name(w), w.percent.round());
            tr!(
                "{name} va al {figure}% ({window})",
                "{name} is at {figure}% ({window})"
            )
        }
        (None, _) => tr!("{name} llegó a su límite", "{name} reached its limit"),
    };
    let renewal = match &check.window {
        Some(w) => renews(w),
        None => renews_at(check.resets_at, None),
    };
    if let Some(renewal) = renewal {
        text.push_str(&format!(" · {renewal}"));
    }
    let age = age_label(now_secs() - check.checked_at);
    text.push_str(&tr!(" · dato de {age}", " · figure from {age}"));
    text
}

/// The warning before `hedy run`, on stderr.
fn print_run_warning(s: &Style, account: &Account, warning: &RunLimitWarning) {
    let reached = warning.check.level == LimitLevel::Reached;
    let mark = if reached {
        s.error("✗")
    } else {
        s.warn("⚠")
    };
    eprintln!("{mark} {}", limit_sentence(account, &warning.check));
    eprintln!(
        "  {}",
        tr!(
            "Hedy no cambia de cuenta por su cuenta. Puedes:",
            "Hedy never switches accounts by itself. You can:"
        )
    );
    let name = &account.name;
    eprintln!("    · {}", tr!("seguir con {name}", "go on with {name}"));
    if warning.protected {
        eprintln!(
            "    · {}",
            s.muted(tr!(
                "este proyecto está protegido: solo usa {name}",
                "this project is protected: it only uses {name}"
            ))
        );
    } else if warning.alternatives.is_empty() {
        eprintln!(
            "    · {}",
            tr!(
                "usar una cuenta de otro proveedor (no hay ninguna aquí: `hedy account add`)",
                "use another provider's account (there is none here: `hedy account add`)"
            )
        );
    } else {
        for (other, figure) in &warning.alternatives {
            let figure = match figure {
                Some(p) => format!("{} · {}", other.name, percent(s, *p, 0)),
                None => {
                    let other_name = &other.name;
                    tr!("{other_name} · sin datos", "{other_name} · no data")
                }
            };
            let command = format!("hedy run -a {}", other.id);
            eprintln!(
                "    · {} {}  {}",
                tr!("usar otra:", "use another:"),
                s.command(command),
                s.muted(format!("({figure})"))
            );
        }
    }
    eprintln!(
        "    · {}",
        tr!("esperar a que se renueve", "wait for it to renew")
    );
}

/// Before `hedy run`: warns when the account is close to (≥ 90 %) or at its
/// limit, from the cache only, and in a terminal asks whether to go on (yes
/// by default; no at the limit). Returns whether to open the CLI.
pub fn confirm_run(hedy: &Hedy, resolved: &Resolved, yes: bool) -> bool {
    // Nothing known or the check failed: never in the way of a launch.
    let Ok(Some(warning)) = hedy.run_limit_warning(resolved) else {
        return true;
    };
    let s = Style::for_stderr();
    print_run_warning(&s, &resolved.account, &warning);
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if yes || !interactive {
        return true;
    }
    let default = warning.check.level != LimitLevel::Reached;
    let name = &resolved.account.name;
    let choices = match default {
        true => tr!("[S/n]", "[Y/n]"),
        false => tr!("[s/N]", "[y/N]"),
    };
    eprint!(
        "{} ",
        s.bold(tr!(
            "¿Seguir con {name}? {choices}",
            "Continue with {name}? {choices}"
        ))
    );
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    if std::io::stdin().lock().read_line(&mut answer).is_err() {
        return false;
    }
    let go = match answer.trim().to_lowercase().chars().next() {
        Some('s' | 'y') => true,
        Some('n') => false,
        _ => default,
    };
    if !go {
        eprintln!(
            "{}",
            s.muted(tr!(
                "No se abrió nada. Elige otra cuenta con `hedy run -a <cuenta>` o espera.",
                "Nothing was opened. Pick another account with `hedy run -a <account>` or wait."
            ))
        );
    }
    go
}
