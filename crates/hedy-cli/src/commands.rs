//! One function per command. They call use cases on [`Hedy`] and render results.

use std::io::{BufRead, IsTerminal};
use std::path::Path;

use anyhow::{Result, bail};
use chrono::Timelike;
use hedy_core::application::{
    AccountStatus, CheckArea, CheckLevel, DoctorReport, Hedy, NewAccount, NewTemplate, Proposal,
    RECENT_HOURS, Setting, SkillFiles, TaskActivity, TemplateScope,
};
use hedy_core::domain::memory::USER_AUTHOR;
use hedy_core::domain::{
    Account, Category, ConfigFolder, Credentials, LoginState, Note, Phase, Project, PromptTemplate,
    Provider, Skill, SkillStatus, TaskState,
};
use hedy_core::i18n::{self, Language};
use hedy_core::tr;

use crate::cli::AddAccount;
use crate::mascot::{self, Mood};
use crate::spinner;
use crate::style::{Style, pad, tilde};

/// What to do when there are no accounts yet.
fn print_no_accounts(s: &Style) {
    let personal = tr!("Claude personal", "Personal Claude");
    let work = tr!("Claude trabajo", "Work Claude");
    let mut text = Vec::new();
    if s.enabled() {
        text.push(s.bold(tr!("¡Hola! Soy Hedy.", "Hi! I'm Hedy.")));
    }
    text.extend([
        tr!(
            "Todavía no hay cuentas. Agrega la primera, por ejemplo:",
            "No accounts yet. Add the first one, for example:"
        ),
        format!(
            "  {}",
            s.command(format!("hedy account add \"{personal}\" --claude"))
        ),
        format!(
            "  {}",
            s.command(format!(
                "hedy account add \"{work}\" --claude --default-folder"
            ))
        ),
    ]);
    mascot::print_beside(s, Mood::Hi, &text);
}

/// "Buenas tardes, Ana", from the time of day and `ui.name`.
fn greeting(hedy: &Hedy) -> String {
    let hello = match chrono::Local::now().hour() {
        6..12 => tr!("Buenos días", "Good morning"),
        12..20 => tr!("Buenas tardes", "Good afternoon"),
        _ => tr!("Buenas noches", "Good evening"),
    };
    let name = hedy
        .setting(Setting::UserName)
        .ok()
        .flatten()
        .filter(|n| !n.trim().is_empty());
    match name {
        Some(name) => tr!("¡{hello}, {name}!", "{hello}, {name}!"),
        None => tr!("¡{hello}!", "{hello}!"),
    }
}

/// Muted text whose `quoted` parts are painted as commands.
fn hint(s: &Style, text: &str) -> String {
    if !s.enabled() {
        return text.to_owned();
    }
    text.split('`')
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 1 {
                s.command(part)
            } else {
                s.muted(part)
            }
        })
        .collect()
}

fn tips() -> Vec<String> {
    vec![
        tr!(
            "`hedy run` abre la CLI oficial con la cuenta de este proyecto.",
            "`hedy run` opens the official CLI with this project's account."
        ),
        tr!(
            "`hedy use <cuenta> --protect` reserva una cuenta solo para un proyecto.",
            "`hedy use <account> --protect` keeps an account for one project only."
        ),
        tr!(
            "`hedy tareas` muestra qué hacen tus IAs en todos los proyectos.",
            "`hedy tasks` shows what your AIs are doing in every project."
        ),
        tr!(
            "`hedy resumen` muestra cuánto usaste, por cliente y por proyecto.",
            "`hedy summary` shows how much you used, by client and by project."
        ),
        tr!(
            "`hedy client <nombre>` suma el costo de tus proyectos por cliente.",
            "`hedy client <name>` adds up the cost of your projects by client."
        ),
        tr!(
            "`hedy memoria` lista lo que tus IAs propusieron recordar; tú decides qué se guarda.",
            "`hedy memory` lists what your AIs proposed to remember; you decide what is kept."
        ),
        tr!(
            "`hedy config set ui.name <tu nombre>` y te saludo por tu nombre.",
            "`hedy config set ui.name <your name>` and I'll greet you by name."
        ),
        tr!(
            "`hedy config set ui.language en` me cambia a inglés.",
            "`hedy config set ui.language es` switches me to Spanish."
        ),
        tr!(
            "`hedy completions fish` activa el autocompletado (también bash y zsh).",
            "`hedy completions fish` turns on completions (bash and zsh too)."
        ),
        tr!(
            "`hedy diagnostico` revisa tus CLIs y cuentas, y avisa si una CLI cambió de versión.",
            "`hedy doctor` checks your CLIs and accounts, and warns when a CLI changed version."
        ),
        tr!(
            "`-C <carpeta>` usa cualquier comando sobre otro proyecto sin entrar en él.",
            "`-C <folder>` runs any command on another project without going there."
        ),
        tr!(
            "Si una cuenta llega a su límite te aviso: nunca cambio de cuenta sola.",
            "If an account hits its limit I'll tell you: I never switch accounts on my own."
        ),
    ]
}

/// One tip, different from run to run. Only in a terminal.
fn print_tip(s: &Style) {
    if !s.enabled() {
        return;
    }
    let tips = tips();
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as usize);
    let tip = &tips[seed % tips.len()];
    println!("\n  {} {}\n", s.warn("✦"), hint(s, tip));
}

pub fn status(hedy: &Hedy, dir: &Path) -> Result<()> {
    let s = Style::for_stdout();
    let status = spinner::while_running(
        &tr!("Revisando tus cuentas…", "Checking your accounts…"),
        || hedy.status(dir),
    )?;

    if status.accounts.is_empty() {
        print_no_accounts(&s);
        return Ok(());
    }

    let find =
        |id: &hedy_core::domain::AccountId| status.accounts.iter().find(|a| &a.account.id == id);
    let mut text = Vec::new();
    if s.enabled() {
        text.extend([s.bold(greeting(hedy)), String::new()]);
    }
    match &status.current_project {
        Some(project) => {
            let lock = if project.protected {
                s.warn(tr!("  · protegido", "  · protected"))
            } else {
                String::new()
            };
            // The two titles are aligned.
            let titles = [tr!("Proyecto", "Project"), tr!("Cuenta", "Account")];
            let width = titles.iter().map(|t| t.chars().count()).max().unwrap_or(0);
            let [project_title, account_title] =
                titles.map(|t| pad(&s.muted(&t), t.chars().count(), width));
            text.push(format!(
                "{project_title} {}  {}{lock}",
                s.bold(&project.name),
                s.muted(short_path(&project.path)),
            ));
            let account = match project.account.as_ref().and_then(find) {
                Some(a) => account_label(&s, &a.account),
                None => hint(
                    &s,
                    &tr!(
                        "ninguna (usa `hedy use <cuenta>`)",
                        "none (use `hedy use <account>`)"
                    ),
                ),
            };
            text.push(format!("{account_title} {account}"));
        }
        None => {
            text.push(s.muted(tr!(
                "Esta carpeta no es un proyecto de Hedy.",
                "This folder is not a Hedy project."
            )));
            match status.default_account.as_ref().and_then(find) {
                Some(a) => text.push(format!(
                    "{}  {}",
                    s.muted(tr!("Cuenta por defecto", "Default account")),
                    account_label(&s, &a.account)
                )),
                None => text.push(hint(
                    &s,
                    &tr!(
                        "Sin cuenta por defecto (usa `hedy account default <cuenta>`).",
                        "No default account (use `hedy account default <account>`)."
                    ),
                )),
            }
        }
    }
    mascot::print_beside(&s, Mood::Hi, &text);

    println!("\n{}", s.section(&tr!("Cuentas", "Accounts")));
    let cards: Vec<Card> = status
        .accounts
        .iter()
        .map(|a| status_card(&s, a, status.default_account.as_ref()))
        .collect();
    print_cards(&s, &cards);
    if s.enabled() {
        print_tip(&s);
    } else {
        println!(
            "\n{}",
            tr!(
                "Uso por proyecto y cliente: `hedy resumen`.",
                "Usage by project and client: `hedy summary`."
            )
        );
    }
    Ok(())
}

pub fn set_client(hedy: &Hedy, dir: &Path, client: Option<&str>) -> Result<()> {
    let s = Style::for_stdout();
    let Some(project) = hedy.project_for(dir)? else {
        bail!(
            "{}",
            tr!(
                "esta carpeta no es un proyecto de Hedy; usa `hedy use <cuenta>` primero",
                "this folder is not a Hedy project; use `hedy use <account>` first"
            )
        );
    };
    let project = hedy.set_project_client(&project.path, client)?;
    let name = s.bold(&project.name);
    match &project.client {
        Some(client) => {
            let client = s.bold(client);
            println!(
                "{} {}",
                s.ok("✔"),
                tr!("{name} es para {client}", "{name} is for {client}")
            );
        }
        None => println!(
            "{}",
            tr!(
                "{name} ya no tiene cliente",
                "{name} no longer has a client"
            )
        ),
    }
    Ok(())
}

/// Spanish writes decimals with a comma.
fn decimals(number: String) -> String {
    match i18n::language() {
        Language::Spanish => number.replace('.', ","),
        Language::English => number,
    }
}

/// "182 k" / "1,2 M" tokens.
fn tokens(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{} k", n / 1_000),
        _ => decimals(format!("{:.1} M", n as f64 / 1_000_000.0)),
    }
}

fn usd(cost: Option<f64>) -> String {
    cost.map_or_else(|| "—".into(), |c| decimals(format!("${c:.2}")))
}

/// `~/…` path, keeping its end when it is long.
fn short_path(path: &Path) -> String {
    const MAX: usize = 40;
    let path = tilde(path);
    let len = path.chars().count();
    if len <= MAX {
        return path;
    }
    let tail: String = path.chars().skip(len - (MAX - 1)).collect();
    format!("…{tail}")
}

/// Cuts `text` to `width` characters with an ellipsis.
fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Splits `text` into lines of at most `width` characters.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().expect("never empty");
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(word.to_owned());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

const NAME_WIDTH: usize = 24;
const BAR_WIDTH: usize = 20;

pub fn summary(hedy: &Hedy, days: u32) -> Result<()> {
    let s = Style::for_stdout();
    let summary = hedy.usage_summary(days)?;
    let period = if days == 7 {
        tr!("esta semana", "this week")
    } else {
        tr!("los últimos {days} días", "in the last {days} days")
    };
    if summary.projects.is_empty() {
        let mut text = vec![tr!(
            "Sin uso registrado {period}.",
            "No usage recorded {period}."
        )];
        if s.enabled() {
            text.push(s.muted(tr!(
                "Lo que hagas con Hedy aparecerá aquí, por cliente y por proyecto.",
                "What you do with Hedy will show up here, by client and by project."
            )));
        }
        mascot::print_beside(&s, Mood::Sleepy, &text);
        return Ok(());
    }

    let mut total = hedy_core::domain::UsageSample::default();
    for p in &summary.projects {
        total.add(&p.usage);
    }
    let totals = format!(
        "{} tokens · {}",
        tokens(total.total_tokens()),
        usd(total.cost_usd)
    );
    println!();
    println!(
        "  {}  {}",
        s.bold(tr!("Uso {period}", "Usage {period}")),
        s.accent(totals)
    );

    let rows = |title: String, rows: Vec<(String, u64, Option<f64>)>| {
        println!("\n{}", s.section(&title));
        let max = rows.iter().map(|r| r.1).max().unwrap_or(0).max(1);
        for (name, used, cost) in rows {
            let name = fit(&name, NAME_WIDTH);
            let bar = if s.enabled() {
                format!("{}  ", s.bar(used as f64 / max as f64, BAR_WIDTH))
            } else {
                String::new()
            };
            println!(
                "  {}  {bar}{:>8}  {:>8}",
                pad(&name, name.chars().count(), NAME_WIDTH),
                tokens(used),
                s.muted(format!("{:>8}", usd(cost))),
            );
        }
    };
    let no_client = tr!("Sin cliente", "No client");
    rows(
        tr!("Por cliente", "By client"),
        summary
            .clients
            .iter()
            .map(|c| {
                let name = c.client.clone().unwrap_or_else(|| no_client.clone());
                (name, c.usage.total_tokens(), c.usage.cost_usd)
            })
            .collect(),
    );
    rows(
        tr!("Por proyecto", "By project"),
        summary
            .projects
            .iter()
            .map(|p| (p.name.clone(), p.usage.total_tokens(), p.usage.cost_usd))
            .collect(),
    );
    let note = tr!(
        "Tokens que Hedy registró en sus tareas. El costo es una estimación: en suscripciones, \
         lo que calcula la CLI (no lo que se cobra); en API keys, según el precio de `api_price.<proveedor>`.",
        "Tokens Hedy recorded in its tasks. The cost is an estimate: for subscriptions, \
         what the CLI calculates (not what you are charged); for API keys, from the `api_price.<provider>` price."
    );
    println!();
    for line in wrap(&note, 72) {
        println!("  {}", hint(&s, &line));
    }
    println!();
    Ok(())
}

/// "3 min", "2 h 5 min", "3 d".
fn duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3_600 => format!("{} min", seconds / 60),
        3_600..86_400 => match (seconds % 3_600) / 60 {
            0 => format!("{} h", seconds / 3_600),
            minutes => format!("{} h {minutes} min", seconds / 3_600),
        },
        _ => format!("{} d", seconds / 86_400),
    }
}

fn phase_label(phase: Phase) -> String {
    match phase {
        Phase::Plan => tr!("plan", "plan"),
        Phase::Implement => tr!("implementación", "implementation"),
        Phase::Tests => tr!("tests", "tests"),
        Phase::Review => tr!("revisión", "review"),
    }
}

/// The mark and the colored label of a task's state.
fn state_mark(s: &Style, state: TaskState) -> (String, String) {
    let label = state.label();
    match state {
        TaskState::Plan | TaskState::Permission => (s.warn("◆"), s.warn(label)),
        TaskState::Limited => (s.warn("■"), s.warn(label)),
        TaskState::Working => (s.accent("▶"), s.accent(label)),
        TaskState::Review | TaskState::Answered => (s.ok("✔"), s.ok(label)),
        TaskState::Failed => (s.error("✗"), s.error(label)),
        TaskState::Accepted | TaskState::Cancelled => (s.muted("·"), s.muted(label)),
    }
}

const TASK_TITLE_WIDTH: usize = 46;

/// `hedy tasks`: what every project's AIs are doing, what needs the user first.
pub fn tasks(hedy: &Hedy) -> Result<()> {
    let s = Style::for_stdout();
    let board = hedy.activity()?;
    if board.is_empty() {
        let mut text = vec![tr!(
            "No hay tareas en curso ni de las últimas {RECENT_HOURS} h.",
            "No running tasks, and none in the last {RECENT_HOURS} h."
        )];
        if s.enabled() {
            text.push(s.muted(tr!(
                "Pídele algo a una IA desde la app de Hedy y aparecerá aquí.",
                "Ask an AI for something in the Hedy app and it will show up here."
            )));
        }
        mascot::print_beside(&s, Mood::Sleepy, &text);
        return Ok(());
    }

    let accounts = hedy.accounts()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let needs_you: Vec<&TaskActivity> = board.iter().filter(|a| a.state.needs_you()).collect();
    let working: Vec<&TaskActivity> = board
        .iter()
        .filter(|a| a.state == TaskState::Working)
        .collect();
    let recent: Vec<&TaskActivity> = board.iter().filter(|a| !a.state.is_active()).collect();

    let row = |item: &TaskActivity| {
        let (mark, label) = state_mark(&s, item.state);
        let title = fit(&item.task.title, TASK_TITLE_WIDTH);
        let title_len = title.chars().count();
        println!(
            "  {mark} {}  {label}",
            pad(&s.bold(title), title_len, TASK_TITLE_WIDTH)
        );
        let account = accounts.iter().find(|a| a.id == item.task.account);
        let who = match account {
            Some(a) => format!("{} {}", s.chip(a.color), a.name),
            None => item.task.account.to_string(),
        };
        let time = if item.state.is_active() {
            let elapsed = duration(now - item.task.created_at);
            tr!("{elapsed} en marcha", "running for {elapsed}")
        } else {
            let ago = duration(now - item.updated_at);
            tr!("hace {ago}", "{ago} ago")
        };
        let details = [
            s.muted(&item.project_name),
            who,
            s.muted(&item.agent),
            s.muted(phase_label(item.phase)),
            s.muted(time),
        ];
        println!("    {}", details.join(&s.muted(" · ")));
        if s.enabled() {
            println!();
        }
    };

    for (title, items) in [
        (tr!("Te necesitan", "Need you"), &needs_you),
        (tr!("En curso", "Running"), &working),
        (
            tr!("Últimas {RECENT_HOURS} h", "Last {RECENT_HOURS} h"),
            &recent,
        ),
    ] {
        if items.is_empty() {
            continue;
        }
        println!("\n{}", s.section(&format!("{title} ({})", items.len())));
        if s.enabled() {
            println!();
        }
        for item in items.iter() {
            row(item);
        }
    }
    if !needs_you.is_empty() {
        println!(
            "  {} {}",
            s.accent("→"),
            hint(
                &s,
                &tr!(
                    "Respóndeles desde la app de Hedy: ahí están el plan, el permiso o el límite.",
                    "Answer them in the Hedy app: the plan, the permission or the limit is there."
                )
            )
        );
        if s.enabled() {
            println!();
        }
    }
    Ok(())
}

fn account_label(s: &Style, account: &Account) -> String {
    format!(
        "{} {} {}",
        s.chip(account.color),
        s.bold(&account.name),
        s.muted(format!("({})", account.id))
    )
}

fn kind_label(account: &Account) -> String {
    let kind = match &account.credentials {
        Credentials::ApiKey => "API key".to_owned(),
        Credentials::Subscription(ConfigFolder::Default) => tr!(
            "suscripción · carpeta por defecto",
            "subscription · default folder"
        ),
        Credentials::Subscription(ConfigFolder::Managed(_)) => {
            tr!("suscripción", "subscription")
        }
    };
    format!("{} · {kind}", account.provider)
}

fn login_label(s: &Style, account: &Account, login: &LoginState) -> String {
    let text = match (login, account.is_subscription()) {
        (LoginState::LoggedIn { plan: Some(plan) }, _) => {
            tr!("✔ conectada · {plan}", "✔ signed in · {plan}")
        }
        (LoginState::LoggedIn { plan: None }, true) => tr!("✔ conectada", "✔ signed in"),
        (LoginState::LoggedIn { plan: None }, false) => tr!("✔ key guardada", "✔ key saved"),
        (LoginState::LoggedOut, true) => tr!("✗ sin sesión", "✗ signed out"),
        (LoginState::LoggedOut, false) => tr!("✗ sin key", "✗ no key"),
        (LoginState::Unknown, _) => tr!("? desconocido", "? unknown"),
    };
    match login {
        LoginState::LoggedIn { .. } => s.ok(&text),
        LoginState::LoggedOut => s.error(&text),
        LoginState::Unknown => s.warn(&text),
    }
}

/// An account in two lines: name and login on the first, details below.
struct Card {
    account: Account,
    dimmed: bool,
    login: Option<String>,
    details: Vec<String>,
    projects: Option<String>,
    /// A command that fixes something, shown with an arrow.
    fix: Option<String>,
}

fn print_cards(s: &Style, cards: &[Card]) {
    let head_len = |c: &Card| c.account.name.chars().count() + 1 + c.account.id.as_str().len();
    let width = cards.iter().map(head_len).max().unwrap_or(0);
    for card in cards {
        let a = &card.account;
        let name = if card.dimmed {
            s.muted(&a.name)
        } else {
            s.bold(&a.name)
        };
        let head = format!("{name} {}", s.muted(a.id.as_str()));
        match &card.login {
            Some(login) => println!(
                "  {} {}   {login}",
                s.chip(a.color),
                pad(&head, head_len(card), width)
            ),
            None => println!("  {} {head}", s.chip(a.color)),
        }
        println!("    {}", s.muted(card.details.join(" · ")));
        if let Some(projects) = &card.projects {
            println!("    {}", s.muted(projects));
        }
        if let Some(fix) = &card.fix {
            println!("    {} {}", s.accent("→"), s.command(fix));
        }
        if s.enabled() {
            println!();
        }
    }
}

fn status_card(
    s: &Style,
    status: &AccountStatus,
    default: Option<&hedy_core::domain::AccountId>,
) -> Card {
    let a = &status.account;
    let mut details = vec![kind_label(a)];
    if default == Some(&a.id) {
        details.push(tr!("cuenta por defecto", "default account"));
    }
    if !status.available {
        details.push(tr!("no disponible aquí", "not available here"));
    }
    let projects: Vec<String> = status
        .projects
        .iter()
        .map(|p| {
            if p.protected {
                let name = &p.name;
                tr!("{name} (protegido)", "{name} (protected)")
            } else {
                p.name.clone()
            }
        })
        .collect();
    let projects = (!projects.is_empty()).then(|| {
        let list = projects.join(", ");
        tr!("proyectos: {list}", "projects: {list}")
    });
    let fix = matches!(status.login, LoginState::LoggedOut)
        .then(|| format!("hedy account login {}", a.id));
    Card {
        account: a.clone(),
        dimmed: !status.available,
        login: Some(login_label(s, a, &status.login)),
        details,
        projects,
        fix,
    }
}

pub fn list_accounts(hedy: &Hedy) -> Result<()> {
    let s = Style::for_stdout();
    let accounts = hedy.accounts()?;
    if accounts.is_empty() {
        print_no_accounts(&s);
        return Ok(());
    }
    let default = hedy.setting(Setting::DefaultAccount)?;
    let cards: Vec<Card> = accounts
        .into_iter()
        .map(|a| {
            let mut details = vec![kind_label(&a)];
            match &a.credentials {
                Credentials::Subscription(ConfigFolder::Managed(p)) => details.push(tilde(p)),
                // The kind already says "default folder".
                Credentials::Subscription(ConfigFolder::Default) => {}
                Credentials::ApiKey => {
                    details.push(tr!("llavero del sistema", "system keyring"));
                }
            }
            if default.as_deref() == Some(a.id.as_str()) {
                details.push(tr!("cuenta por defecto", "default account"));
            }
            Card {
                account: a,
                dimmed: false,
                login: None,
                details,
                projects: None,
                fix: None,
            }
        })
        .collect();
    if s.enabled() {
        println!();
    }
    print_cards(&s, &cards);
    Ok(())
}

pub fn add_account(hedy: &Hedy, args: AddAccount) -> Result<()> {
    let s = Style::for_stdout();
    let provider = args.provider();
    let api_key = args.api.is_some();
    let account = hedy.add_account(NewAccount {
        name: args.name,
        provider,
        api_key,
        default_folder: args.default_folder,
        color: args.color,
    })?;
    let (label, kind) = (account_label(&s, &account), kind_label(&account));
    let mut text = vec![
        s.ok(tr!("✔ Cuenta creada", "✔ Account created")),
        format!("{label} {}", s.muted(format!("· {kind}"))),
    ];
    if hedy.setting(Setting::DefaultAccount)?.is_none() {
        hedy.set_setting(Setting::DefaultAccount, Some(account.id.as_str()))?;
        text.push(s.muted(tr!(
            "Es la cuenta por defecto para los proyectos sin cuenta.",
            "It is the default account for projects without one."
        )));
    }
    mascot::print_beside(&s, Mood::Happy, &text);
    if provider == Provider::Gemini && !api_key {
        println!(
            "{}",
            s.warn(tr!(
                "Aviso: Gemini CLI ya no acepta cuentas personales de Google (solo Code Assist Standard/Enterprise). Para una cuenta personal usa `--antigravity` o `--api gemini`.",
                "Warning: Gemini CLI no longer accepts personal Google accounts (only Code Assist Standard/Enterprise). For a personal account use `--antigravity` or `--api gemini`."
            ))
        );
    }
    if !args.no_login {
        login(hedy, account.id.as_str())?;
    }
    Ok(())
}

pub fn login(hedy: &Hedy, id: &str) -> Result<()> {
    let s = Style::for_stdout();
    let account = hedy.account(id)?;

    if !account.is_subscription() {
        let key = read_api_key(&account)?;
        hedy.set_api_key(id, &key)?;
        println!(
            "{}",
            s.ok(tr!(
                "✔ API key guardada en el llavero del sistema.",
                "✔ API key saved in the system keyring."
            ))
        );
        return Ok(());
    }
    if let LoginState::LoggedIn { plan } = hedy.login_state(&account)? {
        let plan = plan.map(|p| format!(" ({p})")).unwrap_or_default();
        let name = &account.name;
        println!(
            "{}",
            s.ok(tr!(
                "✔ {name} ya tiene sesión iniciada{plan}.",
                "✔ {name} is already signed in{plan}."
            ))
        );
        return Ok(());
    }
    println!("{}", s.muted(login_hint(account.provider)));
    let code = hedy.login(id)?;
    if code != Some(0) {
        bail!(
            "{}",
            tr!(
                "el login terminó con error (código {code:?})",
                "the login ended with an error (code {code:?})"
            )
        );
    }
    let name = &account.name;
    match hedy.login_state(&account)? {
        LoginState::LoggedOut => bail!(
            "{}",
            tr!(
                "la cuenta sigue sin sesión",
                "the account is still signed out"
            )
        ),
        _ => mascot::print_beside(
            &s,
            Mood::Happy,
            &[s.ok(tr!("✔ {name} lista.", "✔ {name} is ready."))],
        ),
    }
    Ok(())
}

/// What the user does in the official login of each CLI.
fn login_hint(provider: Provider) -> String {
    match provider {
        Provider::Gemini => tr!(
            "Gemini pide el login al abrirse: elige «Log in with Google» y sal con /quit.",
            "Gemini asks you to log in when it opens: choose \"Log in with Google\" and exit with /quit."
        ),
        Provider::Kimi => tr!(
            "Kimi Code se abre: escribe /login, elige «Kimi Code (OAuth)» y sal con /exit.",
            "Kimi Code opens: type /login, choose \"Kimi Code (OAuth)\" and exit with /exit."
        ),
        Provider::Antigravity => tr!(
            "Antigravity abre el login de Google: inicia sesión y pega el código que te muestra.",
            "Antigravity opens the Google login: sign in and paste the code it shows you."
        ),
        _ => tr!("Abriendo el login oficial…", "Opening the official login…"),
    }
}

fn read_api_key(account: &Account) -> Result<String> {
    if std::io::stdin().is_terminal() {
        let name = &account.name;
        Ok(rpassword::prompt_password(tr!(
            "API key de {name} (no se muestra): ",
            "API key for {name} (hidden): "
        ))?)
    } else {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line)
    }
}

pub fn remove_account(hedy: &Hedy, id: &str) -> Result<()> {
    let s = Style::for_stdout();
    let account = hedy.remove_account(id)?;
    let label = account_label(&s, &account);
    println!(
        "{} {}",
        s.ok("✔"),
        tr!("Cuenta quitada: {label}", "Account removed: {label}")
    );
    if let Credentials::Subscription(ConfigFolder::Managed(folder)) = &account.credentials {
        let folder = tilde(folder);
        println!(
            "{}",
            s.muted(tr!(
                "Su carpeta (con el login) sigue en {folder}. Bórrala tú si ya no la necesitas.",
                "Its folder (with the login) is still at {folder}. Delete it yourself if you no longer need it."
            ))
        );
    }
    Ok(())
}

pub fn rename_account(hedy: &Hedy, id: &str, name: &str) -> Result<()> {
    let s = Style::for_stdout();
    let account = hedy.rename_account(id, name)?;
    let label = account_label(&s, &account);
    println!(
        "{} {}",
        s.ok("✔"),
        tr!("Cuenta renombrada: {label}", "Account renamed: {label}")
    );
    Ok(())
}

pub fn set_default(hedy: &Hedy, id: Option<&str>) -> Result<()> {
    let s = Style::for_stdout();
    hedy.set_setting(Setting::DefaultAccount, id)?;
    match id {
        Some(id) => {
            let label = account_label(&s, &hedy.account(id)?);
            println!(
                "{} {}",
                s.ok("✔"),
                tr!("Cuenta por defecto: {label}", "Default account: {label}")
            );
        }
        None => println!("{}", tr!("Sin cuenta por defecto.", "No default account.")),
    }
    Ok(())
}

pub fn use_account(hedy: &Hedy, dir: &Path, id: &str, protect: Option<bool>) -> Result<()> {
    let s = Style::for_stdout();
    let project = hedy.use_account(dir, id, protect)?;
    let account = hedy.account(id)?;
    let name = s.bold(&project.name);
    let path = s.muted(short_path(&project.path));
    let label = account_label(&s, &account);
    let lock = if project.protected {
        s.warn(tr!(" · protegido", " · protected"))
    } else {
        String::new()
    };
    println!(
        "{} {}",
        s.ok("✔"),
        tr!(
            "{name} {path} usa {label}{lock}",
            "{name} {path} uses {label}{lock}"
        )
    );
    Ok(())
}

pub fn run(hedy: &Hedy, dir: &Path, account: Option<&str>, args: Vec<String>) -> Result<()> {
    let s = Style::for_stderr();
    let resolved = hedy.resolve_run(dir, account, args)?;
    let place = resolved
        .project
        .as_ref()
        .map(|p| {
            let name = &p.name;
            tr!(" en {name}", " in {name}")
        })
        .unwrap_or_default();
    eprintln!(
        "{} {} {} {}",
        s.accent("▶"),
        s.chip(resolved.account.color),
        s.bold(&resolved.account.name),
        s.muted(format!("· {}{place}", resolved.spec.program))
    );
    Err(hedy.exec(&resolved).into())
}

fn category_label(category: Category) -> String {
    match category {
        Category::Decision => tr!("Decisión", "Decision"),
        Category::Convention => tr!("Convención", "Convention"),
        Category::Rule => tr!("Regla", "Rule"),
        Category::Preference => tr!("Preferencia", "Preference"),
    }
}

/// Width of the `#12` column.
const ID_WIDTH: usize = 5;
const NOTE_WIDTH: usize = 68;

/// `#12  Decisión · pagos/stripe · Codex`, then the text, wrapped.
fn print_note(s: &Style, note: &Note) {
    let id = format!("#{}", note.id);
    let mut details = vec![category_label(note.category)];
    if let Some(topic) = &note.topic {
        details.push(topic.clone());
    }
    details.push(if note.author == USER_AUTHOR {
        tr!("tú", "you")
    } else {
        note.author.clone()
    });
    if note.project.is_none() {
        details.push(tr!("todos los proyectos", "all projects"));
    }
    println!(
        "  {} {}",
        pad(&s.bold(&id), id.chars().count(), ID_WIDTH),
        s.muted(details.join(" · "))
    );
    for line in wrap(&note.text, NOTE_WIDTH) {
        println!("  {} {line}", " ".repeat(ID_WIDTH));
    }
}

/// What a proposal changes, or the notes it looks like, under it.
fn print_proposal(s: &Style, proposal: &Proposal) {
    print_note(s, &proposal.note);
    let indent = " ".repeat(ID_WIDTH);
    if let Some(old) = &proposal.replaces {
        let id = old.id;
        let label = tr!("cambia #{id}, que dice:", "changes #{id}, which says:");
        println!("  {indent} {}", s.warn(label));
        for line in wrap(&old.text, NOTE_WIDTH - 2) {
            println!("  {indent}   {}", s.muted(line));
        }
    }
    for other in &proposal.similar {
        let id = other.id;
        let text = fit(&other.text, NOTE_WIDTH - 20);
        println!(
            "  {indent} {}",
            s.muted(tr!("parecida a #{id}: {text}", "similar to #{id}: {text}"))
        );
    }
}

pub fn memory_list(hedy: &Hedy, dir: &Path) -> Result<()> {
    let s = Style::for_stdout();
    let Some(project) = hedy.project_for(dir)? else {
        bail!(
            "{}",
            tr!(
                "esta carpeta no es un proyecto de Hedy; usa `hedy use <cuenta>` primero",
                "this folder is not a Hedy project; use `hedy use <account>` first"
            )
        );
    };
    let memory = hedy.memory(&project.path)?;
    let name = s.bold(&project.name);
    if memory.notes.is_empty() && memory.proposed.is_empty() {
        let mut text = vec![tr!(
            "Todavía no hay notas en la memoria de {name}.",
            "{name}'s memory has no notes yet."
        )];
        if s.enabled() {
            text.push(s.muted(tr!(
                "Tus IAs propondrán decisiones mientras trabajan: nada se guarda sin tu aprobación.",
                "Your AIs will propose decisions as they work: nothing is kept without your approval."
            )));
        }
        mascot::print_beside(&s, Mood::Sleepy, &text);
        return Ok(());
    }
    println!();
    println!(
        "  {}  {}",
        tr!("Memoria de {name}", "{name} memory"),
        s.muted(short_path(&project.path))
    );
    if !memory.proposed.is_empty() {
        println!(
            "\n{}",
            s.section(&tr!(
                "Propuestas · para aprobar",
                "Proposals · awaiting approval"
            ))
        );
        for proposal in &memory.proposed {
            print_proposal(&s, proposal);
        }
    }
    if !memory.notes.is_empty() {
        println!("\n{}", s.section(&tr!("Aprobadas", "Approved")));
        for note in &memory.notes {
            print_note(&s, note);
        }
    }
    println!();
    if !memory.proposed.is_empty() {
        let line = tr!(
            "Aprueba con `hedy memory approve <id>` o rechaza con `hedy memory reject <id>`.",
            "Approve with `hedy memory approve <id>` or reject with `hedy memory reject <id>`."
        );
        println!("  {}\n", hint(&s, &line));
    }
    Ok(())
}

pub fn memory_approve(hedy: &Hedy, id: i64) -> Result<()> {
    let s = Style::for_stdout();
    let note = hedy.approve_note(id)?;
    let text = s.muted(fit(&note.text, NOTE_WIDTH));
    let done = if note.id == id {
        tr!("Nota #{id} aprobada: {text}", "Note #{id} approved: {text}")
    } else {
        let old = note.id;
        tr!(
            "Nota #{old} actualizada: {text}",
            "Note #{old} updated: {text}"
        )
    };
    println!("{} {done}", s.ok("✔"));
    Ok(())
}

pub fn memory_reject(hedy: &Hedy, id: i64) -> Result<()> {
    let s = Style::for_stdout();
    hedy.reject_note(id)?;
    println!(
        "{} {}",
        s.ok("✔"),
        tr!(
            "Propuesta #{id} rechazada; la memoria no cambió.",
            "Proposal #{id} rejected; the memory did not change."
        )
    );
    Ok(())
}

fn project_here(hedy: &Hedy, dir: &Path) -> Result<Project> {
    match hedy.project_for(dir)? {
        Some(project) => Ok(project),
        None => bail!(
            "{}",
            tr!(
                "esta carpeta no es un proyecto de Hedy; usa `hedy use <cuenta>` primero",
                "this folder is not a Hedy project; use `hedy use <account>` first"
            )
        ),
    }
}

fn print_skill_line(s: &Style, skill: &Skill) {
    let id = format!("#{}", skill.id);
    let mut details = vec![if skill.author == USER_AUTHOR {
        tr!("tú", "you")
    } else {
        skill.author.clone()
    }];
    if skill.project.is_none() {
        details.push(tr!("todos los proyectos", "all projects"));
    }
    println!(
        "  {} {}  {}",
        pad(&s.bold(&id), id.chars().count(), ID_WIDTH),
        s.bold(&skill.name),
        s.muted(details.join(" · "))
    );
    for line in wrap(&skill.description, NOTE_WIDTH) {
        println!("  {} {line}", " ".repeat(ID_WIDTH));
    }
}

/// Tells what happened to the CLIs' skill files, warning about the user's
/// own files that were left alone.
fn print_skill_files(s: &Style, files: &SkillFiles) {
    for path in &files.written {
        let path = short_path(path);
        println!("  {}", s.muted(tr!("escrita {path}", "wrote {path}")));
    }
    for path in &files.removed {
        let path = short_path(path);
        println!("  {}", s.muted(tr!("borrada {path}", "removed {path}")));
    }
    for path in &files.skipped {
        let path = short_path(path);
        println!(
            "  {} {}",
            s.warn("!"),
            tr!(
                "{path} ya existe y no es de Hedy: no se tocó (renombra la habilidad para tener las dos)",
                "{path} already exists and is not Hedy's: left alone (rename the skill to keep both)"
            )
        );
    }
}

pub fn skills_list(hedy: &Hedy, dir: &Path) -> Result<()> {
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let view = hedy.skills(&project.path)?;
    let name = s.bold(&project.name);
    if view.approved.is_empty() && view.proposed.is_empty() {
        let mut text = vec![tr!(
            "Todavía no hay habilidades en {name}.",
            "{name} has no skills yet."
        )];
        if s.enabled() {
            text.push(s.muted(tr!(
                "Cuando una IA descubra un procedimiento reutilizable te lo propondrá: nada se usa sin tu aprobación.",
                "When an AI works out a reusable procedure it will propose it: nothing is used without your approval."
            )));
        }
        mascot::print_beside(&s, Mood::Sleepy, &text);
        return Ok(());
    }
    println!();
    println!(
        "  {}  {}",
        tr!("Habilidades de {name}", "{name} skills"),
        s.muted(short_path(&project.path))
    );
    if !view.proposed.is_empty() {
        println!(
            "\n{}",
            s.section(&tr!(
                "Propuestas · para aprobar",
                "Proposals · awaiting approval"
            ))
        );
        for proposal in &view.proposed {
            print_skill_line(&s, &proposal.skill);
            if let Some(old) = &proposal.replaces {
                let indent = " ".repeat(ID_WIDTH);
                let (id, name) = (old.id, &old.name);
                let label = tr!(
                    "actualiza #{id}; mira el cambio con `hedy skills show {name}`",
                    "updates #{id}; see the change with `hedy skills show {name}`"
                );
                println!("  {indent} {}", s.warn(label));
            }
        }
    }
    if !view.approved.is_empty() {
        println!("\n{}", s.section(&tr!("Aprobadas", "Approved")));
        for skill in &view.approved {
            print_skill_line(&s, skill);
        }
    }
    println!();
    if !view.proposed.is_empty() {
        let line = tr!(
            "Aprueba con `hedy skills approve <id>` o rechaza con `hedy skills reject <id>`.",
            "Approve with `hedy skills approve <id>` or reject with `hedy skills reject <id>`."
        );
        println!("  {}\n", hint(&s, &line));
    }
    Ok(())
}

fn print_skill_body(s: &Style, body: &str) {
    for line in body.lines() {
        println!("    {line}");
    }
    if body.is_empty() {
        println!("    {}", s.muted("—"));
    }
}

pub fn skills_show(hedy: &Hedy, dir: &Path, name: &str) -> Result<()> {
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let skill = hedy.skill_named(&project.path, name)?;
    println!();
    print_skill_line(&s, &skill);
    let status = match skill.status {
        SkillStatus::Approved => tr!("aprobada", "approved"),
        SkillStatus::Proposed => tr!("propuesta", "proposal"),
    };
    println!("  {} {}", " ".repeat(ID_WIDTH), s.muted(status));
    println!();
    print_skill_body(&s, &skill.body);
    // Proposed updates of this skill, with their new text.
    let view = hedy.skills(&project.path)?;
    for proposal in view
        .proposed
        .iter()
        .filter(|p| p.replaces.as_ref().is_some_and(|old| old.id == skill.id))
    {
        let id = proposal.skill.id;
        let author = &proposal.skill.author;
        println!(
            "\n{}",
            s.section(&tr!(
                "Actualización propuesta #{id} · {author}",
                "Proposed update #{id} · {author}"
            ))
        );
        if proposal.skill.description != skill.description {
            println!("  {}", proposal.skill.description);
        }
        print_skill_body(&s, &proposal.skill.body);
    }
    println!();
    Ok(())
}

pub fn skills_approve(hedy: &Hedy, id: i64) -> Result<()> {
    let s = Style::for_stdout();
    let change = hedy.approve_skill(id)?;
    let name = s.bold(&change.skill.name);
    let done = if change.skill.id == id {
        tr!("Habilidad {name} aprobada.", "Skill {name} approved.")
    } else {
        tr!("Habilidad {name} actualizada.", "Skill {name} updated.")
    };
    println!("{} {done}", s.ok("✔"));
    print_skill_files(&s, &change.files);
    Ok(())
}

pub fn skills_reject(hedy: &Hedy, id: i64) -> Result<()> {
    let s = Style::for_stdout();
    hedy.reject_skill(id)?;
    println!(
        "{} {}",
        s.ok("✔"),
        tr!(
            "Propuesta #{id} rechazada; las habilidades no cambiaron.",
            "Proposal #{id} rejected; the skills did not change."
        )
    );
    Ok(())
}

pub fn skills_remove(hedy: &Hedy, dir: &Path, name: &str) -> Result<()> {
    let s = Style::for_stdout();
    let project = project_here(hedy, dir)?;
    let skill = hedy.skill_named(&project.path, name)?;
    let files = hedy.remove_skill(skill.id)?;
    let name = s.bold(&skill.name);
    println!(
        "{} {}",
        s.ok("✔"),
        tr!("Habilidad {name} borrada.", "Skill {name} removed.")
    );
    print_skill_files(&s, &files);
    Ok(())
}

fn area_title(area: CheckArea) -> String {
    match area {
        CheckArea::Cli => tr!("CLIs oficiales", "Official CLIs"),
        CheckArea::Account => tr!("Cuentas", "Accounts"),
        CheckArea::Hedy => "Hedy".into(),
        CheckArea::Environment => tr!("Entorno", "Environment"),
    }
}

fn level_mark(s: &Style, level: CheckLevel) -> String {
    match level {
        CheckLevel::Ok => s.ok("✔"),
        CheckLevel::Info => s.accent("i"),
        CheckLevel::Warning => s.warn("!"),
        CheckLevel::Error => s.error("✗"),
    }
}

/// "2 avisos, 1 error"; `None` when there are neither.
fn doctor_summary(report: &DoctorReport) -> Option<String> {
    let (warnings, errors) = (
        report.count(CheckLevel::Warning),
        report.count(CheckLevel::Error),
    );
    let warnings = match warnings {
        0 => None,
        1 => Some(tr!("1 aviso", "1 warning")),
        n => Some(tr!("{n} avisos", "{n} warnings")),
    };
    let errors = match errors {
        0 => None,
        1 => Some(tr!("1 error", "1 error")),
        n => Some(tr!("{n} errores", "{n} errors")),
    };
    let parts: Vec<String> = warnings.into_iter().chain(errors).collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Widest subject shown before the message; longer ones are cut.
const SUBJECT_WIDTH: usize = 28;

/// Prints the report. Returns `false` when something is broken (an error).
pub fn doctor(hedy: &Hedy) -> Result<bool> {
    let s = Style::for_stdout();
    let report = spinner::while_running(
        &tr!(
            "Revisando las CLIs, las cuentas y Hedy…",
            "Checking the CLIs, the accounts and Hedy…"
        ),
        || hedy.doctor(),
    )?;

    let width = report
        .checks
        .iter()
        .map(|c| c.subject.chars().count())
        .max()
        .unwrap_or(0)
        .min(SUBJECT_WIDTH);
    for area in CheckArea::ALL {
        let checks: Vec<_> = report.in_area(area).collect();
        if checks.is_empty() {
            continue;
        }
        println!("\n{}", s.section(&area_title(area)));
        for check in checks {
            let subject = fit(&check.subject, width);
            let len = subject.chars().count();
            let message = match check.level {
                CheckLevel::Ok | CheckLevel::Info => s.muted(&check.message),
                CheckLevel::Warning | CheckLevel::Error => check.message.clone(),
            };
            println!(
                "  {} {}  {message}",
                level_mark(&s, check.level),
                pad(&s.bold(&subject), len, width)
            );
            if let Some(fix) = &check.fix {
                println!("    {} {}", s.accent("→"), s.command(fix));
            }
        }
    }
    println!();

    match doctor_summary(&report) {
        None => mascot::print_beside(
            &s,
            Mood::Happy,
            &[s.ok(tr!("✔ Todo en orden.", "✔ All good."))],
        ),
        Some(summary) => {
            let summary = if report.has_errors() {
                s.error(format!("✗ {summary}"))
            } else {
                s.warn(format!("! {summary}"))
            };
            println!("  {summary}");
            if s.enabled() {
                println!();
            }
        }
    }
    Ok(!report.has_errors())
}

/// The project of `dir`, or why templates for it cannot be used.
fn template_project(hedy: &Hedy, dir: &Path) -> Result<hedy_core::domain::Project> {
    match hedy.project_for(dir)? {
        Some(project) => Ok(project),
        None => bail!(
            "{}",
            tr!(
                "esta carpeta no es un proyecto de Hedy; usa `hedy use <cuenta>` primero o quita --project",
                "this folder is not a Hedy project; use `hedy use <account>` first or drop --project"
            )
        ),
    }
}

const TEMPLATE_WIDTH: usize = 68;

/// `Escribir pruebas  ejemplo · {archivo}`, then the text, wrapped.
fn print_template(s: &Style, template: &PromptTemplate) {
    let mut details = Vec::new();
    if template.builtin.is_some() {
        details.push(tr!("ejemplo", "example"));
    }
    let placeholders: Vec<String> = template
        .placeholders()
        .iter()
        .map(|p| format!("{{{p}}}"))
        .collect();
    if !placeholders.is_empty() {
        details.push(placeholders.join(" "));
    }
    println!(
        "  {}  {}",
        s.bold(&template.name),
        s.muted(details.join(" · "))
    );
    for line in wrap(&template.text, TEMPLATE_WIDTH) {
        println!("    {}", s.muted(line));
    }
}

pub fn template_list(hedy: &Hedy, dir: &Path) -> Result<()> {
    let s = Style::for_stdout();
    let project = hedy.project_for(dir)?;
    let templates = hedy.templates(TemplateScope::For(
        project.as_ref().map(|p| p.path.as_path()),
    ))?;
    if templates.is_empty() {
        let text = vec![
            tr!("Todavía no hay plantillas.", "There are no templates yet."),
            s.muted(tr!(
                "Agrega una con `hedy template add <nombre> --text \"…\"`.",
                "Add one with `hedy template add <name> --text \"…\"`."
            )),
        ];
        mascot::print_beside(&s, Mood::Sleepy, &text);
        return Ok(());
    }
    let (own, global): (Vec<_>, Vec<_>) = templates.iter().partition(|t| t.project.is_some());
    println!();
    if let Some(project) = &project
        && !own.is_empty()
    {
        let name = &project.name;
        println!(
            "{}",
            s.section(&tr!("Del proyecto {name}", "For project {name}"))
        );
        for template in own {
            print_template(&s, template);
        }
        println!();
    }
    if !global.is_empty() {
        println!(
            "{}",
            s.section(&tr!("Para todos los proyectos", "For every project"))
        );
        for template in global {
            print_template(&s, template);
        }
        println!();
    }
    let line = tr!(
        "En el chat de Hedy, escribe / al empezar un mensaje para usarlas.",
        "In Hedy's chat, type / at the start of a message to use them."
    );
    println!("  {}\n", hint(&s, &line));
    Ok(())
}

pub fn template_add(
    hedy: &Hedy,
    dir: &Path,
    name: &str,
    for_project: bool,
    text: &str,
) -> Result<()> {
    let s = Style::for_stdout();
    let project = if for_project {
        Some(template_project(hedy, dir)?)
    } else {
        None
    };
    let template = hedy.add_template(NewTemplate {
        project: project.as_ref().map(|p| p.path.clone()),
        name: name.into(),
        text: text.into(),
    })?;
    let name = s.bold(&template.name);
    let done = match &project {
        Some(project) => {
            let project = &project.name;
            tr!(
                "Plantilla {name} guardada para {project}.",
                "Template {name} saved for {project}."
            )
        }
        None => tr!(
            "Plantilla {name} guardada para todos los proyectos.",
            "Template {name} saved for every project."
        ),
    };
    println!("{} {done}", s.ok("✔"));
    let placeholders = template.placeholders();
    if !placeholders.is_empty() {
        let list = placeholders
            .iter()
            .map(|p| format!("{{{p}}}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {}",
            s.muted(tr!(
                "Al usarla, Hedy te pedirá: {list}",
                "When you use it, Hedy will ask you for: {list}"
            ))
        );
    }
    Ok(())
}

pub fn template_remove(hedy: &Hedy, dir: &Path, name: &str, for_project: bool) -> Result<()> {
    let s = Style::for_stdout();
    let project = if for_project {
        Some(template_project(hedy, dir)?.path)
    } else {
        None
    };
    let template = hedy
        .templates(TemplateScope::All)?
        .into_iter()
        .find(|t| t.project == project && hedy_core::domain::template::same_name(&t.name, name))
        .ok_or_else(|| {
            hedy_core::application::AppError::TemplateNotFound(name.trim().to_owned())
        })?;
    hedy.remove_template(&template.id)?;
    let name = s.bold(&template.name);
    println!(
        "{} {}",
        s.ok("✔"),
        tr!("Plantilla {name} borrada.", "Template {name} deleted.")
    );
    Ok(())
}

pub fn config_get(hedy: &Hedy, key: &str) -> Result<()> {
    match hedy.setting(key.parse()?)? {
        Some(v) => println!("{v}"),
        None => println!(
            "{}",
            Style::for_stdout().muted(tr!("(sin valor)", "(no value)"))
        ),
    }
    Ok(())
}

/// `None` clears the setting.
pub fn config_set(hedy: &Hedy, key: &str, value: Option<&str>) -> Result<()> {
    Ok(hedy.set_setting(key.parse()?, value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_wrapped_and_cut() {
        assert_eq!(
            wrap("uno dos tres cuatro", 8),
            ["uno dos", "tres", "cuatro"]
        );
        assert_eq!(fit("tienda-online", 20), "tienda-online");
        assert_eq!(fit("tienda-online", 7), "tienda…");
    }

    #[test]
    fn doctor_summary_counts_warnings_and_errors() {
        use hedy_core::application::Check;
        let check = |level| Check {
            area: CheckArea::Cli,
            subject: "x".into(),
            level,
            message: String::new(),
            fix: None,
        };
        let report = |levels: &[CheckLevel]| DoctorReport {
            checks: levels.iter().map(|l| check(*l)).collect(),
        };
        assert_eq!(
            doctor_summary(&report(&[CheckLevel::Ok, CheckLevel::Info])),
            None
        );
        // "2 avisos, 1 error" or "2 warnings, 1 error": another test may
        // switch the process-wide language.
        let summary = doctor_summary(&report(&[
            CheckLevel::Warning,
            CheckLevel::Warning,
            CheckLevel::Error,
        ]))
        .unwrap();
        assert!(
            summary.starts_with("2 ") && summary.ends_with(", 1 error"),
            "{summary}"
        );
    }

    #[test]
    fn durations_read_short() {
        assert_eq!(duration(42), "42 s");
        assert_eq!(duration(185), "3 min");
        assert_eq!(duration(3_600), "1 h");
        assert_eq!(duration(7_500), "2 h 5 min");
        assert_eq!(duration(200_000), "2 d");
        assert_eq!(duration(-5), "0 s");
    }

    #[test]
    fn hints_keep_their_backticks_without_colors() {
        assert_eq!(hint(&Style::plain(), "usa `hedy run`"), "usa `hedy run`");
    }
}
