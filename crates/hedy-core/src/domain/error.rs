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
    SkillRunsCommands,
    #[error("{}", self.message())]
    SkillHiddenText,
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
    #[error("{}", self.message())]
    InvalidTemplate(String),
    #[error("{}", self.message())]
    InvalidTemplateName(String),
    #[error("{}", self.message())]
    EmptyTemplate,
    #[error("{}", self.message())]
    InvalidProjectName(String),
    #[error("{}", self.message())]
    ImageTooLarge(String),
    #[error("{}", self.message())]
    UnsupportedImage(String),
    #[error("{}", self.message())]
    TooManyImages(usize),
    #[error("{}", self.message())]
    InvalidThemeName(String),
    #[error("{}", self.message())]
    InvalidThemeId(String),
    #[error("{}", self.message())]
    InvalidThemeBase(String),
    #[error("{}", self.message())]
    ThemeColorCount { count: usize, max: usize },
    #[error("{}", self.message())]
    InvalidThemeColorKey(String),
    #[error("{}", self.message())]
    InvalidThemeColor { key: String, value: String },
    #[error("{}", self.message())]
    InvalidThemeDescription { max: usize },
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
                "nombre de habilidad no válido: «{name}» (minúsculas, números y guiones, hasta 64 caracteres; p. ej. correr-tests-e2e)",
                "invalid skill name: «{name}» (lowercase letters, digits and dashes, up to 64 characters; e.g. run-e2e-tests)"
            ),
            Self::InvalidSkillDescription => tr!(
                "la descripción de la habilidad debe ser una línea de hasta 1024 caracteres, sin < ni >, que diga cuándo usarla",
                "the skill's description must be one line of up to 1024 characters, without < or >, saying when to use it"
            ),
            Self::InvalidSkillBody => tr!(
                "el contenido de la habilidad no puede estar vacío ni pasar de 80000 caracteres",
                "the skill's body cannot be empty or longer than 80000 characters"
            ),
            Self::SkillLooksSecret(kind) => tr!(
                "la habilidad parece contener un secreto ({kind}); no se guardó. Quítalo y usa un marcador como $NOMBRE_VARIABLE o <tu clave>",
                "the skill looks like it contains a secret ({kind}); it was not saved. Remove it and use a placeholder such as $VARIABLE_NAME or <your key>"
            ),
            Self::SkillHiddenText => tr!(
                "la habilidad tiene texto oculto (caracteres invisibles o que reordenan el texto): las IAs lo leerían y tú no lo verías",
                "the skill has hidden text (invisible characters or ones that reorder the text): the AIs would read it and you would not see it"
            ),
            Self::SkillRunsCommands => tr!(
                "una habilidad propuesta no puede correr comandos al cargarse (!`…` o un bloque ```!); escribe el comando como un paso para que la IA lo pida",
                "a proposed skill cannot run commands when it loads (!`…` or a ```! block); write the command as a step for the AI to ask for"
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
            Self::InvalidTemplate(id) => {
                tr!("plantilla no válida: «{id}»", "invalid template: «{id}»")
            }
            Self::InvalidTemplateName(name) => tr!(
                "nombre de plantilla no válido: «{name}» (una línea de hasta 60 caracteres)",
                "invalid template name: «{name}» (one line of up to 60 characters)"
            ),
            Self::EmptyTemplate => tr!(
                "la plantilla está vacía (o es demasiado larga)",
                "the template is empty (or too long)"
            ),
            Self::InvalidProjectName(name) if name.is_empty() => {
                tr!("ponle un nombre al proyecto", "give the project a name")
            }
            Self::ImageTooLarge(name) => tr!(
                "«{name}» pesa más de 5 MB: reduce la imagen o recórtala",
                "«{name}» is larger than 5 MB: shrink or crop the image"
            ),
            Self::UnsupportedImage(name) => tr!(
                "«{name}» no es una imagen PNG, JPEG, GIF o WebP",
                "«{name}» is not a PNG, JPEG, GIF or WebP image"
            ),
            Self::TooManyImages(max) => tr!(
                "como mucho {max} imágenes por mensaje",
                "at most {max} images per message"
            ),
            Self::InvalidProjectName(name) => tr!(
                "«{name}» no sirve como nombre de carpeta: usa letras o números",
                "«{name}» does not work as a folder name: use letters or digits"
            ),
            Self::InvalidThemeName(_) => tr!(
                "ponle al tema un nombre de hasta 60 caracteres",
                "give the theme a name of up to 60 characters"
            ),
            Self::InvalidThemeId(id) => tr!(
                "identificador de tema no válido: «{id}» (minúsculas, números y guiones, hasta 40)",
                "invalid theme id: «{id}» (lowercase letters, digits and dashes, up to 40)"
            ),
            Self::InvalidThemeBase(base) => tr!(
                "base de tema no válida: «{base}» (usa dark o light)",
                "invalid theme base: «{base}» (use dark or light)"
            ),
            Self::ThemeColorCount { count, max } => tr!(
                "un tema necesita entre 1 y {max} colores (tiene {count})",
                "a theme needs between 1 and {max} colors (it has {count})"
            ),
            Self::InvalidThemeColorKey(key) => tr!(
                "nombre de color no válido: «{key}» (una letra y luego letras o números, hasta 40)",
                "invalid color name: «{key}» (a letter, then letters or digits, up to 40)"
            ),
            Self::InvalidThemeColor { key, value } => tr!(
                "color no válido para «{key}»: «{value}» (usa #RGB, #RRGGBB o #RRGGBBAA)",
                "invalid color for «{key}»: «{value}» (use #RGB, #RRGGBB or #RRGGBBAA)"
            ),
            Self::InvalidThemeDescription { max } => tr!(
                "describe el tema en 1 a {max} caracteres",
                "describe the theme in 1 to {max} characters"
            ),
        }
    }
}
