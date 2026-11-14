//! One function per command. They call use cases on [`Hedy`] and render results.

use std::io::{BufRead, IsTerminal};
use std::path::Path;

use anyhow::{Result, bail};
use hedy_core::application::{AccountStatus, Hedy, NewAccount, Setting};
use hedy_core::domain::{Account, ConfigFolder, Credentials, LoginState, Provider};

use crate::cli::AddAccount;
use crate::style::{Style, pad, tilde};

/// What to do when there are no accounts yet.
fn print_no_accounts(s: &Style) {
    println!("Todavía no hay cuentas. Agrega la primera, por ejemplo:");
    println!(
        "  {}",
        s.accent("hedy account add \"Claude personal\" --claude")
    );
    println!(
        "  {}",
        s.accent("hedy account add \"Claude trabajo\" --claude --default-folder")
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
                " (protegido)"
            } else {
                ""
            };
            println!(
                "{} {}  {}{}",
                s.bold("Proyecto"),
                s.bold(&project.name),
                s.muted(tilde(&project.path)),
                s.warn(lock)
            );
            match project.account.as_ref().and_then(find) {
                Some(a) => println!("{}   {}", s.bold("Cuenta"), account_label(&s, &a.account)),
                None => println!(
                    "{}   {}",
                    s.bold("Cuenta"),
                    s.muted("ninguna (usa `hedy use <cuenta>`)")
                ),
            }
        }
        None => {
            println!("{}", s.muted("Esta carpeta no es un proyecto de Hedy."));
            match status.default_account.as_ref().and_then(find) {
                Some(a) => println!(
                    "{}  {}",
                    s.bold("Cuenta por defecto"),
                    account_label(&s, &a.account)
                ),
                None => println!(
                    "{}",
                    s.muted("Sin cuenta por defecto (usa `hedy account default <cuenta>`).")
                ),
            }
        }
    }

    println!("\n{}", s.bold("Cuentas"));
    print_accounts(&s, &status.accounts, status.default_account.as_ref());
    println!(
        "\n{}",
        s.muted("Uso por proyecto y cliente: `hedy resumen`.")
    );
    Ok(())
}

pub fn set_client(hedy: &Hedy, dir: &Path, client: Option<&str>) -> Result<()> {
    let s = Style::for_stdout();
    let Some(project) = hedy.project_for(dir)? else {
        bail!("esta carpeta no es un proyecto de Hedy; usa `hedy use <cuenta>` primero");
    };
    let project = hedy.set_project_client(&project.path, client)?;
    match &project.client {
        Some(client) => println!("{} es para {}", s.bold(&project.name), s.bold(client)),
        None => println!("{} ya no tiene cliente", s.bold(&project.name)),
    }
    Ok(())
}

/// "182 k" / "1,2 M" tokens.
fn tokens(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{} k", n / 1_000),
        _ => format!("{:.1} M", n as f64 / 1_000_000.0).replace('.', ","),
    }
}

fn usd(cost: Option<f64>) -> String {
    cost.map_or_else(|| "—".into(), |c| format!("${c:.2}").replace('.', ","))
}

pub fn summary(hedy: &Hedy, days: u32) -> Result<()> {
    let s = Style::for_stdout();
    let summary = hedy.usage_summary(days)?;
    let period = if days == 7 {
        "esta semana".to_owned()
    } else {
        format!("los últimos {days} días")
    };
    if summary.projects.is_empty() {
        println!("{}", s.muted(format!("Sin uso registrado {period}.")));
        return Ok(());
    }
    let row = |name: &str, tokens_text: String, cost: String| {
        let len = name.chars().count();
        println!("  {} {:>9}  {:>8}", pad(name, len, 28), tokens_text, cost);
    };
    println!("{} {period}", s.bold("Uso por cliente"));
    for c in &summary.clients {
        let name = c.client.as_deref().unwrap_or("Sin cliente");
        row(name, tokens(c.usage.total_tokens()), usd(c.usage.cost_usd));
    }
    println!("\n{}", s.bold("Por proyecto"));
    for p in &summary.projects {
        row(
            &p.name,
            tokens(p.usage.total_tokens()),
            usd(p.usage.cost_usd),
        );
    }
    println!(
        "\n{}",
        s.muted(
            "Tokens que Hedy registró en sus tareas. El costo es una estimación: en suscripciones, \
             lo que calcula la CLI (no lo que se cobra); en API keys, según el precio de `api_price.<proveedor>`."
        )
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
        Credentials::ApiKey => "API key",
        Credentials::Subscription(ConfigFolder::Default) => "suscripción · carpeta por defecto",
        Credentials::Subscription(ConfigFolder::Managed(_)) => "suscripción",
    };
    format!("{} · {kind}", account.provider)
}

fn login_label(s: &Style, account: &Account, login: &LoginState) -> (String, usize) {
    let text = match (login, account.is_subscription()) {
        (LoginState::LoggedIn { plan: Some(plan) }, _) => format!("✔ conectada · {plan}"),
        (LoginState::LoggedIn { plan: None }, true) => "✔ conectada".to_owned(),
        (LoginState::LoggedIn { plan: None }, false) => "✔ key guardada".to_owned(),
        (LoginState::LoggedOut, true) => "✗ sin sesión".to_owned(),
        (LoginState::LoggedOut, false) => "✗ sin key".to_owned(),
        (LoginState::Unknown, _) => "? desconocido".to_owned(),
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
                    format!("{} (protegido)", p.name)
                } else {
                    p.name.clone()
                }
            })
            .collect();
        if default == Some(&a.id) {
            notes.insert(0, "por defecto".into());
        }
        if !status.available {
            notes.push("no disponible aquí".into());
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
                "carpeta por defecto de la CLI".into()
            }
            Credentials::ApiKey => "llavero del sistema".into(),
        };
        let mark = if default.as_deref() == Some(a.id.as_str()) {
            " · por defecto"
        } else {
            ""
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
    println!(
        "Cuenta creada: {} · {}",
        account_label(&s, &account),
        kind_label(&account)
    );

    if hedy.setting(Setting::DefaultAccount)?.is_none() {
        hedy.set_setting(Setting::DefaultAccount, Some(account.id.as_str()))?;
        println!(
            "{}",
            s.muted("Es la cuenta por defecto para los proyectos sin cuenta.")
        );
    }
    if provider == Provider::Gemini && !api_key {
        println!(
            "{}",
            s.warn("Aviso: Gemini CLI ya no acepta cuentas personales de Google (solo Code Assist Standard/Enterprise). Para una cuenta personal usa `--api gemini`.")
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
        println!("{}", s.ok("✔ API key guardada en el llavero del sistema."));
        return Ok(());
    }
    if let LoginState::LoggedIn { plan } = hedy.login_state(&account)? {
        let plan = plan.map(|p| format!(" ({p})")).unwrap_or_default();
        println!(
            "{}",
            s.ok(format!(
                "✔ {} ya tiene sesión iniciada{plan}.",
                account.name
            ))
        );
        return Ok(());
    }
    match account.provider {
        Provider::Gemini => println!(
            "{}",
            s.muted("Gemini pide el login al abrirse: elige «Log in with Google» y sal con /quit.")
        ),
        _ => println!("{}", s.muted("Abriendo el login oficial…")),
    }
    let code = hedy.login(id)?;
    if code != Some(0) {
        bail!("el login terminó con error (código {code:?})");
    }
    match hedy.login_state(&account)? {
        LoginState::LoggedOut => bail!("la cuenta sigue sin sesión"),
        _ => println!("{}", s.ok(format!("✔ {} lista.", account.name))),
    }
    Ok(())
}

fn read_api_key(account: &Account) -> Result<String> {
    if std::io::stdin().is_terminal() {
        Ok(rpassword::prompt_password(format!(
            "API key de {} (no se muestra): ",
            account.name
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
    println!("Cuenta quitada: {}", account_label(&s, &account));
    if let Credentials::Subscription(ConfigFolder::Managed(folder)) = &account.credentials {
        println!(
            "{}",
            s.muted(format!(
                "Su carpeta (con el login) sigue en {}. Bórrala tú si ya no la necesitas.",
                tilde(folder)
            ))
        );
    }
    Ok(())
}

pub fn set_default(hedy: &Hedy, id: Option<&str>) -> Result<()> {
    let s = Style::for_stdout();
    hedy.set_setting(Setting::DefaultAccount, id)?;
    match id {
        Some(id) => println!(
            "Cuenta por defecto: {}",
            account_label(&s, &hedy.account(id)?)
        ),
        None => println!("Sin cuenta por defecto."),
    }
    Ok(())
}

pub fn use_account(hedy: &Hedy, dir: &Path, id: &str, protect: Option<bool>) -> Result<()> {
    let s = Style::for_stdout();
    let project = hedy.use_account(dir, id, protect)?;
    let account = hedy.account(id)?;
    println!(
        "{} {} usa {}{}",
        s.bold(&project.name),
        s.muted(tilde(&project.path)),
        account_label(&s, &account),
        if project.protected {
            s.warn(" · protegido")
        } else {
            String::new()
        }
    );
    Ok(())
}

pub fn run(hedy: &Hedy, dir: &Path, account: Option<&str>, args: Vec<String>) -> Result<()> {
    let s = Style::for_stderr();
    let resolved = hedy.resolve_run(dir, account, args)?;
    let place = resolved
        .project
        .as_ref()
        .map(|p| format!(" en {}", p.name))
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
        None => println!("{}", Style::for_stdout().muted("(sin valor)")),
    }
    Ok(())
}

/// `None` clears the setting.
pub fn config_set(hedy: &Hedy, key: &str, value: Option<&str>) -> Result<()> {
    Ok(hedy.set_setting(key.parse()?, value)?)
}
