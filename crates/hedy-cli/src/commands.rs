//! One function per command. They call use cases on [`Hedy`] and render results.

use std::io::{BufRead, IsTerminal};
use std::path::Path;

use anyhow::{Result, bail};
use hedy_core::application::{AccountStatus, Hedy, NewAccount, Setting};
use hedy_core::domain::{Account, ConfigFolder, Credentials, LoginState, Provider};
use hedy_core::i18n::{self, Language};
use hedy_core::tr;

use crate::cli::AddAccount;
use crate::style::{Style, pad, tilde};

/// What to do when there are no accounts yet.
fn print_no_accounts(s: &Style) {
    println!(
        "{}",
        tr!(
            "Todavía no hay cuentas. Agrega la primera, por ejemplo:",
            "No accounts yet. Add the first one, for example:"
        )
    );
    let personal = tr!("Claude personal", "Personal Claude");
    let work = tr!("Claude trabajo", "Work Claude");
    println!(
        "  {}",
        s.accent(format!("hedy account add \"{personal}\" --claude"))
    );
    println!(
        "  {}",
        s.accent(format!(
            "hedy account add \"{work}\" --claude --default-folder"
        ))
    );
}

pub fn status(hedy: &Hedy, dir: &Path) -> Result<()> {
    let s = Style::for_stdout();
    let status = hedy.status(dir)?;

    if status.accounts.is_empty() {
        print_no_accounts(&s);
        return Ok(());
    }

    let find =
        |id: &hedy_core::domain::AccountId| status.accounts.iter().find(|a| &a.account.id == id);
    match &status.current_project {
        Some(project) => {
            let lock = if project.protected {
                tr!(" (protegido)", " (protected)")
            } else {
                String::new()
            };
            // The two titles are aligned.
            let titles = [tr!("Proyecto", "Project"), tr!("Cuenta", "Account")];
            let width = titles.iter().map(|t| t.chars().count()).max().unwrap_or(0);
            let [project_title, account_title] =
                titles.map(|t| pad(&s.bold(&t), t.chars().count(), width));
            println!(
                "{project_title} {}  {}{}",
                s.bold(&project.name),
                s.muted(tilde(&project.path)),
                s.warn(lock)
            );
            let account = match project.account.as_ref().and_then(find) {
                Some(a) => account_label(&s, &a.account),
                None => s.muted(tr!(
                    "ninguna (usa `hedy use <cuenta>`)",
                    "none (use `hedy use <account>`)"
                )),
            };
            println!("{account_title} {account}");
        }
        None => {
            println!(
                "{}",
                s.muted(tr!(
                    "Esta carpeta no es un proyecto de Hedy.",
                    "This folder is not a Hedy project."
                ))
            );
            match status.default_account.as_ref().and_then(find) {
                Some(a) => println!(
                    "{}  {}",
                    s.bold(tr!("Cuenta por defecto", "Default account")),
                    account_label(&s, &a.account)
                ),
                None => println!(
                    "{}",
                    s.muted(tr!(
                        "Sin cuenta por defecto (usa `hedy account default <cuenta>`).",
                        "No default account (use `hedy account default <account>`)."
                    ))
                ),
            }
        }
    }

    println!("\n{}", s.bold(tr!("Cuentas", "Accounts")));
    print_accounts(&s, &status.accounts, status.default_account.as_ref());
    println!(
        "\n{}",
        s.muted(tr!(
            "Uso por proyecto y cliente: `hedy resumen`.",
            "Usage by project and client: `hedy summary`."
        ))
    );
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
                "{}",
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

pub fn summary(hedy: &Hedy, days: u32) -> Result<()> {
    let s = Style::for_stdout();
    let summary = hedy.usage_summary(days)?;
    let period = if days == 7 {
        tr!("esta semana", "this week")
    } else {
        tr!("los últimos {days} días", "in the last {days} days")
    };
    if summary.projects.is_empty() {
        println!(
            "{}",
            s.muted(tr!(
                "Sin uso registrado {period}.",
                "No usage recorded {period}."
            ))
        );
        return Ok(());
    }
    let row = |name: &str, tokens_text: String, cost: String| {
        let len = name.chars().count();
        println!("  {} {:>9}  {:>8}", pad(name, len, 28), tokens_text, cost);
    };
    println!(
        "{} {period}",
        s.bold(tr!("Uso por cliente", "Usage by client"))
    );
    let no_client = tr!("Sin cliente", "No client");
    for c in &summary.clients {
        let name = c.client.as_deref().unwrap_or(&no_client);
        row(name, tokens(c.usage.total_tokens()), usd(c.usage.cost_usd));
    }
    println!("\n{}", s.bold(tr!("Por proyecto", "By project")));
    for p in &summary.projects {
        row(
            &p.name,
            tokens(p.usage.total_tokens()),
            usd(p.usage.cost_usd),
        );
    }
    println!(
        "\n{}",
        s.muted(tr!(
            "Tokens que Hedy registró en sus tareas. El costo es una estimación: en suscripciones, \
             lo que calcula la CLI (no lo que se cobra); en API keys, según el precio de `api_price.<proveedor>`.",
            "Tokens Hedy recorded in its tasks. The cost is an estimate: for subscriptions, \
             what the CLI calculates (not what you are charged); for API keys, from the `api_price.<provider>` price."
        ))
    );
    Ok(())
}

fn account_label(s: &Style, account: &Account) -> String {
    format!(
        "{} {} {}",
        s.chip(account.color),
        account.name,
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

fn login_label(s: &Style, account: &Account, login: &LoginState) -> (String, usize) {
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
    let len = text.chars().count();
    let painted = match login {
        LoginState::LoggedIn { .. } => s.ok(&text),
        LoginState::LoggedOut => s.error(&text),
        LoginState::Unknown => s.warn(&text),
    };
    (painted, len)
}

fn print_accounts(
    s: &Style,
    accounts: &[AccountStatus],
    default: Option<&hedy_core::domain::AccountId>,
) {
    let name_w = accounts
        .iter()
        .map(|a| a.account.name.chars().count())
        .max()
        .unwrap_or(0);
    let id_w = accounts
        .iter()
        .map(|a| a.account.id.as_str().len())
        .max()
        .unwrap_or(0);
    let kind_w = accounts
        .iter()
        .map(|a| kind_label(&a.account).chars().count())
        .max()
        .unwrap_or(0);
    let logins: Vec<_> = accounts
        .iter()
        .map(|a| login_label(s, &a.account, &a.login))
        .collect();
    let login_w = logins.iter().map(|(_, len)| *len).max().unwrap_or(0);

    for (status, (login, login_len)) in accounts.iter().zip(logins) {
        let a = &status.account;
        let mut notes: Vec<String> = status
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
        if default == Some(&a.id) {
            notes.insert(0, tr!("por defecto", "default"));
        }
        if !status.available {
            notes.push(tr!("no disponible aquí", "not available here"));
        }
        let name = if status.available {
            a.name.clone()
        } else {
            s.muted(&a.name)
        };
        println!(
            "  {} {}  {}  {}  {}  {}",
            s.chip(a.color),
            pad(&name, a.name.chars().count(), name_w),
            s.muted(pad(a.id.as_str(), a.id.as_str().len(), id_w)),
            pad(&kind_label(a), kind_label(a).chars().count(), kind_w),
            pad(&login, login_len, login_w),
            s.muted(notes.join(" · ")),
        );
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
    for a in accounts {
        let folder = match &a.credentials {
            Credentials::Subscription(ConfigFolder::Managed(p)) => tilde(p),
            Credentials::Subscription(ConfigFolder::Default) => {
                tr!("carpeta por defecto de la CLI", "the CLI's default folder")
            }
            Credentials::ApiKey => tr!("llavero del sistema", "system keyring"),
        };
        let mark = if default.as_deref() == Some(a.id.as_str()) {
            tr!(" · por defecto", " · default")
        } else {
            String::new()
        };
        println!(
            "{}  {}  {}{}",
            account_label(&s, &a),
            kind_label(&a),
            s.muted(folder),
            s.muted(mark)
        );
    }
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
    println!(
        "{}",
        tr!(
            "Cuenta creada: {label} · {kind}",
            "Account created: {label} · {kind}"
        )
    );

    if hedy.setting(Setting::DefaultAccount)?.is_none() {
        hedy.set_setting(Setting::DefaultAccount, Some(account.id.as_str()))?;
        println!(
            "{}",
            s.muted(tr!(
                "Es la cuenta por defecto para los proyectos sin cuenta.",
                "It is the default account for projects without one."
            ))
        );
    }
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
        _ => println!("{}", s.ok(tr!("✔ {name} lista.", "✔ {name} is ready."))),
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
        "{}",
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
                "{}",
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
    let path = s.muted(tilde(&project.path));
    let label = account_label(&s, &account);
    let lock = if project.protected {
        s.warn(tr!(" · protegido", " · protected"))
    } else {
        String::new()
    };
    println!(
        "{}",
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
        "{} {} {}",
        s.chip(resolved.account.color),
        resolved.account.name,
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
