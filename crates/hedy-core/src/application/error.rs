use std::path::PathBuf;

use super::ports::PortError;
use crate::domain::{AccountId, DomainError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error(transparent)]
    Port(#[from] PortError),

    #[error("no existe la cuenta «{0}» (mira `hedy account list`)")]
    AccountNotFound(String),

    #[error("ya existe una cuenta con el identificador «{0}»")]
    AccountExists(AccountId),

    #[error("ya hay una cuenta que usa la carpeta por defecto de {0}")]
    DefaultFolderTaken(&'static str),

    #[error(
        "la cuenta «{account}» está asignada a {}; asígnales otra antes de borrarla",
        projects.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    )]
    AccountInUse {
        account: AccountId,
        projects: Vec<PathBuf>,
    },

    #[error("este proyecto no tiene cuenta y no hay cuenta por defecto (usa `hedy use <cuenta>`)")]
    NoAccountResolved,

    #[error("la cuenta «{0}» es por API key: no tiene login de CLI")]
    NotASubscription(AccountId),

    #[error("la cuenta «{0}» es de suscripción: no usa API key")]
    NotAnApiAccount(AccountId),

    #[error("la API key está vacía")]
    EmptyApiKey,

    #[error(
        "«{0}» es la cuenta por defecto y no se puede reservar; elige otra con `hedy account default <cuenta>`"
    )]
    DefaultCannotBeProtected(AccountId),

    #[error("ajuste desconocido: «{0}»")]
    UnknownSetting(String),
}

pub type AppResult<T> = Result<T, AppError>;
