//! One function per command. They call use cases on [`Hedy`] and render results.

use std::io::{BufRead, IsTerminal};
use std::path::Path;

use anyhow::{Result, bail};
use chrono::Timelike;
use hedy_core::application::{AccountStatus, Hedy, NewAccount, Setting};
use hedy_core::domain::{Account, ConfigFolder, Credentials, LoginState, Provider};
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
            "`hedy resumen` muestra cuánto usaste, por cliente y por proyecto.",
            "`hedy summary` shows how much you used, by client and by project."
        ),
        tr!(
            "`hedy client <nombre>` suma el costo de tus proyectos por cliente.",
            "`hedy client <name>` adds up the cost of your projects by client."
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
    fn hints_keep_their_backticks_without_colors() {
        assert_eq!(hint(&Style::plain(), "usa `hedy run`"), "usa `hedy run`");
    }
}
