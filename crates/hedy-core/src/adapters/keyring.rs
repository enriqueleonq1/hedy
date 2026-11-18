use std::path::Path;

use keyring::{Entry, Error as KeyringError};
use sha2::{Digest, Sha256};

use crate::application::ports::{PortError, PortResult, SecretStore};
use crate::domain::AccountId;

/// API keys and this PC's sync keys in the OS keyring (Secret Service /
/// libsecret on Linux).
pub struct KeyringSecretStore {
    service: String,
    /// The sync keys' entry, one per data folder: a Hedy tried with
    /// `HEDY_DATA_DIR` must never overwrite the real one.
    sync_entry: String,
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self {
            service: "hedy".into(),
            sync_entry: "sync-device".into(),
        }
    }
}

impl KeyringSecretStore {
    /// The store for the Hedy whose data lives in `data_dir`.
    pub fn for_data_dir(data_dir: &Path) -> Self {
        // The same folder however it is written (`/x`, `/x/`, a link to it).
        let data_dir = data_dir
            .canonicalize()
            .unwrap_or_else(|_| data_dir.to_owned());
        let hash = Sha256::digest(data_dir.to_string_lossy().as_bytes());
        let short: String = hash[..6].iter().map(|b| format!("{b:02x}")).collect();
        Self {
            sync_entry: format!("sync-device:{short}"),
            ..Self::default()
        }
    }

    fn entry(&self, account: &AccountId) -> PortResult<Entry> {
        Entry::new(&self.service, &format!("api-key:{account}")).map_err(keyring_error)
    }

    /// Personal sync keeps the entry it always had; each team has its own
    /// next to it.
    fn sync(&self, space: &str) -> PortResult<Entry> {
        let name = match space {
            "" => self.sync_entry.clone(),
            space => format!("{}:{space}", self.sync_entry),
        };
        Entry::new(&self.service, &name).map_err(keyring_error)
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

    fn api_key(&self, account: &AccountId) -> PortResult<Option<String>> {
        match self.entry(account)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
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

    fn sync_keys(&self, space: &str) -> PortResult<Option<String>> {
        match self.sync(space)?.get_password() {
            Ok(keys) => Ok(Some(keys)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }

    fn set_sync_keys(&self, space: &str, keys: &str) -> PortResult<()> {
        self.sync(space)?.set_password(keys).map_err(keyring_error)
    }

    fn delete_sync_keys(&self, space: &str) -> PortResult<()> {
        match self.sync(space)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_data_folder_has_its_own_sync_entry() {
        let real = KeyringSecretStore::for_data_dir(Path::new("/home/a/.local/share/hedy"));
        let trial = KeyringSecretStore::for_data_dir(Path::new("/tmp/hedy-test"));
        assert_ne!(real.sync_entry, trial.sync_entry);
        assert!(real.sync_entry.starts_with("sync-device:") && real.sync_entry.len() == 24);
        assert_eq!(real.service, "hedy");
    }
}
