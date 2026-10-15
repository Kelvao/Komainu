use crate::domain::backup::{BackupConfig, BackupConfigStorePort, BackupError, BackupRecord, BoxFuture};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Default)]
struct BackupStoreData {
    pub config: BackupConfig,
    pub history: Vec<BackupRecord>,
}

pub struct FileBackupStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileBackupStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            lock: Mutex::new(()),
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
        serde_json::from_str(&content)
            .map_err(|e| BackupError::ConfigError(format!("Failed to parse backup store JSON: {e}")))
    }

    fn write_data(&self, data: &BackupStoreData) -> Result<(), BackupError> {
        let json = serde_json::to_string_pretty(data)
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
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            let data = self.read_data()?;
            Ok(data.config)
        })
    }

    fn save_config(&self, config: &BackupConfig) -> BoxFuture<'_, Result<(), BackupError>> {
        let config = config.clone();
        Box::pin(async move {
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut data = self.read_data()?;
            data.config = config;
            self.write_data(&data)
        })
    }

    fn load_history(&self) -> BoxFuture<'_, Result<Vec<BackupRecord>, BackupError>> {
        Box::pin(async move {
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            let data = self.read_data()?;
            Ok(data.history)
        })
    }

    fn add_record(&self, record: BackupRecord) -> BoxFuture<'_, Result<(), BackupError>> {
        Box::pin(async move {
            let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut data = self.read_data()?;
            data.history.insert(0, record);
            // Retain last 50 backup records
            if data.history.len() > 50 {
                data.history.truncate(50);
            }
            self.write_data(&data)
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
        let store = FileBackupStore::new(&path);

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

    fn tempfile_dir(prefix: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("komainu_{}_{}", prefix, std::process::id()));
        let _ = fs::create_dir_all(&tmp);
        tmp
    }
}
