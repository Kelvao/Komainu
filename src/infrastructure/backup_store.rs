use crate::domain::backup::{
    decrypt_aes_256_gcm, encrypt_aes_256_gcm, BackupConfig, BackupConfigStorePort, BackupError,
    BackupRecord, BoxFuture,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Default, Clone)]
struct BackupStoreData {
    pub config: BackupConfig,
    pub history: Vec<BackupRecord>,
}

const MAX_BACKUP_HISTORY_ENTRIES: usize = 50;

pub struct FileBackupStore {
    path: PathBuf,
    secret: Option<String>,
    cache: Mutex<Option<BackupStoreData>>,
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(s: &str) -> Result<Vec<u8>, BackupError> {
    if !s.len().is_multiple_of(2) {
        return Err(BackupError::EncryptionError("Invalid hex string length".to_string()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| BackupError::EncryptionError(format!("Invalid hex byte: {e}")))
        })
        .collect()
}

fn encrypt_field(val: &str, secret: &str) -> Result<String, BackupError> {
    if val.is_empty() || secret.is_empty() {
        return Ok(val.to_string());
    }
    if val.starts_with("enc:") {
        return Ok(val.to_string());
    }
    let encrypted = encrypt_aes_256_gcm(val.as_bytes(), secret)?;
    Ok(format!("enc:{}", encode_hex(&encrypted)))
}

fn decrypt_field(val: &str, secret: &str) -> Result<String, BackupError> {
    if val.is_empty() || secret.is_empty() || !val.starts_with("enc:") {
        return Ok(val.to_string());
    }
    let hex_data = &val[4..];
    let ciphertext = decode_hex(hex_data)?;
    let decrypted = decrypt_aes_256_gcm(&ciphertext, secret)?;
    String::from_utf8(decrypted)
        .map_err(|e| BackupError::EncryptionError(format!("Decrypted secret is not valid UTF-8: {e}")))
}

impl FileBackupStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let secret = std::env::var("KOMAINU_STORE_KEY")
            .or_else(|_| std::env::var("SESSION_SECRET"))
            .ok()
            .filter(|s| !s.is_empty());
        Self::with_secret(path, secret)
    }

    pub fn with_secret(path: impl AsRef<Path>, secret: Option<String>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            secret,
            cache: Mutex::new(None),
        }
    }

    fn read_data(&self) -> Result<BackupStoreData, BackupError> {
        if !self.path.exists() {
            return Ok(BackupStoreData::default());
        }
        let content = fs::read_to_string(&self.path)
            .map_err(|e| BackupError::IoError(format!("Failed to read backup store at {:?}: {e}", self.path)))?;
        if content.trim().is_empty() {
            return Ok(BackupStoreData::default());
        }
        let mut data: BackupStoreData = serde_json::from_str(&content)
            .map_err(|e| BackupError::ConfigError(format!("Failed to parse backup store JSON: {e}")))?;

        if let Some(ref sec) = self.secret {
            data.config.encryption_password = decrypt_field(&data.config.encryption_password, sec)?;
            data.config.pcloud_token = decrypt_field(&data.config.pcloud_token, sec)?;
            data.config.pcloud_client_secret = decrypt_field(&data.config.pcloud_client_secret, sec)?;
        }

        if data.history.len() > MAX_BACKUP_HISTORY_ENTRIES {
            let excess = data.history.len() - MAX_BACKUP_HISTORY_ENTRIES;
            data.history.drain(0..excess);
        }

        Ok(data)
    }

    fn write_data(&self, data: &BackupStoreData) -> Result<(), BackupError> {
        let mut store_data = data.clone();
        if let Some(ref sec) = self.secret {
            store_data.config.encryption_password = encrypt_field(&store_data.config.encryption_password, sec)?;
            store_data.config.pcloud_token = encrypt_field(&store_data.config.pcloud_token, sec)?;
            store_data.config.pcloud_client_secret = encrypt_field(&store_data.config.pcloud_client_secret, sec)?;
        }

        let json = serde_json::to_string_pretty(&store_data)
            .map_err(|e| BackupError::ConfigError(format!("Failed to serialize backup store: {e}")))?;
        
        // Write atomically via temporary file
        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, json.as_bytes())
            .map_err(|e| BackupError::IoError(format!("Failed to write tmp backup store: {e}")))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
        }

        fs::rename(&tmp_path, &self.path)
            .map_err(|e| BackupError::IoError(format!("Failed to commit backup store: {e}")))?;

        Ok(())
    }
}

impl BackupConfigStorePort for FileBackupStore {
    fn load_config(&self) -> BoxFuture<'_, Result<BackupConfig, BackupError>> {
        Box::pin(async move {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.is_none() {
                *cache = Some(self.read_data()?);
            }
            let mut cfg = cache.as_ref().unwrap().config.clone();
            if let Ok(pw) = std::env::var("BACKUP_ENCRYPTION_PASSWORD")
                && !pw.is_empty() {
                    cfg.encryption_password = pw;
                }
            if let Ok(tok) = std::env::var("PCLOUD_TOKEN")
                && !tok.is_empty() {
                    cfg.pcloud_token = tok;
                }
            Ok(cfg)
        })
    }

    fn save_config(&self, config: &BackupConfig) -> BoxFuture<'_, Result<(), BackupError>> {
        let config = config.clone();
        Box::pin(async move {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            let mut data = if let Some(ref d) = *cache {
                d.clone()
            } else {
                self.read_data()?
            };
            data.config = config;
            self.write_data(&data)?;
            *cache = Some(data);
            Ok(())
        })
    }

    fn load_history(&self) -> BoxFuture<'_, Result<Vec<BackupRecord>, BackupError>> {
        Box::pin(async move {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.is_none() {
                *cache = Some(self.read_data()?);
            }
            Ok(cache.as_ref().unwrap().history.clone())
        })
    }

    fn add_record(&self, record: BackupRecord) -> BoxFuture<'_, Result<(), BackupError>> {
        Box::pin(async move {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            let mut data = if let Some(ref d) = *cache {
                d.clone()
            } else {
                self.read_data()?
            };
            data.history.push(record);
            if data.history.len() > MAX_BACKUP_HISTORY_ENTRIES {
                let excess = data.history.len() - MAX_BACKUP_HISTORY_ENTRIES;
                data.history.drain(0..excess);
            }
            self.write_data(&data)?;
            *cache = Some(data);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::backup::{BackupStatus, PCloudRegion};

    #[tokio::test]
    async fn test_file_backup_store_roundtrip() {
        let dir = tempfile_dir("test_file_backup_store");
        let path = dir.join("backup_store.json");
        let store = FileBackupStore::with_secret(&path, Some("local_master_secret".to_string()));

        // Initial load should give default
        let initial_cfg = store.load_config().await.unwrap();
        assert!(!initial_cfg.enabled);
        assert_eq!(initial_cfg.cron_expression, "0 3 * * *");

        // Save new config
        let new_cfg = BackupConfig {
            enabled: true,
            cron_expression: "0 4 * * *".to_string(),
            encryption_password: "super-password".to_string(),
            pcloud_region: PCloudRegion::US,
            pcloud_token: "tok123".to_string(),
            pcloud_folder: "9876".to_string(),
            pcloud_client_id: "client123".to_string(),
            pcloud_client_secret: "secret123".to_string(),
        };
        store.save_config(&new_cfg).await.unwrap();

        // Check raw content on disk: sensitive fields must be encrypted
        let raw_disk = fs::read_to_string(&path).unwrap();
        assert!(!raw_disk.contains("super-password"), "Plaintext password leaked on disk!");
        assert!(!raw_disk.contains("tok123"), "Plaintext pCloud token leaked on disk!");
        assert!(!raw_disk.contains("secret123"), "Plaintext pCloud client secret leaked on disk!");
        assert!(raw_disk.contains("enc:"), "Encrypted prefix expected on disk");

        // Loading via store decrypts seamlessly
        let loaded = store.load_config().await.unwrap();
        assert_eq!(loaded, new_cfg);

        // Add history record
        let record = BackupRecord {
            id: "rec-1".to_string(),
            timestamp: 1700000000,
            filename: "vaultwarden_backup_test.zip.enc".to_string(),
            size_bytes: 1048576,
            duration_secs: 5,
            status: BackupStatus::Success,
        };
        store.add_record(record.clone()).await.unwrap();

        let history = store.load_history().await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0], record);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_backup_store_caps_history() {
        let dir = tempfile_dir("test_file_backup_store_caps");
        let path = dir.join("backup_store.json");
        let store = FileBackupStore::with_secret(&path, Some("secret".to_string()));

        for i in 0..60 {
            let record = BackupRecord {
                id: format!("rec-{i}"),
                timestamp: 1700000000 + i as u64,
                filename: format!("backup_{i}.zip.enc"),
                size_bytes: 1000,
                duration_secs: 1,
                status: BackupStatus::Success,
            };
            store.add_record(record).await.unwrap();
        }

        let history = store.load_history().await.unwrap();
        assert_eq!(history.len(), MAX_BACKUP_HISTORY_ENTRIES);
        assert_eq!(history.first().unwrap().id, "rec-10");
        assert_eq!(history.last().unwrap().id, "rec-59");

        let _ = fs::remove_dir_all(&dir);
    }

    fn tempfile_dir(prefix: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("komainu_{}_{}_{}", prefix, std::process::id(), uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&tmp);
        tmp
    }
}
