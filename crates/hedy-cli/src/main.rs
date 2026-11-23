//! `hedy`: driving adapter over `hedy-core`.

mod ask;
mod cli;
mod commands;
mod commit;
mod flow_edit;
mod flow_run;
mod mascot;
mod memory_ask;
mod review;
mod skill_transfer;
mod skill_trial;
mod slash;
mod spinner;
mod style;
mod sync;
mod team;
mod usage;

use std::process::ExitCode;

use anyhow::Result;
use hedy_core::adapters::{self, HedyPaths};

use cli::{
    AccountCommand, Cli, Command, ConfigCommand, FlowCommand, MemoryCommand, RolesCommand,
    SkillsCommand, SyncCommand, TeamCommand, TemplateCommand,
};
use style::Style;

fn main() -> ExitCode {
    // `hedy … | head`: a print to a closed pipe ends the command without a
    // panic message. Only the message: SIGPIPE stays ignored, so a pipe to
    // an agent that closes still reads as an error the command handles.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if !(message.starts_with("failed printing to std") && message.contains("Broken pipe")) {
            default_hook(info);
        }
    }));
    // Claude Code runs this binary as its `PreToolUse` hook for `hedy ask`.
    if std::env::args().skip(1).eq(adapters::hook::HOOK_ARGS) {
        return ExitCode::from(u8::try_from(adapters::hook::run()).unwrap_or(1));
    }
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
    let paths = HedyPaths::detect()?;
    // Every command that runs an agent (`ask`, `memory ask`, `commit`…) does
    // so with the hook that keeps it off protected files, as in the app.
    let hedy = adapters::system_with_hook(&paths, &std::env::current_exe()?)?;
    if let Command::Ask(args) = command {
        return ask::ask(&hedy, args);
    }

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
        Command::New {
            name,
            parent,
            account,
            protect,
        } => commands::new_project(&hedy, &dir, &name, parent, account, protect),
        Command::Tasks => commands::tasks(&hedy),
        Command::Summary { week: _, days } => commands::summary(&hedy, days.unwrap_or(7)),
        Command::Client { name, clear } => {
            commands::set_client(&hedy, &dir, if clear { None } else { name.as_deref() })
        }
        Command::Permissions { mode } => commands::permissions(&hedy, &dir, mode.as_deref()),
        Command::Block {
            patterns,
            remove,
            usual,
            clear,
        } => commands::block(&hedy, &dir, &patterns, remove, usual, clear),
        Command::Run { account, yes, args } => {
            commands::run(&hedy, &dir, account.as_deref(), yes, args)
        }
        Command::Usage {
            cached,
            refresh,
            short,
            json,
        } => usage::usage(
            &hedy,
            usage::UsageOptions {
                cached,
                refresh,
                short,
                json,
            },
        ),
        Command::Commands { account, json } => {
            slash::commands(&hedy, &dir, account.as_deref(), json)
        }
        Command::Review {
            revs,
            staged,
            task,
            account,
            model,
            json,
        } => review::review(
            &hedy,
            &dir,
            review::ReviewOptions {
                revs,
                staged,
                task,
                account,
                model,
                json,
            },
        ),
        Command::Commit {
            account,
            model,
            print,
            yes,
        } => commit::commit(
            &hedy,
            &dir,
            commit::CommitOptions {
                account: account.as_deref(),
                model: model.as_deref(),
                print,
                yes,
            },
        ),
        Command::Account(sub) => match sub {
            AccountCommand::Add(args) => commands::add_account(&hedy, args),
            AccountCommand::List => commands::list_accounts(&hedy),
            AccountCommand::Login { account } => commands::login(&hedy, &account),
            AccountCommand::Remove { account } => commands::remove_account(&hedy, &account),
            AccountCommand::Rename { account, name } => {
                commands::rename_account(&hedy, &account, &name)
            }
            AccountCommand::Default { account, clear } => {
                commands::set_default(&hedy, if clear { None } else { account.as_deref() })
            }
        },
        Command::Memory { command } => match command.unwrap_or(MemoryCommand::List) {
            MemoryCommand::List => commands::memory_list(&hedy, &dir),
            MemoryCommand::Ask(args) => memory_ask::memory_ask(&hedy, &dir, args),
            MemoryCommand::Approve { id } => commands::memory_approve(&hedy, id),
            MemoryCommand::Reject { id } => {
                commands::refuse_in_agent_session()?;
                commands::memory_reject(&hedy, id)
            }
        },
        Command::Skills { command } => match command.unwrap_or(SkillsCommand::List) {
            SkillsCommand::List => commands::skills_list(&hedy, &dir),
            SkillsCommand::Show { name } => commands::skills_show(&hedy, &dir, &name),
            SkillsCommand::Approve { id } => commands::skills_approve(&hedy, id),
            SkillsCommand::Reject { id } => {
                commands::refuse_in_agent_session()?;
                commands::skills_reject(&hedy, id)
            }
            SkillsCommand::Remove { name } => {
                commands::refuse_in_agent_session()?;
                commands::skills_remove(&hedy, &dir, &name)
            }
            SkillsCommand::Import {
                path,
                global,
                replace,
                yes,
                review,
                no_review,
                account,
                model,
            } => {
                let reviewer = skill_transfer::Reviewer {
                    wanted: (review || no_review).then_some(review),
                    account: account.as_deref(),
                    model: model.as_deref(),
                };
                skill_transfer::import(&hedy, &dir, &path, global, replace, yes, &reviewer)
            }
            SkillsCommand::Try {
                name,
                tasks,
                check,
                yes,
                account,
                model,
            } => {
                let reviewer = skill_transfer::Reviewer {
                    wanted: None,
                    account: account.as_deref(),
                    model: model.as_deref(),
                };
                skill_trial::try_skill(&hedy, &dir, &name, &tasks, check.as_deref(), yes, &reviewer)
            }
            SkillsCommand::Check {
                name,
                no_review,
                account,
                model,
            } => {
                let reviewer = skill_transfer::Reviewer {
                    wanted: no_review.then_some(false),
                    account: account.as_deref(),
                    model: model.as_deref(),
                };
                skill_transfer::check(&hedy, &dir, &name, &reviewer)
            }
            SkillsCommand::Export { names, to } => skill_transfer::export(&hedy, &dir, &names, &to),
        },
        Command::Flow { command } => match command.unwrap_or(FlowCommand::List) {
            FlowCommand::List => commands::flow_list(&hedy, &dir),
            FlowCommand::Show { name } => commands::flow_show(&hedy, &dir, &name),
            FlowCommand::Run {
                name,
                request,
                model,
            } => flow_run::run(&hedy, &dir, &name, &request, model),
            FlowCommand::New { name, from, global } => {
                flow_edit::new_flow(&hedy, &dir, &name, from.as_deref(), global)
            }
            FlowCommand::Edit { name } => flow_edit::edit_flow(&hedy, &dir, &name),
            FlowCommand::Import {
                file,
                global,
                replace,
            } => flow_edit::import_flow(&hedy, &dir, &file, global, replace),
            FlowCommand::Default { name, clear } => {
                flow_edit::default_flow(&hedy, &dir, name.as_deref(), clear)
            }
            FlowCommand::Share { name } => flow_edit::share_flow(&hedy, &dir, &name),
            FlowCommand::Unshare { name } => flow_edit::unshare_flow(&hedy, &dir, &name),
            FlowCommand::Allow { command } => flow_edit::allow_command(&hedy, &command),
        },
        Command::Roles { command } => match command.unwrap_or(RolesCommand::List) {
            RolesCommand::List => flow_edit::roles_list(&hedy, &dir),
            RolesCommand::Show { id } => flow_edit::role_show(&hedy, &dir, &id),
            RolesCommand::New { id, from, global } => {
                flow_edit::role_new(&hedy, &dir, &id, from.as_deref(), global)
            }
            RolesCommand::Edit { id, global } => flow_edit::role_edit(&hedy, &dir, &id, global),
            RolesCommand::Remove { id } => flow_edit::role_remove(&hedy, &dir, &id),
            RolesCommand::Share { id } => flow_edit::share_role(&hedy, &dir, &id),
            RolesCommand::Unshare { id } => flow_edit::unshare_role(&hedy, &dir, &id),
        },
        Command::Template { command } => match command.unwrap_or(TemplateCommand::List) {
            TemplateCommand::List => commands::template_list(&hedy, &dir),
            TemplateCommand::Add {
                name,
                project,
                text,
            } => commands::template_add(&hedy, &dir, &name, project, &text),
            TemplateCommand::Remove { name, project } => {
                commands::template_remove(&hedy, &dir, &name, project)
            }
        },
        Command::Sync { command } => {
            let command = command.unwrap_or(SyncCommand::Status);
            // Only looking: what joins, leaves or decides is the user's.
            if !matches!(
                command,
                SyncCommand::Status | SyncCommand::Now | SyncCommand::Conflicts
            ) {
                commands::refuse_in_agent_session()?;
            }
            match command {
                SyncCommand::Status => sync::status(&hedy),
                SyncCommand::Setup { url, name } => sync::setup(&hedy, &url, name.as_deref()),
                SyncCommand::Join { url, name } => sync::join(&hedy, &url, name.as_deref()),
                SyncCommand::Now => sync::now_sync(&hedy),
                SyncCommand::Conflicts => sync::conflicts(&hedy),
                SyncCommand::Resolve { id, other, .. } => sync::resolve(&hedy, id, other),
                SyncCommand::Owner {
                    owner,
                    mine,
                    shared,
                    forget,
                } => {
                    let choice = match (mine, shared, forget) {
                        (true, _, _) => Some(sync::OwnerChoice::Mine),
                        (_, true, _) => Some(sync::OwnerChoice::Shared),
                        (_, _, true) => Some(sync::OwnerChoice::Forget),
                        _ => None,
                    };
                    sync::owner(&hedy, owner.as_deref(), choice)
                }
                SyncCommand::Project {
                    personal,
                    local,
                    shared,
                    follow,
                } => {
                    let choice = match (personal, local, shared, follow) {
                        (true, ..) => Some(sync::HomeChoice::Personal),
                        (_, true, ..) => Some(sync::HomeChoice::Local),
                        (_, _, Some(owner), _) => Some(sync::HomeChoice::Shared(owner)),
                        (.., true) => Some(sync::HomeChoice::Follow),
                        _ => None,
                    };
                    sync::project(&hedy, &dir, choice)
                }
                SyncCommand::Remove { pc, yes } => sync::remove(&hedy, &pc, yes),
                SyncCommand::Leave { yes } => sync::leave(&hedy, yes),
            }
        }
        Command::Team { team, command } => {
            let command = command.unwrap_or(TeamCommand::Status);
            let pick = || team::pick(&hedy, &dir, team.as_deref());
            match command {
                TeamCommand::Status => team::status(&hedy, &dir, team.as_deref()),
                TeamCommand::Create { url, name, me, pc } => {
                    team::create(&hedy, &url, &name, me.as_deref(), pc.as_deref())
                }
                TeamCommand::Invite {
                    cancel: Some(number),
                    ..
                } => team::cancel_invite(&hedy, &pick()?, number),
                TeamCommand::Invite { admin, mine, .. } => {
                    commands::refuse_in_agent_session()?;
                    team::invite(&hedy, &pick()?, admin, mine)
                }
                TeamCommand::Join { url, me, pc } => {
                    team::join(&hedy, &url, me.as_deref(), pc.as_deref())
                }
                TeamCommand::Now => team::now_sync(&hedy, &pick()?),
                TeamCommand::Link => team::link(&hedy, &dir, &pick()?),
                TeamCommand::Notes => team::notes(&hedy, &pick()?),
                TeamCommand::Add {
                    text,
                    everyone,
                    category,
                } => {
                    // Published at once for a project note or an admin.
                    commands::refuse_in_agent_session()?;
                    team::add(&hedy, &dir, &pick()?, &text.join(" "), everyone, &category)
                }
                TeamCommand::Publish { id, everyone } => {
                    commands::refuse_in_agent_session()?;
                    team::publish(&hedy, &pick()?, id, everyone)
                }
                TeamCommand::Review {
                    id,
                    accept,
                    reject,
                    yes,
                } => {
                    if accept || reject {
                        commands::refuse_in_agent_session()?;
                    }
                    team::review(&hedy, &pick()?, id.as_deref(), accept, reject, yes)
                }
                TeamCommand::Role { person, admin, .. } => {
                    commands::refuse_in_agent_session()?;
                    team::role(&hedy, &pick()?, &person, admin)
                }
                TeamCommand::Remove { who, pc, yes } => {
                    commands::refuse_in_agent_session()?;
                    team::remove(&hedy, &pick()?, &who, pc, yes)
                }
                TeamCommand::Rename { name, person } => {
                    commands::refuse_in_agent_session()?;
                    team::rename(&hedy, &pick()?, &name, person.as_deref())
                }
                TeamCommand::Leave { yes, here } => {
                    commands::refuse_in_agent_session()?;
                    team::leave(&hedy, &pick()?, yes, here)
                }
            }
        }
        Command::Config(sub) => match sub {
            ConfigCommand::Get { key } => commands::config_get(&hedy, &key),
            // A setting can name the program Hedy starts (`binary.claude`).
            ConfigCommand::Set { key, value } => {
                commands::refuse_in_agent_session()?;
                commands::config_set(&hedy, &key, Some(&value))
            }
            ConfigCommand::Unset { key } => {
                commands::refuse_in_agent_session()?;
                commands::config_set(&hedy, &key, None)
            }
        },
        Command::Completions { .. } | Command::Doctor | Command::Ask(_) => {
            unreachable!("handled above")
        }
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
