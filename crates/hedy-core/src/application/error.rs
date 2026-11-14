use std::path::PathBuf;

use super::ports::{MergeOutcome, PortError};
use crate::domain::{AccountId, DomainError, Provider, TaskId};

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

    #[error("valor no válido para «{key}»: «{value}»")]
    InvalidSettingValue { key: String, value: String },

    #[error("no existe el proyecto {}", .0.display())]
    ProjectNotFound(PathBuf),

    #[error("no existe la tarea {0}")]
    TaskNotFound(TaskId),

    #[error("la tarea {0} no tiene un agente en marcha")]
    TaskNotRunning(TaskId),

    #[error("la tarea {0} ya está cerrada")]
    TaskClosed(TaskId),

    #[error("la tarea {0} no cambia archivos (es una pregunta)")]
    TaskWithoutChanges(TaskId),

    #[error("escribe qué quieres que haga")]
    EmptyPrompt,

    #[error("{0} llega a Hedy en la fase 4; por ahora las tareas usan Claude Code")]
    ProviderNotYetSupported(Provider),

    #[error("{}", merge_message(branch, reason))]
    MergeBlocked {
        branch: String,
        reason: MergeOutcome,
    },

    #[error("ajuste desconocido: «{0}»")]
    UnknownSetting(String),
}

pub type AppResult<T> = Result<T, AppError>;

fn merge_message(branch: &str, reason: &MergeOutcome) -> String {
    let why = match reason {
        MergeOutcome::NotOnBase { current } => {
            format!("tu carpeta principal está en la rama «{current}», no en la rama base")
        }
        MergeOutcome::Dirty => "tu carpeta principal tiene cambios sin guardar en git".to_owned(),
        MergeOutcome::Conflict => "hay conflictos con la rama base".to_owned(),
        MergeOutcome::Merged => "ya está unida".to_owned(),
    };
    format!(
        "Los cambios quedaron guardados en la rama «{branch}», pero no se pudieron unir: {why}. \
         Resuélvelo y vuelve a aceptar, o únela tú con git."
    )
}
