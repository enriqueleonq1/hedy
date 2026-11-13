//! `hedy`: driving adapter over `hedy-core`.

mod cli;
mod commands;
mod style;

use std::process::ExitCode;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use hedy_core::adapters::{self, HedyPaths};

use cli::{AccountCommand, Cli, Command, ConfigCommand};
use style::Style;

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{} {err}", Style::for_stderr().error("error:"));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let command = cli.command.unwrap_or(Command::Status);
    if let Command::Completions { shell } = command {
        let mut script = Vec::new();
        clap_complete::generate(shell, &mut Cli::command(), "hedy", &mut script);
        return write_stdout(&script);
    }

    let dir = match cli.dir {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };
    let hedy = adapters::system(&HedyPaths::detect()?)?;

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
        Command::Config(sub) => match sub {
            ConfigCommand::Get { key } => commands::config_get(&hedy, &key),
            ConfigCommand::Set { key, value } => commands::config_set(&hedy, &key, Some(&value)),
            ConfigCommand::Unset { key } => commands::config_set(&hedy, &key, None),
        },
        Command::Completions { .. } => unreachable!("handled above"),
    }
}

/// Writes to stdout, treating a closed pipe (`hedy … | head`) as success.
fn write_stdout(bytes: &[u8]) -> Result<()> {
    use std::io::{ErrorKind, Write};
    match std::io::stdout().write_all(bytes) {
        Err(e) if e.kind() != ErrorKind::BrokenPipe => Err(e.into()),
        _ => Ok(()),
    }
}
