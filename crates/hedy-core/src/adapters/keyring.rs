use keyring::{Entry, Error as KeyringError};

use crate::application::ports::{PortError, PortResult, SecretStore};
use crate::domain::AccountId;

/// API keys in the OS keyring (Secret Service / libsecret on Linux).
pub struct KeyringSecretStore {
    service: String,
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self {
            service: "hedy".into(),
        }
    }
}

impl KeyringSecretStore {
    fn entry(&self, account: &AccountId) -> PortResult<Entry> {
        Entry::new(&self.service, &format!("api-key:{account}")).map_err(keyring_error)
    }
}

fn keyring_error(err: KeyringError) -> PortError {
    PortError::new(format!("error del llavero del sistema: {err}"))
}

impl SecretStore for KeyringSecretStore {
    fn set_api_key(&self, account: &AccountId, key: &str) -> PortResult<()> {
        self.entry(account)?
            .set_password(key)
            .map_err(keyring_error)
    }

    fn has_api_key(&self, account: &AccountId) -> PortResult<bool> {
        match self.entry(account)?.get_password() {
            Ok(_) => Ok(true),
            Err(KeyringError::NoEntry) => Ok(false),
            Err(e) => Err(keyring_error(e)),
        }
    }

    fn delete_api_key(&self, account: &AccountId) -> PortResult<()> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        }
    }
}
