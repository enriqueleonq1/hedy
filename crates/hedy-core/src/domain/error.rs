use std::path::PathBuf;

use super::{AccountId, Provider};

/// Violations of domain rules. Messages are user-facing (Spanish).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("proveedor desconocido: {0} (usa claude, codex, gemini o kimi)")]
    UnknownProvider(String),

    #[error("identificador de cuenta no válido: «{0}» (solo minúsculas, números y guiones)")]
    InvalidAccountId(String),

    #[error("el nombre de la cuenta no puede estar vacío")]
    EmptyAccountName,

    #[error("color no válido: «{0}» (usa un nombre de la paleta o #RRGGBB)")]
    InvalidColor(String),

    #[error("{provider} no admite cuentas de suscripción")]
    SubscriptionNotSupported { provider: Provider },

    #[error("{provider} no admite cuentas por API key")]
    ApiKeyNotSupported { provider: Provider },

    #[error("las cuentas por API key no tienen CLI que lanzar; se usan desde el chat de Hedy")]
    NothingToLaunch,

    #[error("la cuenta «{account}» está reservada para el proyecto protegido {}", project.display())]
    AccountReserved {
        account: AccountId,
        project: PathBuf,
    },

    #[error("el proyecto {} está protegido y solo puede usar la cuenta «{assigned}»", project.display())]
    ProtectedProject {
        project: PathBuf,
        assigned: AccountId,
    },

    #[error("para proteger un proyecto primero asígnale una cuenta con `hedy use <cuenta>`")]
    ProtectWithoutAccount,

    #[error(
        "no se puede proteger: la cuenta «{account}» también la usan {}",
        others.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    )]
    AccountShared {
        account: AccountId,
        others: Vec<PathBuf>,
    },
}
