use std::path::PathBuf;

use super::ports::{MergeOutcome, PortError};
use crate::domain::{AccountId, DomainError, Provider, TaskId};
use crate::tr;

/// Why a use case failed. Messages are user-facing, in the current language.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("{}", self.message())]
    AccountNotFound(String),
    #[error("{}", self.message())]
    AccountExists(AccountId),
    #[error("{}", self.message())]
    DefaultFolderTaken(&'static str),
    #[error("{}", self.message())]
    AccountInUse {
        account: AccountId,
        projects: Vec<PathBuf>,
    },
    #[error("{}", self.message())]
    NoAccountResolved,
    #[error("{}", self.message())]
    NotASubscription(AccountId),
    #[error("{}", self.message())]
    NotAnApiAccount(AccountId),
    #[error("{}", self.message())]
    EmptyApiKey,
    #[error("{}", self.message())]
    DefaultCannotBeProtected(AccountId),
    #[error("{}", self.message())]
    InvalidSettingValue { key: String, value: String },
    #[error("{}", self.message())]
    ProjectNotFound(PathBuf),
    #[error("{}", self.message())]
    TaskNotFound(TaskId),
    #[error("{}", self.message())]
    TaskNotRunning(TaskId),
    #[error("{}", self.message())]
    TaskClosed(TaskId),
    #[error("{}", self.message())]
    TaskWithoutChanges(TaskId),
    #[error("{}", self.message())]
    MissingApiKey(AccountId),
    #[error("{}", self.message())]
    NotAHandoffCandidate(String),
    #[error("{}", self.message())]
    NoteNotFound(i64),
    #[error("{}", self.message())]
    EmptyNote,
    #[error("{}", self.message())]
    EmptyPrompt,
    #[error("{}", self.message())]
    ProviderNotYetSupported(Provider),
    #[error("{}", self.message())]
    MergeBlocked {
        branch: String,
        reason: MergeOutcome,
    },
    #[error("{}", self.message())]
    UnknownSetting(String),
}

pub type AppResult<T> = Result<T, AppError>;

fn paths(list: &[PathBuf]) -> String {
    list.iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl AppError {
    fn message(&self) -> String {
        match self {
            Self::Domain(e) => e.to_string(),
            Self::Port(e) => e.to_string(),
            Self::AccountNotFound(id) => tr!(
                "no existe la cuenta «{id}» (mira `hedy account list`)",
                "account «{id}» does not exist (see `hedy account list`)"
            ),
            Self::AccountExists(id) => tr!(
                "ya existe una cuenta con el identificador «{id}»",
                "an account with id «{id}» already exists"
            ),
            Self::DefaultFolderTaken(cli) => tr!(
                "ya hay una cuenta que usa la carpeta por defecto de {cli}",
                "another account already uses the default folder of {cli}"
            ),
            Self::AccountInUse { account, projects } => {
                let projects = paths(projects);
                tr!(
                    "la cuenta «{account}» está asignada a {projects}; asígnales otra antes de borrarla",
                    "account «{account}» is assigned to {projects}; assign them another one before removing it"
                )
            }
            Self::NoAccountResolved => tr!(
                "este proyecto no tiene cuenta y no hay cuenta por defecto (usa `hedy use <cuenta>`)",
                "this project has no account and there is no default account (use `hedy use <account>`)"
            ),
            Self::NotASubscription(id) => tr!(
                "la cuenta «{id}» es por API key: no tiene login de CLI",
                "account «{id}» uses an API key: it has no CLI login"
            ),
            Self::NotAnApiAccount(id) => tr!(
                "la cuenta «{id}» es de suscripción: no usa API key",
                "account «{id}» is a subscription: it uses no API key"
            ),
            Self::EmptyApiKey => tr!("la API key está vacía", "the API key is empty"),
            Self::DefaultCannotBeProtected(id) => tr!(
                "«{id}» es la cuenta por defecto y no se puede reservar para un proyecto protegido; elige otra cuenta por defecto (`hedy account default <cuenta>`) o quita la protección (`--unprotect`)",
                "«{id}» is the default account and cannot be reserved for a protected project; pick another default (`hedy account default <account>`) or remove the protection (`--unprotect`)"
            ),
            Self::InvalidSettingValue { key, value } => tr!(
                "valor no válido para «{key}»: «{value}»",
                "invalid value for «{key}»: «{value}»"
            ),
            Self::ProjectNotFound(path) => {
                let path = path.display();
                tr!(
                    "no existe el proyecto {path}",
                    "project {path} does not exist"
                )
            }
            Self::TaskNotFound(id) => tr!("no existe la tarea {id}", "task {id} does not exist"),
            Self::TaskNotRunning(id) => tr!(
                "la tarea {id} no tiene un agente en marcha",
                "task {id} has no agent running"
            ),
            Self::TaskClosed(id) => tr!(
                "la tarea {id} ya está cerrada",
                "task {id} is already closed"
            ),
            Self::TaskWithoutChanges(id) => tr!(
                "la tarea {id} no cambia archivos (es una pregunta)",
                "task {id} changes no files (it is a question)"
            ),
            Self::MissingApiKey(id) => tr!(
                "la cuenta «{id}» no tiene API key guardada",
                "account «{id}» has no saved API key"
            ),
            Self::NotAHandoffCandidate(id) => tr!(
                "no se puede pasar la tarea a «{id}»: elige otro proveedor o una API key",
                "the task cannot move to «{id}»: pick another provider or an API key"
            ),
            Self::NoteNotFound(id) => tr!("no existe la nota {id}", "note {id} does not exist"),
            Self::EmptyNote => tr!("la nota está vacía", "the note is empty"),
            Self::EmptyPrompt => tr!("escribe qué quieres que haga", "write what you want done"),
            Self::ProviderNotYetSupported(provider) => tr!(
                "las tareas con {provider} todavía no están disponibles en Hedy",
                "tasks with {provider} are not available in Hedy yet"
            ),
            Self::MergeBlocked { branch, reason } => {
                let why = match reason {
                    MergeOutcome::NotOnBase { current } => tr!(
                        "tu carpeta principal está en la rama «{current}», no en la rama base",
                        "your main folder is on branch «{current}», not on the base branch"
                    ),
                    MergeOutcome::Dirty => tr!(
                        "tu carpeta principal tiene cambios sin guardar en git",
                        "your main folder has uncommitted changes"
                    ),
                    MergeOutcome::Conflict => {
                        tr!(
                            "hay conflictos con la rama base",
                            "it conflicts with the base branch"
                        )
                    }
                    MergeOutcome::Merged => tr!("ya está unida", "it is already merged"),
                };
                tr!(
                    "Los cambios quedaron guardados en la rama «{branch}», pero no se pudieron unir: {why}. Resuélvelo y vuelve a aceptar, o únela tú con git.",
                    "The changes are saved on branch «{branch}», but could not be merged: {why}. Fix it and accept again, or merge it yourself with git."
                )
            }
            Self::UnknownSetting(key) => {
                tr!("ajuste desconocido: «{key}»", "unknown setting: «{key}»")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, with_language};

    #[test]
    fn messages_follow_the_language() {
        let error = AppError::EmptyNote;
        assert_eq!(
            with_language(Language::English, || error.to_string()),
            "the note is empty"
        );
        assert_eq!(error.to_string(), "la nota está vacía");
    }
}
