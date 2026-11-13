//! Command-line definition. English commands with Spanish aliases.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;
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

    /// Ajustes: `default_account`, `binary.claude`, `binary.codex`, `binary.gemini`.
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
        #[arg(required_unless_present = "clear")]
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
    use clap::CommandFactory;

    #[test]
    fn definition_is_valid() {
        Cli::command().debug_assert();
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
