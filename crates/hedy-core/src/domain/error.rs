use std::path::PathBuf;

use super::{AccountId, Provider};
use crate::tr;

/// Violations of domain rules. Messages are user-facing, in the current language.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("{}", self.message())]
    UnknownProvider(String),
    #[error("{}", self.message())]
    InvalidAccountId(String),
    #[error("{}", self.message())]
    EmptyAccountName,
    #[error("{}", self.message())]
    InvalidColor(String),
    #[error("{}", self.message())]
    UnknownCategory(String),
    #[error("{}", self.message())]
    InvalidTopic(String),
    #[error("{}", self.message())]
    InvalidPrice(String),
    #[error("{}", self.message())]
    InvalidSkillName(String),
    #[error("{}", self.message())]
    InvalidSkillDescription,
    #[error("{}", self.message())]
    InvalidSkillBody,
    #[error("{}", self.message())]
    SkillLooksSecret(&'static str),
    #[error("{}", self.message())]
    ApiKeyNotSupported { provider: Provider },
    #[error("{}", self.message())]
    NothingToLaunch,
    #[error("{}", self.message())]
    SingleAccountProvider { provider: Provider },
    #[error("{}", self.message())]
    AccountReserved {
        account: AccountId,
        project: PathBuf,
    },
    #[error("{}", self.message())]
    ProtectedProject {
        project: PathBuf,
        assigned: AccountId,
    },
    #[error("{}", self.message())]
    ProtectWithoutAccount,
    #[error("{}", self.message())]
    AccountShared {
        account: AccountId,
        others: Vec<PathBuf>,
    },
}

impl DomainError {
    fn message(&self) -> String {
        match self {
            Self::UnknownProvider(p) => tr!(
                "proveedor desconocido: {p} (usa claude, codex, gemini, kimi o antigravity)",
                "unknown provider: {p} (use claude, codex, gemini, kimi or antigravity)"
            ),
            Self::InvalidAccountId(id) => tr!(
                "identificador de cuenta no válido: «{id}» (solo minúsculas, números y guiones)",
                "invalid account id: «{id}» (lowercase letters, digits and dashes only)"
            ),
            Self::EmptyAccountName => tr!(
                "el nombre de la cuenta no puede estar vacío",
                "the account name cannot be empty"
            ),
            Self::InvalidColor(c) => tr!(
                "color no válido: «{c}» (usa un nombre de la paleta o #RRGGBB)",
                "invalid color: «{c}» (use a palette name or #RRGGBB)"
            ),
            Self::UnknownCategory(c) => tr!(
                "categoría desconocida: {c} (decisión, convención, regla o preferencia)",
                "unknown category: {c} (decision, convention, rule or preference)"
            ),
            Self::InvalidTopic(t) => tr!(
                "tema no válido: «{t}» (usa una ruta corta en minúsculas, p. ej. arquitectura/pagos)",
                "invalid topic: «{t}» (use a short lowercase path, e.g. architecture/payments)"
            ),
            Self::InvalidPrice(p) => tr!(
                "precio no válido: «{p}» (usa entrada/salida en USD por millón de tokens, p. ej. 0.60/2.50)",
                "invalid price: «{p}» (use input/output in USD per million tokens, e.g. 0.60/2.50)"
            ),
            Self::InvalidSkillName(name) => tr!(
                "nombre de habilidad no válido: «{name}» (minúsculas, números y guiones, hasta 64 caracteres, sin «claude» ni «anthropic»; p. ej. correr-tests-e2e)",
                "invalid skill name: «{name}» (lowercase letters, digits and dashes, up to 64 characters, without «claude» or «anthropic»; e.g. run-e2e-tests)"
            ),
            Self::InvalidSkillDescription => tr!(
                "la descripción de la habilidad debe ser una línea de hasta 1024 caracteres, sin < ni >, que diga cuándo usarla",
                "the skill's description must be one line of up to 1024 characters, without < or >, saying when to use it"
            ),
            Self::InvalidSkillBody => tr!(
                "el contenido de la habilidad no puede estar vacío ni pasar de 20000 caracteres",
                "the skill's body cannot be empty or longer than 20000 characters"
            ),
            Self::SkillLooksSecret(kind) => tr!(
                "la habilidad parece contener un secreto ({kind}); no se guardó. Quítalo y usa un marcador como $NOMBRE_VARIABLE o <tu clave>",
                "the skill looks like it contains a secret ({kind}); it was not saved. Remove it and use a placeholder such as $VARIABLE_NAME or <your key>"
            ),
            Self::ApiKeyNotSupported { provider } => tr!(
                "{provider} no admite cuentas por API key",
                "{provider} does not take API-key accounts"
            ),
            Self::NothingToLaunch => tr!(
                "las cuentas por API key no tienen CLI que lanzar; se usan desde el chat de Hedy",
                "API-key accounts have no CLI to launch; use them from Hedy's chat"
            ),
            Self::SingleAccountProvider { provider } => tr!(
                "{provider} guarda la sesión en el llavero del sistema y no permite elegir carpeta: solo admite una cuenta por equipo, en su carpeta por defecto",
                "{provider} keeps its login in the system keyring and cannot use another folder: only one account per computer, in its default folder"
            ),
            Self::AccountReserved { account, project } => {
                let project = project.display();
                tr!(
                    "la cuenta «{account}» está reservada para el proyecto protegido {project}",
                    "account «{account}» is reserved for the protected project {project}"
                )
            }
            Self::ProtectedProject { project, assigned } => {
                let project = project.display();
                tr!(
                    "el proyecto {project} está protegido y solo puede usar la cuenta «{assigned}»",
                    "project {project} is protected and can only use account «{assigned}»"
                )
            }
            Self::ProtectWithoutAccount => tr!(
                "para proteger un proyecto primero asígnale una cuenta con `hedy use <cuenta>`",
                "to protect a project, first assign it an account with `hedy use <account>`"
            ),
            Self::AccountShared { account, others } => {
                let others = others
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                tr!(
                    "no se puede proteger: la cuenta «{account}» también la usan {others}",
                    "cannot protect it: account «{account}» is also used by {others}"
                )
            }
        }
    }
}
