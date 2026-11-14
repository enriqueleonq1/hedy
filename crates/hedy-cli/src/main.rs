//! `hedy`: driving adapter over `hedy-core`.

mod cli;
mod commands;
mod mascot;
mod spinner;
mod style;

use std::process::ExitCode;

use anyhow::Result;
use hedy_core::adapters::{self, HedyPaths};

use cli::{AccountCommand, Cli, Command, ConfigCommand, MemoryCommand};
use style::Style;

fn main() -> ExitCode {
    // The help is shown while parsing, before the database is open: it follows
    // `HEDY_LANGUAGE` or the system locale. Once `adapters::system` opens the
    // database, the `ui.language` setting (if any) applies to everything else.
    hedy_core::i18n::init(None);
    match run(Cli::parse_args()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{} {err}", Style::for_stderr().error("✗ error:"));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let command = cli.command.unwrap_or(Command::Status);
    if let Command::Completions { shell } = command {
        let mut script = Vec::new();
        clap_complete::generate(shell, &mut Cli::full_command(), "hedy", &mut script);
        write_stdout(&script)?;
        return Ok(ExitCode::SUCCESS);
    }

    let dir = match cli.dir {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };
    let hedy = adapters::system(&HedyPaths::detect()?)?;

    // The only command whose findings (not a failure) set the exit code.
    if let Command::Doctor = command {
        let healthy = commands::doctor(&hedy)?;
        return Ok(if healthy {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }

    match command {
        Command::Status => commands::status(&hedy, &dir),
        Command::Use {
            account,
            protect,
            unprotect,
        } => {
            let protection = match (protect, unprotect) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            };
            commands::use_account(&hedy, &dir, &account, protection)
        }
        Command::Tasks => commands::tasks(&hedy),
        Command::Summary { week: _, days } => commands::summary(&hedy, days.unwrap_or(7)),
        Command::Client { name, clear } => {
            commands::set_client(&hedy, &dir, if clear { None } else { name.as_deref() })
        }
        Command::Run { account, args } => commands::run(&hedy, &dir, account.as_deref(), args),
        Command::Account(sub) => match sub {
            AccountCommand::Add(args) => commands::add_account(&hedy, args),
            AccountCommand::List => commands::list_accounts(&hedy),
            AccountCommand::Login { account } => commands::login(&hedy, &account),
            AccountCommand::Remove { account } => commands::remove_account(&hedy, &account),
            AccountCommand::Default { account, clear } => {
                commands::set_default(&hedy, if clear { None } else { account.as_deref() })
            }
        },
        Command::Memory { command } => match command.unwrap_or(MemoryCommand::List) {
            MemoryCommand::List => commands::memory_list(&hedy, &dir),
            MemoryCommand::Approve { id } => commands::memory_approve(&hedy, id),
            MemoryCommand::Reject { id } => commands::memory_reject(&hedy, id),
        },
        Command::Config(sub) => match sub {
            ConfigCommand::Get { key } => commands::config_get(&hedy, &key),
            ConfigCommand::Set { key, value } => commands::config_set(&hedy, &key, Some(&value)),
            ConfigCommand::Unset { key } => commands::config_set(&hedy, &key, None),
        },
        Command::Completions { .. } | Command::Doctor => unreachable!("handled above"),
    }?;
    Ok(ExitCode::SUCCESS)
}

/// Writes to stdout, treating a closed pipe (`hedy … | head`) as success.
fn write_stdout(bytes: &[u8]) -> Result<()> {
    use std::io::{ErrorKind, Write};
    match std::io::stdout().write_all(bytes) {
        Err(e) if e.kind() != ErrorKind::BrokenPipe => Err(e.into()),
        _ => Ok(()),
    }
}
