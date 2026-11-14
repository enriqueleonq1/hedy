//! Command-line definition. English commands with Spanish aliases.

use std::path::PathBuf;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::Shell;
use hedy_core::application::Setting;
use hedy_core::domain::{Color, Provider};

#[derive(Debug, Parser)]
#[command(
    name = "hedy",
    version,
    about = "Trabaja con varias IAs de programación y varias cuentas desde un solo lugar",
    after_help = "Sin subcomando muestra el estado del proyecto actual."
)]
pub struct Cli {
    /// Carpeta desde la que se resuelve el proyecto (por defecto, la actual).
    #[arg(long, short = 'C', global = true, value_name = "CARPETA")]
    pub dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// The command line, with every setting key listed in `hedy config --help`
    /// (they depend on the providers, so they are added at runtime).
    pub fn full_command() -> clap::Command {
        let keys: Vec<String> = Setting::all()
            .into_iter()
            .map(|s| format!("  {}", s.key()))
            .collect();
        let keys = format!("Claves:\n{}", keys.join("\n"));
        Self::command().mut_subcommand("config", |config| {
            ["get", "set", "unset"]
                .into_iter()
                .fold(config.after_help(&keys), |config, name| {
                    config.mut_subcommand(name, |sub| sub.after_help(&keys))
                })
        })
    }

    pub fn parse_args() -> Self {
        Self::from_arg_matches(&Self::full_command().get_matches()).unwrap_or_else(|e| e.exit())
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Cuentas, su sesión y el proyecto actual.
    #[command(visible_alias = "estado")]
    Status,

    /// Asigna una cuenta al proyecto actual (lo registra si es nuevo).
    #[command(visible_alias = "usar")]
    Use {
        /// Identificador de la cuenta (mira `hedy account list`).
        account: String,
        /// Reserva la cuenta solo para este proyecto.
        #[arg(long, conflicts_with = "unprotect")]
        protect: bool,
        /// Quita la reserva.
        #[arg(long)]
        unprotect: bool,
    },

    /// Uso y costo estimado por cliente y por proyecto.
    #[command(visible_alias = "resumen")]
    Summary {
        /// Los últimos 7 días (por defecto).
        #[arg(long, visible_alias = "semana", conflicts_with = "days")]
        week: bool,
        /// Los últimos N días.
        #[arg(long, visible_alias = "dias", value_name = "N")]
        days: Option<u32>,
    },

    /// Para quién es el proyecto actual, para sumar el costo por cliente.
    #[command(visible_alias = "cliente")]
    Client {
        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        name: Option<String>,
        /// Quita el cliente.
        #[arg(long)]
        clear: bool,
    },

    /// Abre la CLI oficial con la cuenta del proyecto.
    Run {
        /// Usa esta cuenta en vez de la del proyecto.
        #[arg(long, short)]
        account: Option<String>,
        /// Argumentos para la CLI (después de `--`).
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Gestiona cuentas.
    #[command(visible_alias = "cuenta", subcommand)]
    Account(AccountCommand),

    /// Ajustes (las claves están en `hedy config --help`).
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Genera el autocompletado para tu shell (bash, zsh o fish).
    Completions { shell: Shell },
}

#[derive(Debug, Subcommand)]
pub enum AccountCommand {
    /// Registra una cuenta y abre el login oficial.
    #[command(visible_alias = "agregar")]
    Add(AddAccount),

    /// Lista las cuentas.
    #[command(visible_alias = "listar")]
    List,

    /// Abre el login oficial de una cuenta de suscripción, o pide la API key.
    Login { account: String },

    /// Deja de usar una cuenta en Hedy (su carpeta no se borra).
    #[command(visible_alias = "quitar")]
    Remove { account: String },

    /// Cuenta para los proyectos que no tienen una asignada.
    #[command(visible_alias = "predeterminada")]
    Default {
        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        account: Option<String>,
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Muestra el valor de un ajuste.
    Get { key: String },
    /// Cambia un ajuste.
    Set { key: String, value: String },
    /// Borra un ajuste.
    Unset { key: String },
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("kind").required(true).args(["claude", "codex", "gemini", "api"])))]
pub struct AddAccount {
    /// Nombre visible, por ejemplo "Claude personal".
    pub name: String,
    /// Suscripción de Claude Code.
    #[arg(long)]
    pub claude: bool,
    /// Suscripción de ChatGPT con Codex.
    #[arg(long)]
    pub codex: bool,
    /// Gemini CLI (Code Assist Standard/Enterprise).
    #[arg(long)]
    pub gemini: bool,
    /// Proveedor por API key: kimi o gemini.
    #[arg(long, value_name = "PROVEEDOR")]
    pub api: Option<Provider>,
    /// Usa la carpeta que la CLI ya usa por defecto (p. ej. ~/.claude) en vez de crear una.
    #[arg(long, conflicts_with = "api")]
    pub default_folder: bool,
    /// Color: azul, morado, verde, naranja, cian, rosa, amarillo, turquesa o #RRGGBB.
    #[arg(long)]
    pub color: Option<Color>,
    /// No abre el login al terminar.
    #[arg(long)]
    pub no_login: bool,
}

impl AddAccount {
    pub fn provider(&self) -> Provider {
        match (self.claude, self.codex, self.gemini, self.api) {
            (true, ..) => Provider::Claude,
            (_, true, ..) => Provider::Codex,
            (_, _, true, _) => Provider::Gemini,
            (.., Some(p)) => p,
            _ => unreachable!("clap requires one of the provider flags"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_is_valid() {
        Cli::full_command().debug_assert();
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
    fn spanish_aliases_work() {
        let cli =
            Cli::try_parse_from(["hedy", "cuenta", "agregar", "Kimi", "--api", "kimi"]).unwrap();
        let Some(Command::Account(AccountCommand::Add(add))) = cli.command else {
            panic!("expected account add");
        };
        assert_eq!(add.provider(), Provider::Kimi);
        assert!(matches!(
            Cli::try_parse_from(["hedy", "usar", "trabajo", "--protect"])
                .unwrap()
                .command,
            Some(Command::Use { protect: true, .. })
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
    fn run_passes_arguments_through() {
        let cli =
            Cli::try_parse_from(["hedy", "run", "-a", "personal", "--", "--resume", "x"]).unwrap();
        let Some(Command::Run { account, args }) = cli.command else {
            panic!("expected run");
        };
        assert_eq!(account.as_deref(), Some("personal"));
        assert_eq!(args, vec!["--resume", "x"]);
    }
}
