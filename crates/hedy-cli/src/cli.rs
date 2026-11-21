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
            .mut_subcommand("ask", |ask| ask.after_help(ask_examples()))
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

/// Examples at the end of `hedy ask --help`.
fn ask_examples() -> String {
    let examples = [
        tr!(
            "hedy ask \"¿qué hace esta regex: ^\\d{{3}}$?\"",
            "hedy ask \"what does this regex do: ^\\d{{3}}$\""
        ),
        "git diff | hedy ask -m sonnet".to_owned(),
        tr!(
            "hedy ask -c \"¿y en Python?\"",
            "hedy ask -c \"and in Python?\""
        ),
        tr!(
            "hedy ask --json \"resume este error\" > respuesta.json",
            "hedy ask --json \"summarize this error\" > answer.json"
        ),
    ];
    let examples: Vec<String> = examples.iter().map(|e| format!("  {e}")).collect();
    let examples = examples.join("\n");
    tr!("Ejemplos:\n{examples}", "Examples:\n{examples}")
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
            "permissions",
            "",
            tr!(
                "Qué hacen sin preguntar las tareas nuevas del proyecto actual (sin nada: el modo de ahora)",
                "What the current project's new tasks do without asking (with nothing: the current mode)"
            ),
        ),
        (
            "permissions",
            "mode",
            tr!(
                "manual: todo pregunta · edits: las ediciones van solas · auto: también los comandos, salvo los arriesgados",
                "manual: everything asks · edits: edits go through · auto: commands too, except risky ones"
            ),
        ),
        (
            "block",
            "",
            tr!(
                "Archivos que la IA no lee en el proyecto actual (sin nada: la lista; vacía por defecto, como en Claude Code)",
                "Files the AI does not read in the current project (with nothing: the list; empty by default, as in Claude Code)"
            ),
        ),
        (
            "block",
            "patterns",
            tr!(
                "Nombres o patrones a agregar (.env, .env.*, *.pem)",
                "Names or patterns to add (.env, .env.*, *.pem)"
            ),
        ),
        (
            "block",
            "remove",
            tr!(
                "Quita esos patrones en vez de agregarlos",
                "Remove those patterns instead of adding them"
            ),
        ),
        (
            "block",
            "usual",
            tr!(
                "Agrega los de siempre: .env, claves y credenciales",
                "Add the usual ones: .env, keys and credentials"
            ),
        ),
        (
            "block",
            "clear",
            tr!(
                "Vacía la lista: la IA lee todo",
                "Empty the list: the AI reads everything"
            ),
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
            "commands",
            "",
            tr!(
                "Los comandos con «/» y las habilidades que tiene una cuenta, según su CLI",
                "The slash commands and skills an account has, as its CLI reports them"
            ),
        ),
        (
            "commands",
            "account",
            tr!(
                "Cuenta (por defecto, la del proyecto o la predeterminada)",
                "Account (by default, the project's or the default one)"
            ),
        ),
        ("commands", "json", tr!("Salida en JSON", "JSON output")),
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
            "review",
            "",
            tr!(
                "Revisa un cambio contra las reglas escritas (tuyas, del proyecto y del equipo); solo avisa, nunca bloquea",
                "Check a change against the written rules (yours, the project's and the team's); it only warns, never blocks"
            ),
        ),
        (
            "review",
            "revs",
            tr!(
                "Un rango de commits (main..HEAD) o un commit; por defecto, lo que no tiene commit todavía",
                "A range of commits (main..HEAD) or one commit; by default, what is not committed yet"
            ),
        ),
        (
            "review",
            "staged",
            tr!(
                "Solo lo preparado para el commit (git add)",
                "Only what is staged for the commit (git add)"
            ),
        ),
        (
            "review",
            "task",
            tr!(
                "Los cambios de una tarea de Hedy (su id, de hedy tasks)",
                "A Hedy task's changes (its id, from hedy tasks)"
            ),
        ),
        (
            "review",
            "account",
            tr!(
                "Cuenta que revisa (por defecto, la del proyecto; Hedy nunca elige otra)",
                "Account that checks (by default, the project's; Hedy never picks another)"
            ),
        ),
        (
            "review",
            "model",
            tr!("Modelo que revisa", "Model that checks"),
        ),
        (
            "review",
            "json",
            tr!(
                "Avisos y reglas en JSON, para scripts y hooks",
                "Warnings and rules as JSON, for scripts and hooks"
            ),
        ),
        (
            "ask",
            "",
            tr!(
                "Pregunta rápida desde la terminal; también queda en Hedy → Conversaciones",
                "Ask a quick question from the terminal; it also shows in Hedy → Conversations"
            ),
        ),
        (
            "ask",
            "question",
            tr!(
                "La pregunta (las palabras se unen con espacios); con «-», o sin pregunta y con una tubería, se lee de la entrada",
                "The question (words are joined with spaces); with \"-\", or none and a pipe, it is read from stdin"
            ),
        ),
        (
            "ask",
            "account",
            tr!(
                "Cuenta a usar (por defecto, la predeterminada; Hedy nunca elige otra)",
                "Account to use (by default, the default one; Hedy never picks another)"
            ),
        ),
        (
            "ask",
            "model",
            tr!(
                "Modelo, p. ej. fable, opus, sonnet o haiku con Claude",
                "Model, e.g. fable, opus, sonnet or haiku with Claude"
            ),
        ),
        (
            "ask",
            "research",
            tr!(
                "Investiga (también en la web) y cita las fuentes, en vez de solo responder",
                "Research (on the web too) and cite the sources, instead of just answering"
            ),
        ),
        (
            "ask",
            "follow",
            tr!(
                "Sigue la conversación más reciente, o la indicada con --continue=<ID>",
                "Follow up on the most recent conversation, or on --continue=<ID>"
            ),
        ),
        (
            "ask",
            "json",
            tr!(
                "Al terminar, imprime la respuesta como JSON (id, title, account, answer)",
                "When done, print the answer as JSON (id, title, account, answer)"
            ),
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
            "memory ask",
            "",
            tr!(
                "Pregunta a tu memoria: responde solo con lo escrito y cita cada nota",
                "Ask your memory: it answers only from what is written and cites each note"
            ),
        ),
        ("memory ask", "question", tr!("La pregunta", "The question")),
        (
            "memory ask",
            "team",
            tr!(
                "Solo la memoria de ese equipo (por su nombre)",
                "Only that team's memory (by its name)"
            ),
        ),
        (
            "memory ask",
            "all",
            tr!(
                "Toda tu memoria: tus notas, las de tus proyectos y las de tus equipos (por defecto fuera de un proyecto)",
                "All your memory: your notes, your projects' and your teams' (the default outside a project)"
            ),
        ),
        (
            "memory ask",
            "code",
            tr!(
                "Búscalo en el código del proyecto y propone la nota que lo habría respondido",
                "Look for it in the project's code and propose the note that would have answered it"
            ),
        ),
        (
            "memory ask",
            "account",
            tr!(
                "Cuenta que responde (por defecto, la del proyecto)",
                "Account that answers (by default, the project's)"
            ),
        ),
        (
            "memory ask",
            "model",
            tr!("Modelo que responde", "Model that answers"),
        ),
        (
            "memory ask",
            "json",
            tr!(
                "Respuesta y fuentes en JSON, para scripts",
                "Answer and sources as JSON, for scripts"
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
                "Aprueba una propuesta y la pone a disposición de tus IAs dentro de Hedy",
                "Approve a proposal and make it available to your AIs inside Hedy"
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
            "skills import",
            "",
            tr!(
                "Importa habilidades de archivos SKILL.md: primero muestra qué entra y qué no, y por qué",
                "Import skills from SKILL.md files: first shows what goes in and what does not, and why"
            ),
        ),
        (
            "skills import",
            "path",
            tr!(
                "Un SKILL.md, la carpeta de una habilidad, una carpeta de habilidades (como ~/.claude/skills) o un proyecto",
                "A SKILL.md, a skill's folder, a folder of skills (such as ~/.claude/skills) or a project"
            ),
        ),
        (
            "skills import",
            "global",
            tr!("Para todos los proyectos", "For every project"),
        ),
        (
            "skills import",
            "replace",
            tr!(
                "Actualiza tus habilidades aprobadas con el mismo nombre",
                "Update your approved skills with the same name"
            ),
        ),
        (
            "skills import",
            "yes",
            tr!("Sin preguntar", "Without asking"),
        ),
        (
            "skills import",
            "review",
            tr!(
                "Que la IA revise cada una contra tus notas, aunque sean muchas (una petición por habilidad)",
                "Have the AI review each one against your notes, even many (one request per skill)"
            ),
        ),
        (
            "skills import",
            "no_review",
            tr!(
                "Sin la revisión de la IA: solo lo que mide Hedy, sin gastar tokens",
                "Without the AI's review: only what Hedy measures, spending no tokens"
            ),
        ),
        (
            "skills import",
            "account",
            tr!(
                "Cuenta que revisa (por defecto, la del proyecto)",
                "Account that reviews (by default, the project's)"
            ),
        ),
        (
            "skills import",
            "model",
            tr!("Modelo que revisa", "Model that reviews"),
        ),
        (
            "skills try",
            "",
            tr!(
                "Prueba una habilidad en tareas reales: cada una dos veces, con y sin ella, en copias aparte; gasta tokens",
                "Try a skill on real tasks: each twice, with and without it, in separate copies; spends tokens"
            ),
        ),
        (
            "skills try",
            "name",
            tr!("Nombre de la habilidad", "Name of the skill"),
        ),
        (
            "skills try",
            "tasks",
            tr!(
                "Una tarea de prueba (hasta 3); sin ninguna, la IA propone",
                "A test task (up to 3); with none, the AI proposes them"
            ),
        ),
        (
            "skills try",
            "check",
            tr!(
                "Comando que Hedy corre en cada copia para ver si acierta (por defecto, los tests del proyecto)",
                "Command Hedy runs in each copy to see whether it passes (by default, the project's tests)"
            ),
        ),
        (
            "skills try",
            "yes",
            tr!(
                "Sin preguntar (solo con --task y --check)",
                "Without asking (only with --task and --check)"
            ),
        ),
        (
            "skills try",
            "account",
            tr!(
                "Cuenta que propone las tareas (las tareas usan las del proyecto)",
                "Account that proposes the tasks (the tasks use the project's)"
            ),
        ),
        (
            "skills try",
            "model",
            tr!("Modelo de las tareas", "Model of the tasks"),
        ),
        (
            "skills check",
            "",
            tr!(
                "¿Ayuda o estorba? Lo que mide Hedy de una habilidad aprobada y lo que ve la IA contra tus notas",
                "Does it help or get in the way? What Hedy measures of an approved skill and what the AI sees against your notes"
            ),
        ),
        (
            "skills check",
            "name",
            tr!("Nombre de la habilidad", "Name of the skill"),
        ),
        (
            "skills check",
            "no_review",
            tr!(
                "Sin la revisión de la IA: solo lo que mide Hedy, sin gastar tokens",
                "Without the AI's review: only what Hedy measures, spending no tokens"
            ),
        ),
        (
            "skills check",
            "account",
            tr!(
                "Cuenta que revisa (por defecto, la del proyecto)",
                "Account that reviews (by default, the project's)"
            ),
        ),
        (
            "skills check",
            "model",
            tr!("Modelo que revisa", "Model that reviews"),
        ),
        (
            "skills export",
            "",
            tr!(
                "Exporta las habilidades aprobadas como archivos SKILL.md, el formato que lee cada CLI; nunca reemplaza un archivo",
                "Export the approved skills as SKILL.md files, the format every CLI reads; never replaces a file"
            ),
        ),
        (
            "skills export",
            "names",
            tr!(
                "Las habilidades que exportar (todas si no pones ninguna)",
                "The skills to export (all of them when none is given)"
            ),
        ),
        (
            "skills export",
            "to",
            tr!(
                "Carpeta donde escribirlas: <carpeta>/<nombre>/SKILL.md",
                "Folder to write them in: <folder>/<name>/SKILL.md"
            ),
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
            "sync",
            "",
            tr!(
                "Tu memoria, habilidades, plantillas y temas, iguales en todas tus PCs (por defecto, el estado)",
                "Your memory, skills, templates and themes, the same on every PC (by default, the status)"
            ),
        ),
        (
            "sync status",
            "",
            tr!(
                "Dónde sincroniza, tus PCs y los conflictos",
                "Where it syncs, your PCs and the conflicts"
            ),
        ),
        (
            "sync setup",
            "",
            tr!(
                "Configura la sincronización con un repo git privado y vacío, y muestra tu clave de recuperación",
                "Set up sync with an empty private git repo, and show your recovery key"
            ),
        ),
        (
            "sync setup",
            "url",
            tr!(
                "Dirección del repo (GitHub, GitLab, Bitbucket o tu propio servidor)",
                "The repo's URL (GitHub, GitLab, Bitbucket or your own server)"
            ),
        ),
        (
            "sync setup",
            "name",
            tr!(
                "Nombre de esta PC (por defecto, el del equipo)",
                "This PC's name (by default, the machine's)"
            ),
        ),
        (
            "sync join",
            "",
            tr!(
                "Suma esta PC con tu clave de recuperación",
                "Add this PC with your recovery key"
            ),
        ),
        (
            "sync join",
            "url",
            tr!(
                "Dirección del repo que ya usas en otra PC",
                "The URL of the repo you already use on another PC"
            ),
        ),
        (
            "sync join",
            "name",
            tr!(
                "Nombre de esta PC (por defecto, el del equipo)",
                "This PC's name (by default, the machine's)"
            ),
        ),
        ("sync now", "", tr!("Sincroniza ahora", "Sync now")),
        (
            "sync conflicts",
            "",
            tr!(
                "Notas y habilidades editadas a la vez en dos PCs",
                "Notes and skills edited at once on two PCs"
            ),
        ),
        (
            "sync resolve",
            "",
            tr!("Resuelve un conflicto", "Settle a conflict"),
        ),
        (
            "sync resolve",
            "id",
            tr!(
                "El número que muestra `hedy sync conflicts`",
                "The number `hedy sync conflicts` shows"
            ),
        ),
        (
            "sync resolve",
            "keep",
            tr!("Quédate con la versión en uso", "Keep the version in use"),
        ),
        (
            "sync resolve",
            "other",
            tr!("Usa la otra versión", "Use the other version"),
        ),
        (
            "sync owner",
            "",
            tr!(
                "Tus respuestas sobre los dueños de repos (o una respuesta nueva)",
                "Your answers about repo owners (or a new one)"
            ),
        ),
        (
            "sync owner",
            "owner",
            tr!(
                "Dueño con su servidor, p. ej. github.com/empresa-org",
                "The owner with its server, e.g. github.com/empresa-org"
            ),
        ),
        (
            "sync owner",
            "mine",
            tr!(
                "Su memoria es solo tuya: se sincroniza",
                "Its memory is yours alone: it syncs"
            ),
        ),
        (
            "sync owner",
            "shared",
            tr!(
                "Es de un grupo (empresa, cliente, comunidad): no sale de esta PC",
                "It belongs to a group (company, client, community): it stays on this PC"
            ),
        ),
        (
            "sync owner",
            "forget",
            tr!(
                "Olvida la respuesta: sus proyectos vuelven a preguntar",
                "Forget the answer: its projects ask again"
            ),
        ),
        (
            "sync project",
            "",
            tr!(
                "Dónde vive la memoria del proyecto actual (o cámbialo)",
                "Where the current project's memory lives (or change it)"
            ),
        ),
        (
            "sync project",
            "personal",
            tr!(
                "Personal: se sincroniza con todas tus PCs",
                "Personal: synced to all your PCs"
            ),
        ),
        (
            "sync project",
            "local",
            tr!("Solo en esta PC", "On this PC only"),
        ),
        (
            "sync project",
            "shared",
            tr!(
                "Compartida con un grupo, p. ej. github.com/empresa-org",
                "Shared with a group, e.g. github.com/empresa-org"
            ),
        ),
        (
            "sync project",
            "follow",
            tr!(
                "Según el dueño de su repo otra vez",
                "From its repo's owner again"
            ),
        ),
        (
            "sync remove",
            "",
            tr!(
                "Quita otra PC: no podrá leer lo que guardes desde ahora",
                "Remove another PC: it cannot read what you keep from now on"
            ),
        ),
        (
            "sync remove",
            "pc",
            tr!(
                "Su nombre o su id (`hedy sync` los muestra)",
                "Its name or its id (`hedy sync` shows them)"
            ),
        ),
        ("sync remove", "yes", tr!("Sin preguntar", "Without asking")),
        (
            "sync leave",
            "",
            tr!(
                "Deja de sincronizar en esta PC (tus notas se quedan)",
                "Stop syncing on this PC (your notes stay)"
            ),
        ),
        ("sync leave", "yes", tr!("Sin preguntar", "Without asking")),
        (
            "team",
            "",
            tr!(
                "La memoria compartida de un equipo, en su propio repo git (por defecto, el estado)",
                "A team's shared memory, in its own git repo (by default, the status)"
            ),
        ),
        (
            "team",
            "team",
            tr!(
                "El equipo (nombre o id; por defecto, el del proyecto actual o el único)",
                "The team (name or id; by default, the current project's or the only one)"
            ),
        ),
        (
            "team status",
            "",
            tr!(
                "Personas, invitaciones y lo que espera revisión",
                "People, invites and what waits for review"
            ),
        ),
        (
            "team create",
            "",
            tr!(
                "Crea un equipo en un repo git privado y vacío; eres su admin",
                "Create a team in an empty private git repo; you are its admin"
            ),
        ),
        (
            "team create",
            "url",
            tr!("Dirección del repo", "The repo's address"),
        ),
        (
            "team create",
            "name",
            tr!("Nombre del equipo", "The team's name"),
        ),
        (
            "team create",
            "me",
            tr!("Tu nombre en el equipo", "Your name in the team"),
        ),
        (
            "team create",
            "pc",
            tr!(
                "Nombre de esta PC (por defecto, el de la máquina)",
                "This PC's name (by default, the machine's)"
            ),
        ),
        (
            "team invite",
            "",
            tr!(
                "Crea un código que sirve una vez, por 48 horas",
                "Make a code that works once, for 48 hours"
            ),
        ),
        (
            "team invite",
            "admin",
            tr!(
                "La persona entra como admin",
                "The person joins as an admin"
            ),
        ),
        (
            "team invite",
            "mine",
            tr!("Para otra PC tuya", "For another PC of yours"),
        ),
        (
            "team invite",
            "cancel",
            tr!(
                "Cancela la invitación con ese número",
                "Cancel the invite with that number"
            ),
        ),
        (
            "team join",
            "",
            tr!(
                "Entra a un equipo con una invitación (el código se pregunta sin mostrarlo, o se lee de la entrada)",
                "Join a team with an invite (the code is asked, hidden, or read from stdin)"
            ),
        ),
        (
            "team join",
            "url",
            tr!("Dirección del repo del equipo", "The team repo's address"),
        ),
        (
            "team join",
            "me",
            tr!("Tu nombre en el equipo", "Your name in the team"),
        ),
        (
            "team join",
            "pc",
            tr!(
                "Nombre de esta PC (por defecto, el de la máquina)",
                "This PC's name (by default, the machine's)"
            ),
        ),
        (
            "team now",
            "",
            tr!(
                "Sincroniza el equipo ahora (o mira si ya te dejaron entrar)",
                "Sync the team now (or see whether you were let in)"
            ),
        ),
        (
            "team link",
            "",
            tr!(
                "La memoria del proyecto actual pasa a ser del equipo",
                "The current project's memory becomes the team's"
            ),
        ),
        (
            "team notes",
            "",
            tr!("Las notas del equipo", "The team's notes"),
        ),
        (
            "team add",
            "",
            tr!(
                "Agrega una nota del equipo al proyecto actual",
                "Add a team note to the current project"
            ),
        ),
        (
            "team add",
            "text",
            tr!("Texto de la nota", "The note's text"),
        ),
        (
            "team add",
            "everyone",
            tr!(
                "A la capa Equipo (siempre en el contexto; la de un miembro es una sugerencia)",
                "To the team layer (always in context; a member's is a suggestion)"
            ),
        ),
        (
            "team add",
            "category",
            tr!(
                "decision, convention, rule o preference",
                "decision, convention, rule or preference"
            ),
        ),
        (
            "team publish",
            "",
            tr!(
                "Pasa una nota de esta PC (`hedy memory` las lista) al equipo",
                "Move a note of this PC (`hedy memory` lists them) to the team"
            ),
        ),
        ("team publish", "id", tr!("Id de la nota", "The note's id")),
        (
            "team publish",
            "everyone",
            tr!("A la capa Equipo", "To the team layer"),
        ),
        (
            "team review",
            "",
            tr!(
                "Las sugerencias, o acepta o rechaza una",
                "The suggestions, or accept or turn one down"
            ),
        ),
        (
            "team review",
            "id",
            tr!(
                "La sugerencia (el final de su id)",
                "The suggestion (the end of its id)"
            ),
        ),
        ("team review", "accept", tr!("Acéptala", "Accept it")),
        ("team review", "reject", tr!("Recházala", "Turn it down")),
        (
            "team review",
            "yes",
            tr!(
                "Acepta sin mostrarla ni preguntar",
                "Accept without showing it or asking"
            ),
        ),
        (
            "team role",
            "",
            tr!(
                "Cambia el rol de una persona (admins)",
                "Change a person's role (admins)"
            ),
        ),
        (
            "team role",
            "person",
            tr!(
                "Su nombre o el comienzo de su id",
                "Their name or the start of their id"
            ),
        ),
        ("team role", "admin", tr!("Admin", "Admin")),
        ("team role", "member", tr!("Miembro", "Member")),
        (
            "team remove",
            "",
            tr!(
                "Quita a una persona del equipo (admins), o una PC",
                "Remove a person from the team (admins), or a PC"
            ),
        ),
        (
            "team remove",
            "who",
            tr!("Su nombre o su id", "Their name or their id"),
        ),
        (
            "team remove",
            "pc",
            tr!("Es una PC, no una persona", "It is a PC, not a person"),
        ),
        ("team remove", "yes", tr!("Sin preguntar", "Without asking")),
        (
            "team rename",
            "",
            tr!(
                "Cambia el nombre del equipo, o el de una persona (admins)",
                "Rename the team, or a person (admins)"
            ),
        ),
        (
            "team rename",
            "name",
            tr!("El nombre nuevo", "The new name"),
        ),
        (
            "team rename",
            "person",
            tr!(
                "La persona (su nombre o id)",
                "The person (their name or id)"
            ),
        ),
        (
            "team leave",
            "",
            tr!("Sal del equipo en esta PC", "Leave the team on this PC"),
        ),
        ("team leave", "yes", tr!("Sin preguntar", "Without asking")),
        (
            "team leave",
            "here",
            tr!(
                "Solo olvida el equipo en esta PC, sin pedir salir (un admin la quita después)",
                "Only forget the team on this PC, without asking to leave (an admin removes it later)"
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
        ("question", tr!("PREGUNTA", "QUESTION")),
        ("model", tr!("MODELO", "MODEL")),
        ("url", tr!("DIRECCIÓN", "URL")),
        ("owner", tr!("DUEÑO", "OWNER")),
        ("pc", tr!("PC", "PC")),
        ("code", tr!("CÓDIGO", "CODE")),
        ("me", tr!("NOMBRE", "NAME")),
        ("who", tr!("QUIÉN", "WHO")),
        ("person", tr!("PERSONA", "PERSON")),
        ("team", tr!("EQUIPO", "TEAM")),
        ("revs", tr!("COMMITS", "COMMITS")),
        ("task", tr!("TAREA", "TASK")),
        ("path", tr!("RUTA", "PATH")),
        ("names", tr!("NOMBRES", "NAMES")),
        ("tasks", tr!("TAREA", "TASK")),
        ("check", tr!("COMANDO", "COMMAND")),
        ("to", tr!("CARPETA", "FOLDER")),
        ("cancel", tr!("NÚMERO", "NUMBER")),
        ("category", tr!("CATEGORÍA", "CATEGORY")),
        // `hedy sync project --shared <DUEÑO>` (a flag elsewhere, without one).
        ("shared", tr!("DUEÑO", "OWNER")),
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

    Permissions {
        #[arg(value_parser = ["manual", "edits", "auto"])]
        mode: Option<String>,
    },

    Block {
        patterns: Vec<String>,
        #[arg(long, requires = "patterns", conflicts_with_all = ["usual", "clear"])]
        remove: bool,
        #[arg(long, conflicts_with = "clear")]
        usual: bool,
        #[arg(long, conflicts_with = "patterns")]
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

    Ask(AskArgs),

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

    Commands {
        #[arg(long, short)]
        account: Option<String>,
        #[arg(long)]
        json: bool,
    },

    Review {
        #[arg(conflicts_with_all = ["staged", "task"])]
        revs: Option<String>,
        #[arg(long, conflicts_with = "task")]
        staged: bool,
        #[arg(long)]
        task: Option<String>,
        #[arg(long, short)]
        account: Option<String>,
        #[arg(long, short)]
        model: Option<String>,
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

    Sync {
        #[command(subcommand)]
        command: Option<SyncCommand>,
    },

    Team {
        #[arg(long, global = true)]
        team: Option<String>,
        #[command(subcommand)]
        command: Option<TeamCommand>,
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

    Ask(MemoryAsk),

    Approve { id: i64 },

    Reject { id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum SkillsCommand {
    List,

    Show {
        name: String,
    },

    Approve {
        id: i64,
    },

    Reject {
        id: i64,
    },

    Remove {
        name: String,
    },

    Import {
        path: PathBuf,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        replace: bool,
        #[arg(long, short)]
        yes: bool,
        #[arg(long, conflicts_with = "no_review")]
        review: bool,
        #[arg(long)]
        no_review: bool,
        #[arg(long, short)]
        account: Option<String>,
        #[arg(long, short)]
        model: Option<String>,
    },

    Try {
        name: String,
        #[arg(long = "task")]
        tasks: Vec<String>,
        #[arg(long)]
        check: Option<String>,
        #[arg(long, short)]
        yes: bool,
        #[arg(long, short)]
        account: Option<String>,
        #[arg(long, short)]
        model: Option<String>,
    },

    Check {
        name: String,
        #[arg(long)]
        no_review: bool,
        #[arg(long, short)]
        account: Option<String>,
        #[arg(long, short)]
        model: Option<String>,
    },

    Export {
        names: Vec<String>,
        #[arg(long)]
        to: PathBuf,
    },
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
pub enum SyncCommand {
    Status,

    Setup {
        url: String,
        #[arg(long)]
        name: Option<String>,
    },

    Join {
        url: String,
        #[arg(long)]
        name: Option<String>,
    },

    Now,

    Conflicts,

    #[command(group(clap::ArgGroup::new("choice").required(true).args(["keep", "other"])))]
    Resolve {
        id: i64,
        #[arg(long)]
        keep: bool,
        #[arg(long)]
        other: bool,
    },

    #[command(group(clap::ArgGroup::new("answer").args(["mine", "shared", "forget"])))]
    Owner {
        #[arg(
            required_if_eq_any([("mine", "true"), ("shared", "true"), ("forget", "true")]),
            requires = "answer"
        )]
        owner: Option<String>,
        #[arg(long)]
        mine: bool,
        #[arg(long)]
        shared: bool,
        #[arg(long)]
        forget: bool,
    },

    #[command(group(clap::ArgGroup::new("home").args(["personal", "local", "shared", "follow"])))]
    Project {
        #[arg(long)]
        personal: bool,
        #[arg(long)]
        local: bool,
        #[arg(long)]
        shared: Option<String>,
        #[arg(long)]
        follow: bool,
    },

    Remove {
        pc: String,
        #[arg(long, short)]
        yes: bool,
    },

    Leave {
        #[arg(long, short)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum TeamCommand {
    Status,

    Create {
        url: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        me: Option<String>,
        #[arg(long)]
        pc: Option<String>,
    },

    Invite {
        #[arg(long)]
        admin: bool,
        #[arg(long, conflicts_with = "admin")]
        mine: bool,
        #[arg(long, conflicts_with_all = ["admin", "mine"])]
        cancel: Option<u32>,
    },

    Join {
        url: String,
        #[arg(long)]
        me: Option<String>,
        #[arg(long)]
        pc: Option<String>,
    },

    Now,

    Link,

    Notes,

    Add {
        #[arg(required = true)]
        text: Vec<String>,
        #[arg(long)]
        everyone: bool,
        #[arg(long, default_value = "convention")]
        category: String,
    },

    Publish {
        id: i64,
        #[arg(long)]
        everyone: bool,
    },

    Review {
        id: Option<String>,
        #[arg(long, requires = "id")]
        accept: bool,
        #[arg(long, requires = "id", conflicts_with = "accept")]
        reject: bool,
        #[arg(long, short)]
        yes: bool,
    },

    #[command(group(clap::ArgGroup::new("new_role").required(true).args(["admin", "member"])))]
    Role {
        person: String,
        #[arg(long)]
        admin: bool,
        #[arg(long)]
        member: bool,
    },

    Remove {
        who: String,
        #[arg(long)]
        pc: bool,
        #[arg(long, short)]
        yes: bool,
    },

    Rename {
        name: String,
        #[arg(long)]
        person: Option<String>,
    },

    Leave {
        #[arg(long, short)]
        yes: bool,
        #[arg(long)]
        here: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Get { key: String },
    Set { key: String, value: String },
    Unset { key: String },
}

#[derive(Debug, Args)]
pub struct AskArgs {
    pub question: Vec<String>,
    #[arg(long, short, conflicts_with = "follow")]
    pub account: Option<String>,
    #[arg(long, short, conflicts_with = "follow")]
    pub model: Option<String>,
    #[arg(long, short, conflicts_with = "follow")]
    pub research: bool,
    /// `Some("")`: the most recent conversation. An id needs `=`, so that
    /// `-c "question"` stays a question.
    #[arg(
        long = "continue",
        short = 'c',
        id = "follow",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "",
        value_name = "ID"
    )]
    pub follow: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct MemoryAsk {
    #[arg(required = true)]
    pub question: Vec<String>,
    #[arg(long, conflicts_with = "all")]
    pub team: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long, conflicts_with_all = ["team", "all"])]
    pub code: bool,
    #[arg(long, short)]
    pub account: Option<String>,
    #[arg(long, short)]
    pub model: Option<String>,
    #[arg(long)]
    pub json: bool,
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
        assert!(parse(&["hedy", "block"]).is_ok());
        assert!(parse(&["hedy", "permissions"]).is_ok());
        assert!(parse(&["hedy", "permissions", "auto"]).is_ok());
        assert!(parse(&["hedy", "permissions", "bypass"]).is_err());
        assert!(parse(&["hedy", "block", ".env", "*.pem"]).is_ok());
        assert!(parse(&["hedy", "block", "--usual"]).is_ok());
        assert!(parse(&["hedy", "block", "--remove"]).is_err());
        assert!(parse(&["hedy", "block", ".env", "--clear"]).is_err());
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
        assert!(matches!(
            parse(&["hedy", "skills", "import", "~/skills", "--replace", "-y"]),
            Some(Command::Skills {
                command: Some(SkillsCommand::Import {
                    replace: true,
                    yes: true,
                    global: false,
                    ..
                })
            })
        ));
        assert!(matches!(
            parse(&["hedy", "skills", "export", "lint", "e2e", "--to", "out"]),
            Some(Command::Skills {
                command: Some(SkillsCommand::Export { names, .. })
            }) if names == ["lint", "e2e"]
        ));
        assert!(
            Cli::try_parse_from(["hedy", "skills", "export"]).is_err(),
            "--to is needed"
        );
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
    fn ask_takes_words_and_an_optional_conversation_to_follow() {
        let ask = |args: &[&str]| {
            let mut argv = vec!["hedy", "ask"];
            argv.extend_from_slice(args);
            match Cli::try_parse_from(argv).map(|c| c.command) {
                Ok(Some(Command::Ask(ask))) => Ok(ask),
                Ok(other) => panic!("expected ask, got {other:?}"),
                Err(e) => Err(e),
            }
        };
        let plain = ask(&["-m", "haiku", "¿qué", "es?"]).unwrap();
        assert_eq!(plain.question, vec!["¿qué", "es?"]);
        assert_eq!(plain.model.as_deref(), Some("haiku"));
        assert_eq!(plain.follow, None);

        // `-c` never takes the question as an id.
        let latest = ask(&["-c", "¿y en Python?"]).unwrap();
        assert_eq!(latest.follow.as_deref(), Some(""));
        assert_eq!(latest.question, vec!["¿y en Python?"]);
        let given = ask(&["--continue=t1", "más"]).unwrap();
        assert_eq!(given.follow.as_deref(), Some("t1"));
        assert_eq!(given.question, vec!["más"]);

        assert_eq!(ask(&["-"]).unwrap().question, vec!["-"]);
        assert!(ask(&[]).unwrap().question.is_empty());
        // A follow-up keeps the conversation's account, model and mode.
        assert!(ask(&["-c", "-a", "x", "q"]).is_err());
        assert!(ask(&["-c", "-r", "q"]).is_err());
        assert!(ask(&["-c", "-m", "opus", "q"]).is_err());
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
    fn commands_takes_an_account_and_json() {
        assert!(matches!(
            Cli::try_parse_from(["hedy", "commands", "-a", "personal", "--json"])
                .unwrap()
                .command,
            Some(Command::Commands { account: Some(a), json: true }) if a == "personal"
        ));
        assert!(matches!(
            Cli::try_parse_from(["hedy", "commands"]).unwrap().command,
            Some(Command::Commands {
                account: None,
                json: false
            })
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
