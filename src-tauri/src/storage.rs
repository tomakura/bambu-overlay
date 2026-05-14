use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use keyring::{Entry, Error as KeyringError};

use crate::types::{ProfileSecret, StoredStore};

const SERVICE: &str = "dev.flow.bambu-overlay";
const STORE_FILE: &str = "profiles.json";

#[derive(Clone)]
pub struct CredentialStore;

impl CredentialStore {
    pub fn new() -> Self {
        Self
    }

    pub fn save_profile_secret(
        &self,
        profile_id: &str,
        secret: &ProfileSecret,
    ) -> anyhow::Result<()> {
        let entry = Entry::new(SERVICE, &credential_key(profile_id))?;
        let encoded = serde_json::to_string(secret)?;
        entry.set_password(&encoded)?;
        let saved = entry.get_password()?;
        if saved != encoded {
            return Err(anyhow::anyhow!(
                "Secure storage verification failed after saving credentials."
            ));
        }
        Ok(())
    }

    pub fn load_profile_secret(&self, profile_id: &str) -> anyhow::Result<ProfileSecret> {
        let entry = Entry::new(SERVICE, &credential_key(profile_id))?;
        let value = entry.get_password()?;
        Ok(serde_json::from_str(&value)?)
    }

    pub fn delete_profile_secret(&self, profile_id: &str) -> anyhow::Result<()> {
        let entry = Entry::new(SERVICE, &credential_key(profile_id))?;
        match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

pub fn is_missing_credential(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<KeyringError>()
        .is_some_and(|error| matches!(error, KeyringError::NoEntry))
        || error
            .to_string()
            .contains("No matching entry found in secure storage")
}

pub fn load_store() -> anyhow::Result<StoredStore> {
    let path = store_path()?;
    if !path.exists() {
        return Ok(StoredStore::default());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn save_store(store: &StoredStore) -> anyhow::Result<()> {
    let path = store_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(store)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn store_path() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from("dev", "Flow", "BambuOverlay")
        .ok_or_else(|| anyhow::anyhow!("Unable to resolve the app config directory."))?;
    Ok(dirs.config_dir().join(STORE_FILE))
}

fn credential_key(profile_id: &str) -> String {
    format!("profile:{profile_id}")
}
