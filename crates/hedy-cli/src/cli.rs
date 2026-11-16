//! Command-line definition. Commands and flags are English only; the help follows the language.
//!
//! The help is in the current language, so its texts are not doc comments
//! (clap's derive makes those static): they live in [`help_texts`] and are put
//! in the command at runtime by [`Cli::full_command`].

use std::path::PathBuf;

use clap::{Args, Command as ClapCommand, CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::Shell;
use hedy_core::application::Setting;
use hedy_core::domain::{Color, Provider};
use hedy_core::i18n::{self, Language};
use hedy_core::tr;

#[derive(Debug, Parser)]
#[command(name = "hedy", version, styles = crate::style::help_styles())]
pub struct Cli {
    #[arg(long, short = 'C', global = true)]
    pub dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// The command line with its help in the current language, and every
    /// setting key listed in `hedy config --help` (they depend on the
    /// providers, so they are added at runtime).
    pub fn full_command() -> ClapCommand {
        let mut texts = help_texts();
        let command = localize(Self::command(), "", &mut texts, &value_names());
        debug_assert!(texts.is_empty(), "help texts for nothing: {texts:?}");

        let keys: Vec<String> = Setting::all()
            .into_iter()
            .map(|s| format!("  {}", s.key()))
            .collect();
        let keys = keys.join("\n");
        let keys = tr!("Claves:\n{keys}", "Keys:\n{keys}");
        let command = command
            .after_help(tr!(
                "Sin subcomando muestra el estado del proyecto actual.",
                "Without a subcommand, shows the status of the current project."
            ))
            .mut_subcommand("config", |config| {
                ["get", "set", "unset"]
                    .into_iter()
                    .fold(config.after_help(&keys), |config, name| {
                        config.mut_subcommand(name, |sub| sub.after_help(&keys))
                    })
            });
        // The blob greets on top of `hedy --help`, only in a terminal.
        let style = crate::style::Style::for_stdout();
        let command = if style.enabled() {
            let text = [
                format!(
                    "{} {}",
                    style.bold("Hedy"),
                    style.muted(env!("CARGO_PKG_VERSION"))
                ),
                style.muted(tr!(
                    "Tus IAs y tus cuentas, en un solo lugar.",
                    "Your AIs and your accounts, in one place."
                )),
            ];
            let banner = crate::mascot::beside(&style, crate::mascot::Mood::Hi, &text);
            command.before_help(banner.join("\n"))
        } else {
            command
        };
        match i18n::language() {
            Language::Spanish => spanish_builtins(command),
            Language::English => command,
        }
    }

    pub fn parse_args() -> Self {
        Self::from_arg_matches(&Self::full_command().get_matches()).unwrap_or_else(|e| e.exit())
    }
}

/// Every help text: `(command path, argument id, text)`, where an empty id is
/// the command's own description.
fn help_texts() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "",
            "",
            tr!(
                "Trabaja con varias IAs de programación y varias cuentas desde un solo lugar",
                "Work with several coding AIs and several accounts from one place"
            ),
        ),
        (
            "",
            "dir",
            tr!(
                "Carpeta desde la que se resuelve el proyecto (por defecto, la actual)",
                "Folder the project is resolved from (by default, the current one)"
            ),
        ),
        (
            "status",
            "",
            tr!(
                "Cuentas, su sesión y el proyecto actual",
                "Show the accounts, their login and the current project"
            ),
        ),
        (
            "use",
            "",
            tr!(
                "Asigna una cuenta al proyecto actual (lo registra si es nuevo)",
                "Assign an account to the current project (registering it if new)"
            ),
        ),
        (
            "use",
            "account",
            tr!(
                "Identificador de la cuenta (mira `hedy account list`)",
                "Account id (see `hedy account list`)"
            ),
        ),
        (
            "use",
            "protect",
            tr!(
                "Reserva la cuenta solo para este proyecto",
                "Reserve the account for this project only"
            ),
        ),
        (
            "use",
            "unprotect",
            tr!("Quita la reserva", "Remove the reservation"),
        ),
        (
            "new",
            "",
            tr!(
                "Empieza un proyecto desde cero: crea su carpeta con git y lo registra",
                "Start a project from scratch: create its folder with git and register it"
            ),
        ),
        (
            "new",
            "name",
            tr!(
                "Nombre del proyecto; la carpeta usa una versión segura («Mi tienda» → mi-tienda)",
                "Project name; the folder gets a safe version (\"My shop\" → my-shop)"
            ),
        ),
        (
            "new",
            "parent",
            tr!(
                "Dónde crearlo (por defecto, donde están tus otros proyectos, o ~/Proyectos)",
                "Where to create it (by default, where your other projects are, or ~/Projects)"
            ),
        ),
        (
            "new",
            "account",
            tr!(
                "Cuenta del proyecto (por defecto, la predeterminada)",
                "The project's account (by default, the default one)"
            ),
        ),
        (
            "new",
            "protect",
            tr!(
                "Reserva la cuenta solo para este proyecto",
                "Reserve the account for this project only"
            ),
        ),
        (
            "tasks",
            "",
            tr!(
                "Tareas en curso y recientes de todos los proyectos; primero las que te necesitan",
                "Show running and recent tasks across projects, those that need you first"
            ),
        ),
        (
            "summary",
            "",
            tr!(
                "Uso (tokens) por cliente y por proyecto; costo estimado solo de API keys",
                "Show usage (tokens) by client and by project; estimated cost for API keys only"
            ),
        ),
        (
            "summary",
            "week",
            tr!(
                "Los últimos 7 días (por defecto)",
                "The last 7 days (default)"
            ),
        ),
        (
            "summary",
            "days",
            tr!("Los últimos N días", "The last N days"),
        ),
        (
            "client",
            "",
            tr!(
                "Para quién es el proyecto actual, para agrupar el uso por cliente",
                "Set who the current project is for, to group usage by client"
            ),
        ),
        (
            "client",
            "clear",
            tr!("Quita el cliente", "Remove the client"),
        ),
        (
            "run",
            "",
            tr!(
                "Abre la CLI oficial con la cuenta del proyecto",
                "Open the official CLI with the project's account"
            ),
        ),
        (
            "run",
            "account",
            tr!(
                "Usa esta cuenta en vez de la del proyecto",
                "Use this account instead of the project's"
            ),
        ),
        (
            "run",
            "args",
            tr!(
                "Argumentos para la CLI (después de `--`)",
                "Arguments for the CLI (after `--`)"
            ),
        ),
        (
            "run",
            "yes",
            tr!(
                "No pregunta si la cuenta está cerca de su límite (igual avisa)",
                "Do not ask when the account is close to its limit (it still warns)"
            ),
        ),
        (
            "usage",
            "",
            tr!(
                "Cuánto del plan lleva usado cada cuenta, como /usage en cada CLI",
                "Show how much of its plan each account has used, like /usage in each CLI"
            ),
        ),
        (
            "usage",
            "cached",
            tr!(
                "Solo lo guardado, sin preguntar a nadie (al instante)",
                "Only what is cached, without asking anyone (instant)"
            ),
        ),
        (
            "usage",
            "refresh",
            tr!(
                "Pregunta ahora, aunque nunca más de una vez cada 3 min por cuenta",
                "Ask now, though never more than once every 3 min per account"
            ),
        ),
        (
            "usage",
            "short",
            tr!(
                "Una línea corta para el prompt o la barra de estado (CP 83% · CT 34%); usa lo guardado salvo con --refresh",
                "One short line for a prompt or a status bar (CP 83% · CT 34%); uses the cache unless --refresh"
            ),
        ),
        ("usage", "json", tr!("Salida en JSON", "JSON output")),
        (
            "commit",
            "",
            tr!(
                "Sugiere un mensaje de commit para lo que preparaste (git add) y hace el commit con tu git",
                "Suggest a commit message for what you staged (git add) and commit it with your git"
            ),
        ),
        (
            "commit",
            "account",
            tr!(
                "Cuenta que escribe el mensaje (por defecto, la del proyecto o la predeterminada)",
                "Account that writes the message (by default, the project's or the default one)"
            ),
        ),
        (
            "commit",
            "model",
            tr!(
                "Modelo a usar (por defecto, el de la cuenta)",
                "Model to use (by default, the account's)"
            ),
        ),
        (
            "commit",
            "print",
            tr!(
                "Solo imprime el mensaje: git commit -m \"$(hedy commit --print)\"",
                "Only print the message: git commit -m \"$(hedy commit --print)\""
            ),
        ),
        (
            "commit",
            "yes",
            tr!("Hace el commit sin preguntar", "Commit without asking"),
        ),
        (
            "doctor",
            "",
            tr!(
                "Revisa las CLIs, las cuentas y Hedy, y avisa si una CLI cambió de versión",
                "Check the CLIs, the accounts and Hedy, and warn when a CLI changed version"
            ),
        ),
        ("account", "", tr!("Gestiona cuentas", "Manage accounts")),
        (
            "account add",
            "",
            tr!(
                "Registra una cuenta y abre el login oficial",
                "Register an account and open the official login"
            ),
        ),
        (
            "account add",
            "name",
            tr!(
                "Nombre visible, por ejemplo \"Claude personal\"",
                "Display name, for example \"Personal Claude\""
            ),
        ),
        (
            "account add",
            "claude",
            tr!("Suscripción de Claude Code", "Claude Code subscription"),
        ),
        (
            "account add",
            "codex",
            tr!(
                "Suscripción de ChatGPT con Codex",
                "ChatGPT subscription with Codex"
            ),
        ),
        (
            "account add",
            "gemini",
            tr!(
                "Gemini CLI (Code Assist Standard/Enterprise)",
                "Gemini CLI (Code Assist Standard/Enterprise)"
            ),
        ),
        (
            "account add",
            "kimi",
            tr!("Suscripción de Kimi Code", "Kimi Code subscription"),
        ),
        (
            "account add",
            "antigravity",
            tr!(
                "Antigravity CLI con tu cuenta personal de Google (una por equipo)",
                "Antigravity CLI with your personal Google account (one per computer)"
            ),
        ),
        (
            "account add",
            "api",
            tr!(
                "Proveedor por API key: kimi o gemini",
                "API-key provider: kimi or gemini"
            ),
        ),
        (
            "account add",
            "default_folder",
            tr!(
                "Usa la carpeta que la CLI ya usa por defecto (p. ej. ~/.claude) en vez de crear una",
                "Use the folder the CLI already uses by default (e.g. ~/.claude) instead of creating one"
            ),
        ),
        (
            "account add",
            "color",
            tr!(
                "Color: azul, morado, verde, naranja, cian, rosa, amarillo, turquesa o #RRGGBB",
                "Color: blue, purple, green, orange, cyan, pink, yellow, teal or #RRGGBB"
            ),
        ),
        (
            "account add",
            "no_login",
            tr!(
                "No abre el login al terminar",
                "Do not open the login at the end"
            ),
        ),
        (
            "account list",
            "",
            tr!("Lista las cuentas", "List the accounts"),
        ),
        (
            "account login",
            "",
            tr!(
                "Abre el login oficial de una cuenta de suscripción, o pide la API key",
                "Open the official login of a subscription account, or ask for the API key"
            ),
        ),
        (
            "account remove",
            "",
            tr!(
                "Deja de usar una cuenta en Hedy (su carpeta no se borra)",
                "Stop using an account in Hedy (its folder is not deleted)"
            ),
        ),
        (
            "account rename",
            "",
            tr!(
                "Cambia el nombre que ves de una cuenta (su id y su carpeta no cambian)",
                "Change an account's display name (its id and folder stay the same)"
            ),
        ),
        (
            "account rename",
            "name",
            tr!(
                "Nombre nuevo, p. ej. \"Claude casa\"",
                "New name, e.g. \"Claude home\""
            ),
        ),
        (
            "account default",
            "",
            tr!(
                "Cuenta para los proyectos que no tienen una asignada",
                "Set the account for projects that have none assigned"
            ),
        ),
        (
            "account default",
            "clear",
            tr!("Quita la cuenta por defecto", "Remove the default account"),
        ),
        (
            "memory",
            "",
            tr!(
                "Memoria del proyecto: notas aprobadas y propuestas de tus IAs",
                "Project memory: approved notes and proposals from your AIs"
            ),
        ),
        (
            "memory list",
            "",
            tr!(
                "Lista las notas aprobadas y las propuestas, con sus temas (por defecto)",
                "List the approved notes and the proposals, with their topics (default)"
            ),
        ),
        (
            "memory approve",
            "",
            tr!(
                "Aprueba una propuesta; si cambia una nota con el mismo tema, la reemplaza",
                "Approve a proposal; when it changes a note with the same topic, it replaces it"
            ),
        ),
        (
            "memory approve",
            "id",
            tr!(
                "Número de la propuesta (mira `hedy memory list`)",
                "Number of the proposal (see `hedy memory list`)"
            ),
        ),
        (
            "memory reject",
            "",
            tr!(
                "Rechaza una propuesta: se borra y nada más cambia",
                "Reject a proposal: it is deleted and nothing else changes"
            ),
        ),
        (
            "memory reject",
            "id",
            tr!(
                "Número de la propuesta (mira `hedy memory list`)",
                "Number of the proposal (see `hedy memory list`)"
            ),
        ),
        (
            "skills",
            "",
            tr!(
                "Habilidades aprendidas: procedimientos que tus IAs proponen y tú apruebas",
                "Learned skills: procedures your AIs propose and you approve"
            ),
        ),
        (
            "skills list",
            "",
            tr!(
                "Lista las habilidades aprobadas y las propuestas (por defecto)",
                "List the approved skills and the proposals (default)"
            ),
        ),
        (
            "skills show",
            "",
            tr!(
                "Muestra una habilidad completa y sus actualizaciones propuestas",
                "Show a whole skill and its proposed updates"
            ),
        ),
        (
            "skills show",
            "name",
            tr!(
                "Nombre de la habilidad (mira `hedy skills list`)",
                "Name of the skill (see `hedy skills list`)"
            ),
        ),
        (
            "skills approve",
            "",
            tr!(
                "Aprueba una propuesta y la pone a disposición de las IAs (archivos de cada CLI y herramientas de Hedy)",
                "Approve a proposal and make it available to the AIs (each CLI's files and Hedy's tools)"
            ),
        ),
        (
            "skills approve",
            "id",
            tr!(
                "Número de la propuesta (mira `hedy skills list`)",
                "Number of the proposal (see `hedy skills list`)"
            ),
        ),
        (
            "skills reject",
            "",
            tr!(
                "Rechaza una propuesta: se borra y nada más cambia",
                "Reject a proposal: it is deleted and nothing else changes"
            ),
        ),
        (
            "skills reject",
            "id",
            tr!(
                "Número de la propuesta (mira `hedy skills list`)",
                "Number of the proposal (see `hedy skills list`)"
            ),
        ),
        (
            "skills remove",
            "",
            tr!(
                "Borra una habilidad y los archivos que Hedy escribió para ella",
                "Delete a skill and the files Hedy wrote for it"
            ),
        ),
        (
            "skills remove",
            "name",
            tr!("Nombre de la habilidad", "Name of the skill"),
        ),
        (
            "template",
            "",
            tr!(
                "Plantillas de pedidos para el chat de Hedy, globales o del proyecto",
                "Prompt templates for Hedy's chat, global or for the project"
            ),
        ),
        (
            "template list",
            "",
            tr!(
                "Lista las plantillas del proyecto actual y las globales (por defecto)",
                "List the current project's templates and the global ones (default)"
            ),
        ),
        (
            "template add",
            "",
            tr!(
                "Guarda una plantilla; escribe `{{nombre}}` donde quieras completar algo al usarla",
                "Save a template; write `{{name}}` wherever you want to fill something in when using it"
            ),
        ),
        (
            "template add",
            "name",
            tr!("Nombre de la plantilla", "Template name"),
        ),
        (
            "template add",
            "project",
            tr!(
                "Solo para el proyecto actual (por defecto, para todos)",
                "Only for the current project (by default, for all of them)"
            ),
        ),
        (
            "template add",
            "text",
            tr!(
                "El texto del pedido, p. ej. \"Escribe pruebas para {{archivo}}\"",
                "The request text, e.g. \"Write tests for {{file}}\""
            ),
        ),
        (
            "template remove",
            "",
            tr!(
                "Borra una plantilla (los ejemplos se recuperan desde Ajustes)",
                "Delete a template (the examples can be restored from Settings)"
            ),
        ),
        (
            "template remove",
            "name",
            tr!("Nombre de la plantilla", "Template name"),
        ),
        (
            "template remove",
            "project",
            tr!(
                "Borra la del proyecto actual (por defecto, la global)",
                "Delete the current project's one (by default, the global one)"
            ),
        ),
        (
            "config",
            "",
            tr!(
                "Ajustes (las claves están en `hedy config --help`)",
                "Settings (the keys are in `hedy config --help`)"
            ),
        ),
        (
            "config get",
            "",
            tr!("Muestra el valor de un ajuste", "Show a setting's value"),
        ),
        (
            "config set",
            "",
            tr!("Cambia un ajuste", "Change a setting"),
        ),
        (
            "config unset",
            "",
            tr!("Borra un ajuste", "Clear a setting"),
        ),
        (
            "completions",
            "",
            tr!(
                "Genera el autocompletado para tu shell (bash, zsh o fish)",
                "Generate completions for your shell (bash, zsh or fish)"
            ),
        ),
        (
            "completions",
            "shell",
            tr!(
                "bash, zsh, fish, elvish o powershell",
                "bash, zsh, fish, elvish or powershell"
            ),
        ),
    ]
}

/// Placeholder names (`<CUENTA>`), by argument id, wherever the argument is.
fn value_names() -> Vec<(&'static str, String)> {
    vec![
        ("dir", tr!("CARPETA", "FOLDER")),
        ("parent", tr!("CARPETA", "FOLDER")),
        ("account", tr!("CUENTA", "ACCOUNT")),
        ("name", tr!("NOMBRE", "NAME")),
        ("api", tr!("PROVEEDOR", "PROVIDER")),
        ("args", tr!("ARGUMENTOS", "ARGS")),
        ("key", tr!("CLAVE", "KEY")),
        ("value", tr!("VALOR", "VALUE")),
        ("text", tr!("TEXTO", "TEXT")),
        ("model", tr!("MODELO", "MODEL")),
    ]
}

/// Puts the texts of `command` (at `path`) and of its subcommands, taking
/// each one from `texts`. Arguments are changed in place: moving them would
/// reorder the positional ones.
fn localize(
    command: ClapCommand,
    path: &str,
    texts: &mut Vec<(&'static str, &'static str, String)>,
    values: &[(&'static str, String)],
) -> ClapCommand {
    let mut take = |id: &str| {
        let i = texts.iter().position(|(p, a, _)| *p == path && *a == id)?;
        Some(texts.swap_remove(i).2)
    };
    let mut command = match take("") {
        Some(about) => command.about(about),
        None => command,
    };
    command = command.mut_args(|arg| {
        let arg = match take(arg.get_id().as_str()) {
            Some(help) => arg.help(help),
            None => arg,
        };
        match values.iter().find(|(id, _)| arg.get_id() == id) {
            Some((_, name)) => arg.value_name(name.clone()),
            None => arg,
        }
    });
    command.mut_subcommands(|sub| {
        let path = format!("{path} {}", sub.get_name());
        localize(sub, path.trim_start(), texts, values)
    })
}

/// clap writes its own texts (section titles, `Usage`, `[OPTIONS]`, `--help`,
/// `--version`, the `help` command) in English. In Spanish they are replaced
/// on the built command, where those arguments and the `help` command exist.
fn spanish_builtins(mut command: ClapCommand) -> ClapCommand {
    command.build();
    translate_builtins(command)
}

fn translate_builtins(mut command: ClapCommand) -> ClapCommand {
    let help_command = command.get_name() == "help";
    if help_command {
        command = command.about("Muestra esta ayuda o la de los subcomandos indicados");
    }
    if command.has_subcommands() {
        command = command
            .subcommand_help_heading("Comandos")
            .subcommand_value_name("COMANDO");
    }
    command = command.mut_args(|arg| {
        let heading = if arg.is_positional() {
            "Argumentos"
        } else {
            "Opciones"
        };
        let arg = arg.help_heading(heading);
        match arg.get_id().as_str() {
            "help" => arg.help("Muestra la ayuda"),
            "version" => arg.help("Muestra la versión"),
            "subcommand" if help_command => arg
                .help("Los subcomandos de los que mostrar la ayuda")
                .value_name("COMANDO"),
            _ => arg,
        }
    });

    // The usage comes as "<style>Usage:<reset> hedy …": keep what follows.
    let usage = command.render_usage().ansi().to_string();
    if let Some((_, rest)) = usage.split_once("Usage:") {
        let usage = rest.split_once(' ').map_or(rest, |(_, usage)| usage);
        command = command.override_usage(usage.replace("[OPTIONS]", "[OPCIONES]"));
    }
    let style = command.get_styles().get_usage();
    let (on, off) = (style.render(), style.render_reset());
    let has_args = command.has_subcommands() || command.get_arguments().any(|a| !a.is_hide_set());
    let sections = if has_args { "\n\n{all-args}" } else { "" };
    command = command.help_template(format!(
        "{{before-help}}{{about-with-newline}}\n{on}Uso:{off} {{usage}}{sections}{{after-help}}"
    ));

    command.mut_subcommands(translate_builtins)
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Status,

    Use {
        account: String,
        #[arg(long, conflicts_with = "unprotect")]
        protect: bool,
        #[arg(long)]
        unprotect: bool,
    },

    New {
        name: String,
        #[arg(long = "in", id = "parent")]
        parent: Option<PathBuf>,
        #[arg(long, short)]
        account: Option<String>,
        #[arg(long)]
        protect: bool,
    },

    Tasks,

    Summary {
        #[arg(long, conflicts_with = "days")]
        week: bool,
        #[arg(long, value_name = "N")]
        days: Option<u32>,
    },

    Client {
        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        name: Option<String>,
        #[arg(long)]
        clear: bool,
    },

    Run {
        #[arg(long, short)]
        account: Option<String>,
        #[arg(long, short)]
        yes: bool,
        #[arg(last = true)]
        args: Vec<String>,
    },

    Doctor,

    Usage {
        #[arg(long, conflicts_with = "refresh")]
        cached: bool,
        #[arg(long)]
        refresh: bool,
        #[arg(long, conflicts_with = "json")]
        short: bool,
        #[arg(long)]
        json: bool,
    },

    Commit {
        #[arg(long, short)]
        account: Option<String>,
        #[arg(long, short)]
        model: Option<String>,
        #[arg(long, conflicts_with = "yes")]
        print: bool,
        #[arg(long, short)]
        yes: bool,
    },

    #[command(subcommand)]
    Account(AccountCommand),

    Memory {
        #[command(subcommand)]
        command: Option<MemoryCommand>,
    },

    Skills {
        #[command(subcommand)]
        command: Option<SkillsCommand>,
    },

    Template {
        #[command(subcommand)]
        command: Option<TemplateCommand>,
    },

    #[command(subcommand)]
    Config(ConfigCommand),

    Completions {
        #[arg(hide_possible_values = true)]
        shell: Shell,
    },
}

#[derive(Debug, Subcommand)]
pub enum AccountCommand {
    Add(AddAccount),

    List,

    Login {
        account: String,
    },

    Remove {
        account: String,
    },

    Rename {
        account: String,
        name: String,
    },

    Default {
        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        account: Option<String>,
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    List,

    Approve { id: i64 },

    Reject { id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum SkillsCommand {
    List,

    Show { name: String },

    Approve { id: i64 },

    Reject { id: i64 },

    Remove { name: String },
}

#[derive(Debug, Subcommand)]
pub enum TemplateCommand {
    List,

    Add {
        name: String,
        #[arg(long)]
        project: bool,
        #[arg(long)]
        text: String,
    },

    Remove {
        name: String,
        #[arg(long)]
        project: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Get { key: String },
    Set { key: String, value: String },
    Unset { key: String },
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("kind").required(true).args(["claude", "codex", "gemini", "kimi", "antigravity", "api"])))]
pub struct AddAccount {
    pub name: String,
    #[arg(long)]
    pub claude: bool,
    #[arg(long)]
    pub codex: bool,
    #[arg(long)]
    pub gemini: bool,
    #[arg(long)]
    pub kimi: bool,
    #[arg(long)]
    pub antigravity: bool,
    #[arg(long)]
    pub api: Option<Provider>,
    #[arg(long, conflicts_with = "api")]
    pub default_folder: bool,
    #[arg(long)]
    pub color: Option<Color>,
    #[arg(long)]
    pub no_login: bool,
}

impl AddAccount {
    pub fn provider(&self) -> Provider {
        let flags = [
            (self.claude, Provider::Claude),
            (self.codex, Provider::Codex),
            (self.gemini, Provider::Gemini),
            (self.kimi, Provider::Kimi),
            (self.antigravity, Provider::Antigravity),
        ];
        flags
            .into_iter()
            .find_map(|(set, provider)| set.then_some(provider))
            .or(self.api)
            .expect("clap requires one of the provider flags")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_is_valid() {
        Cli::full_command().debug_assert();
    }

    /// Every help page, in one language. Other tests do not depend on it, so
    /// switching the process-wide language here is safe.
    fn all_help(language: Language) -> String {
        fn walk(command: &mut ClapCommand, out: &mut String) {
            out.push_str(&command.render_help().to_string());
            for sub in command.get_subcommands_mut() {
                walk(sub, out);
            }
        }
        i18n::set_language(language);
        let mut command = Cli::full_command();
        command.build();
        let mut out = String::new();
        walk(&mut command, &mut out);
        out
    }

    #[test]
    fn help_is_all_in_one_language() {
        let spanish = all_help(Language::Spanish);
        let english = all_help(Language::English);
        i18n::set_language(Language::Spanish);
        for clap_text in [
            "Usage",
            "Options",
            "Arguments",
            "Commands",
            "Print",
            "OPTIONS",
        ] {
            assert!(!spanish.contains(clap_text), "{clap_text} in Spanish help");
        }
        assert!(spanish.contains("Uso:") && spanish.contains("[OPCIONES]"));
        assert!(spanish.contains("Muestra la ayuda") && spanish.contains("<CUENTA>"));
        for spanish_text in ["Cuenta", "Muestra", "Opciones", "proyecto", "Claves"] {
            assert!(
                !english.contains(spanish_text),
                "{spanish_text} in English help"
            );
        }
        assert!(english.contains("Usage:") && english.contains("<ACCOUNT>"));
        // Commands are English only: no Spanish aliases in either help.
        assert!(!english.contains("alias") && !spanish.contains("alias"));
    }

    #[test]
    fn config_help_lists_every_key() {
        let mut command = Cli::full_command();
        let config = command.find_subcommand_mut("config").unwrap();
        let help = config.render_help().to_string();
        for setting in Setting::all() {
            assert!(help.contains(&setting.key()), "{} missing", setting.key());
        }
        assert!(help.contains("api_price.kimi") && help.contains("code_map.binary"));
        let set = config.find_subcommand_mut("set").unwrap();
        assert!(set.render_help().to_string().contains("ui.theme"));
    }

    #[test]
    fn clear_does_not_take_a_value_too() {
        let parse = |args: &[&str]| Cli::try_parse_from(args);
        assert!(parse(&["hedy", "account", "default", "x", "--clear"]).is_err());
        assert!(parse(&["hedy", "client", "x", "--clear"]).is_err());
        assert!(parse(&["hedy", "account", "default", "--clear"]).is_ok());
        assert!(parse(&["hedy", "client", "--clear"]).is_ok());
        assert!(parse(&["hedy", "client"]).is_err());
    }

    #[test]
    fn spanish_aliases_are_gone() {
        for alias in [
            "estado",
            "cuenta",
            "usar",
            "diagnostico",
            "memoria",
            "tareas",
        ] {
            assert!(Cli::try_parse_from(["hedy", alias]).is_err(), "{alias}");
        }
        let cli = Cli::try_parse_from(["hedy", "account", "add", "Kimi", "--api", "kimi"]).unwrap();
        let Some(Command::Account(AccountCommand::Add(add))) = cli.command else {
            panic!("expected account add");
        };
        assert_eq!(add.provider(), Provider::Kimi);
        assert!(matches!(
            Cli::try_parse_from(["hedy", "use", "trabajo", "--protect"])
                .unwrap()
                .command,
            Some(Command::Use { protect: true, .. })
        ));
        assert!(matches!(
            Cli::try_parse_from(["hedy", "doctor"]).unwrap().command,
            Some(Command::Doctor)
        ));
    }

    #[test]
    fn requires_exactly_one_provider() {
        assert!(Cli::try_parse_from(["hedy", "account", "add", "X"]).is_err());
        assert!(
            Cli::try_parse_from(["hedy", "account", "add", "X", "--claude", "--codex"]).is_err()
        );
        assert!(Cli::try_parse_from(["hedy", "account", "add", "X", "--api", "openai"]).is_err());
    }

    #[test]
    fn memory_lists_by_default() {
        let parse = |args: &[&str]| Cli::try_parse_from(args).unwrap().command;
        assert!(matches!(
            parse(&["hedy", "memory"]),
            Some(Command::Memory { command: None })
        ));
        assert!(matches!(
            parse(&["hedy", "memory", "approve", "7"]),
            Some(Command::Memory {
                command: Some(MemoryCommand::Approve { id: 7 })
            })
        ));
        assert!(matches!(
            parse(&["hedy", "memory", "reject", "3"]),
            Some(Command::Memory {
                command: Some(MemoryCommand::Reject { id: 3 })
            })
        ));
        assert!(Cli::try_parse_from(["hedy", "memory", "approve", "x"]).is_err());
    }

    #[test]
    fn skills_list_by_default() {
        let parse = |args: &[&str]| Cli::try_parse_from(args).unwrap().command;
        assert!(matches!(
            parse(&["hedy", "skills"]),
            Some(Command::Skills { command: None })
        ));
        assert!(matches!(
            parse(&["hedy", "skills", "approve", "4"]),
            Some(Command::Skills {
                command: Some(SkillsCommand::Approve { id: 4 })
            })
        ));
        assert!(matches!(
            parse(&["hedy", "skills", "show", "run-tests"]),
            Some(Command::Skills {
                command: Some(SkillsCommand::Show { name })
            }) if name == "run-tests"
        ));
        assert!(matches!(
            parse(&["hedy", "skills", "remove", "lint"]),
            Some(Command::Skills {
                command: Some(SkillsCommand::Remove { .. })
            })
        ));
        assert!(Cli::try_parse_from(["hedy", "skills", "reject", "x"]).is_err());
    }

    #[test]
    fn templates_list_by_default() {
        let parse = |args: &[&str]| Cli::try_parse_from(args).unwrap().command;
        assert!(matches!(
            parse(&["hedy", "template"]),
            Some(Command::Template { command: None })
        ));
        let Some(Command::Template {
            command:
                Some(TemplateCommand::Add {
                    name,
                    project,
                    text,
                }),
        }) = parse(&[
            "hedy",
            "template",
            "add",
            "Pruebas",
            "--project",
            "--text",
            "Pruebas para {archivo}",
        ])
        else {
            panic!("expected template add");
        };
        assert_eq!((name.as_str(), project), ("Pruebas", true));
        assert_eq!(text, "Pruebas para {archivo}");
        assert!(Cli::try_parse_from(["hedy", "template", "add", "x"]).is_err());
        assert!(matches!(
            parse(&["hedy", "template", "remove", "x"]),
            Some(Command::Template {
                command: Some(TemplateCommand::Remove { project: false, .. })
            })
        ));
    }

    #[test]
    fn run_passes_arguments_through() {
        let cli =
            Cli::try_parse_from(["hedy", "run", "-a", "personal", "--", "--resume", "x"]).unwrap();
        let Some(Command::Run { account, args, .. }) = cli.command else {
            panic!("expected run");
        };
        assert_eq!(account.as_deref(), Some("personal"));
        assert_eq!(args, vec!["--resume", "x"]);
        assert!(matches!(
            Cli::try_parse_from(["hedy", "run", "-y", "--", "-c"])
                .unwrap()
                .command,
            Some(Command::Run { yes: true, .. })
        ));
    }

    #[test]
    fn usage_and_commit_flags() {
        assert!(matches!(
            Cli::try_parse_from(["hedy", "usage", "--short", "--refresh"])
                .unwrap()
                .command,
            Some(Command::Usage {
                short: true,
                refresh: true,
                cached: false,
                json: false
            })
        ));
        assert!(Cli::try_parse_from(["hedy", "usage", "--cached", "--refresh"]).is_err());
        assert!(Cli::try_parse_from(["hedy", "usage", "--short", "--json"]).is_err());
        let Some(Command::Commit {
            account,
            model,
            print,
            yes,
        }) = Cli::try_parse_from(["hedy", "commit", "-a", "personal", "-m", "haiku", "--print"])
            .unwrap()
            .command
        else {
            panic!("expected commit");
        };
        assert_eq!(
            (account.as_deref(), model.as_deref(), print, yes),
            (Some("personal"), Some("haiku"), true, false)
        );
        assert!(Cli::try_parse_from(["hedy", "commit", "--print", "--yes"]).is_err());
    }
}
