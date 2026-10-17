use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;
use chrono::Local;

use crate::domain::backup::{
    decrypt_file_aes_256_gcm, encrypt_file_aes_256_gcm, BackupConfig, BackupConfigStorePort,
    BackupError, BackupRecord, BackupStatus, PCloudClientPort, PCloudRemoteFile, TempFileGuard,
};
use crate::domain::container::ContainerPort;

use super::archiver::{build_zip_archive, komainu_temp_dir};
use super::scheduler;

struct BackupRunningGuard(Arc<AtomicBool>);

impl Drop for BackupRunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

type RemoteFilesCache = Arc<Mutex<Option<(Instant, Vec<PCloudRemoteFile>)>>>;

pub struct BackupService {
    pub(crate) container_port: Arc<dyn ContainerPort>,
    pub(crate) config_store: Arc<dyn BackupConfigStorePort>,
    pub(crate) pcloud_client: Arc<dyn PCloudClientPort>,
    pub(crate) vw_container_name: String,
    pub(crate) vw_data_path: PathBuf,
    pub(crate) is_running: Arc<AtomicBool>,
    pub(crate) last_run_minute: Arc<Mutex<Option<i64>>>,
    pub(crate) remote_files_cache: RemoteFilesCache,
}

impl BackupService {
    pub fn new(
        container_port: Arc<dyn ContainerPort>,
        config_store: Arc<dyn BackupConfigStorePort>,
        pcloud_client: Arc<dyn PCloudClientPort>,
        vw_container_name: String,
        vw_data_path: impl AsRef<Path>,
    ) -> Self {
        Self {
            container_port,
            config_store,
            pcloud_client,
            vw_container_name,
            vw_data_path: vw_data_path.as_ref().to_path_buf(),
            is_running: Arc::new(AtomicBool::new(false)),
            last_run_minute: Arc::new(Mutex::new(None)),
            remote_files_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn get_config(&self) -> Result<BackupConfig, BackupError> {
        self.config_store.load_config().await
    }

    pub async fn save_config(&self, config: &BackupConfig) -> Result<(), BackupError> {
        // Validate cron expression (supports 5-field standard cron or 6/7-field cron)
        if config.enabled {
            let expr = config.cron_expression.trim();
            if cron::Schedule::from_str(expr).is_err() {
                let normalized = format!("0 {expr}");
                cron::Schedule::from_str(&normalized)
                    .map_err(|e| BackupError::ConfigError(format!("Invalid cron expression '{expr}': {e}")))?;
            }
        }
        self.invalidate_remote_cache().await;
        self.config_store.save_config(config).await
    }

    pub async fn get_history(&self) -> Result<Vec<BackupRecord>, BackupError> {
        self.config_store.load_history().await
    }

    pub async fn invalidate_remote_cache(&self) {
        let mut cache = self.remote_files_cache.lock().await;
        *cache = None;
    }

    pub async fn list_remote_backups(&self) -> Result<Vec<PCloudRemoteFile>, BackupError> {
        {
            let cache = self.remote_files_cache.lock().await;
            if let Some((ts, ref files)) = *cache
                && ts.elapsed() < std::time::Duration::from_secs(45)
            {
                return Ok(files.clone());
            }
        }

        let config = self.config_store.load_config().await?;
        if config.pcloud_token.trim().is_empty() {
            return Ok(Vec::new());
        }
        let files = self
            .pcloud_client
            .list_folder(&config.pcloud_region, &config.pcloud_token, &config.pcloud_folder)
            .await?;

        let mut cache = self.remote_files_cache.lock().await;
        *cache = Some((Instant::now(), files.clone()));
        Ok(files)
    }

    pub async fn download_and_decrypt_backup(
        &self,
        fileid: u64,
        dest_decrypted_path: &Path,
    ) -> Result<u64, BackupError> {
        let config = self.config_store.load_config().await?;
        if config.pcloud_token.trim().is_empty() {
            return Err(BackupError::PCloudUploadFailed(
                "pCloud access token is not configured".to_string(),
            ));
        }
        if config.encryption_password.is_empty() {
            return Err(BackupError::EncryptionError(
                "Encryption password is not configured".to_string(),
            ));
        }

        let temp_enc_path = komainu_temp_dir().join(format!("vw_dl_enc_{}_{}.tmp", fileid, Uuid::new_v4()));
        let _enc_guard = TempFileGuard(temp_enc_path.clone());

        self.pcloud_client
            .download_file(&config.pcloud_region, &config.pcloud_token, fileid, &temp_enc_path)
            .await?;

        let temp_enc_clone = temp_enc_path.clone();
        let dest_decrypted_clone = dest_decrypted_path.to_path_buf();
        let dec_pass = config.encryption_password.clone();
        let decrypted_bytes = tokio::task::spawn_blocking(move || {
            decrypt_file_aes_256_gcm(
                &temp_enc_clone,
                &dest_decrypted_clone,
                &dec_pass,
            )
        })
        .await
        .map_err(|e| BackupError::EncryptionError(format!("Decryption task failed: {e}")))??;

        Ok(decrypted_bytes)
    }

    pub fn start_backup_task(self: &Arc<Self>) -> Result<(), BackupError> {
        if self
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackupError::ConfigError(
                "A backup operation is already running".to_string(),
            ));
        }
        let this = self.clone();
        tokio::spawn(async move {
            let _guard = BackupRunningGuard(this.is_running.clone());
            if let Err(e) = this.run_backup_internal().await {
                error!("Background manual backup failed: {e}");
            }
        });
        Ok(())
    }

    pub async fn is_in_progress(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    pub fn data_path_exists(&self) -> bool {
        self.vw_data_path.exists()
    }

    pub fn env_path_exists(&self) -> bool {
        self.vw_data_path.join("vaultwarden.env").is_file()
            || self.vw_data_path.join(".env").is_file()
    }

    pub async fn execute_backup(&self) -> Result<BackupRecord, BackupError> {
        if self
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackupError::ConfigError(
                "A backup operation is already running".to_string(),
            ));
        }
        let _guard = BackupRunningGuard(self.is_running.clone());

        self.run_backup_internal().await
    }

    async fn run_backup_internal(&self) -> Result<BackupRecord, BackupError> {
        let start_time = Instant::now();
        let timestamp_now = Local::now();
        let record_id = Uuid::new_v4().to_string();
        let filename = format!(
            "vaultwarden_backup_{}.zip.enc",
            timestamp_now.format("%Y%m%d_%H%M%S")
        );

        let config = self.config_store.load_config().await?;
        if config.encryption_password.is_empty() {
            let err = BackupError::EncryptionError(
                "Password for backup encryption is not set in configuration".to_string(),
            );
            self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &err.to_string(), start_time.elapsed().as_secs()).await;
            return Err(err);
        }

        if config.pcloud_token.is_empty() {
            let err = BackupError::PCloudUploadFailed(
                "pCloud access token is not set in configuration".to_string(),
            );
            self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &err.to_string(), start_time.elapsed().as_secs()).await;
            return Err(err);
        }

        info!("Starting Vaultwarden backup routine...");

        // 1. Executar comando oficial de backup no próprio Vaultwarden via Docker Exec
        info!("Executing '/vaultwarden backup' in container '{}'...", self.vw_container_name);
        let exec_cmd = vec!["/vaultwarden".to_string(), "backup".to_string()];
        if let Err(e) = self.container_port.exec_command(&self.vw_container_name, exec_cmd).await {
            let err = BackupError::ContainerExecFailed(e.to_string());
            self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &err.to_string(), start_time.elapsed().as_secs()).await;
            return Err(err);
        }

        // 2. Localizar o arquivo de banco gerado pelo backup em vw_data_path
        if !self.vw_data_path.exists() {
            let err = BackupError::DataPathNotFound(self.vw_data_path.to_string_lossy().to_string());
            self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &err.to_string(), start_time.elapsed().as_secs()).await;
            return Err(err);
        }

        let (sqlite_file_path, is_temp_snapshot) = match self.find_newest_db_backup() {
            Ok(p) => (p, true),
            Err(_) => {
                // Fallback para db.sqlite3 existente se /vaultwarden backup gerou direto ou gravou com outro nome
                let direct_db = self.vw_data_path.join("db.sqlite3");
                if direct_db.exists() {
                    warn!("No db_*.sqlite3 snapshot found; falling back to direct db.sqlite3");
                    (direct_db, false)
                } else {
                    let err = BackupError::DatabaseBackupNotFound(format!(
                        "No database backup found in {}",
                        self.vw_data_path.display()
                    ));
                    self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &err.to_string(), start_time.elapsed().as_secs()).await;
                    return Err(err);
                }
            }
        };
        let _temp_snapshot_guard = if is_temp_snapshot {
            Some(TempFileGuard(sqlite_file_path.clone()))
        } else {
            None
        };

        // 3. Compactar no arquivo ZIP gravando em arquivo temporário no disco
        let temp_zip_path = komainu_temp_dir().join(format!("vw_backup_{}.zip", record_id));
        let _temp_zip_guard = TempFileGuard(temp_zip_path.clone());

        info!("Packaging assets into temporary archive on disk...");
        let vw_data_path_clone = self.vw_data_path.clone();
        let sqlite_file_path_clone = sqlite_file_path.clone();
        let temp_zip_path_clone = temp_zip_path.clone();
        let zip_res = tokio::task::spawn_blocking(move || {
            build_zip_archive(&vw_data_path_clone, &sqlite_file_path_clone, &temp_zip_path_clone, is_temp_snapshot)
        })
        .await
        .map_err(|e| BackupError::IoError(format!("Zip packaging task failed: {e}")))?;

        if let Err(e) = zip_res {
            self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &e.to_string(), start_time.elapsed().as_secs()).await;
            return Err(e);
        }

        // 4. Criptografar com AES-256-GCM via streaming em arquivo temporário (64KB de RAM)
        let temp_enc_path = komainu_temp_dir().join(format!("vw_backup_{}.enc", record_id));
        let _temp_enc_guard = TempFileGuard(temp_enc_path.clone());

        info!("Encrypting backup archive with AES-256-GCM...");
        let temp_zip_path_clone2 = temp_zip_path.clone();
        let temp_enc_path_clone = temp_enc_path.clone();
        let enc_password = config.encryption_password.clone();
        let enc_res = tokio::task::spawn_blocking(move || {
            encrypt_file_aes_256_gcm(&temp_zip_path_clone2, &temp_enc_path_clone, &enc_password)
        })
        .await
        .map_err(|e| BackupError::EncryptionError(format!("Encryption task failed: {e}")))?;

        if let Err(e) = enc_res {
            self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &e.to_string(), start_time.elapsed().as_secs()).await;
            return Err(e);
        }

        // Deleta o arquivo zip temporário imediatamente para liberar espaço no disco
        drop(_temp_zip_guard);

        let file_size = match std::fs::metadata(&temp_enc_path) {
            Ok(meta) => meta.len(),
            Err(e) => {
                let err = BackupError::IoError(format!("Failed to read encrypted backup metadata: {e}"));
                self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &err.to_string(), start_time.elapsed().as_secs()).await;
                return Err(err);
            }
        };

        // 5. Enviar para a API do pCloud via streaming direto do disco
        info!("Uploading encrypted archive ({} bytes) to pCloud...", file_size);
        if let Err(e) = self
            .pcloud_client
            .upload(
                &config.pcloud_region,
                &config.pcloud_token,
                &config.pcloud_folder,
                &filename,
                &temp_enc_path,
            )
            .await
        {
            self.record_failure(&record_id, timestamp_now.timestamp() as u64, &filename, &e.to_string(), start_time.elapsed().as_secs()).await;
            return Err(e);
        }
        drop(_temp_enc_guard);

        let duration_secs = start_time.elapsed().as_secs();
        let record = BackupRecord {
            id: record_id,
            timestamp: timestamp_now.timestamp() as u64,
            filename: filename.clone(),
            size_bytes: file_size,
            duration_secs,
            status: BackupStatus::Success,
        };

        let _ = self.config_store.add_record(record.clone()).await;
        self.invalidate_remote_cache().await;
        info!("Backup completed and uploaded successfully: {}", filename);
        Ok(record)
    }

    async fn record_failure(&self, id: &str, timestamp: u64, filename: &str, error_msg: &str, duration_secs: u64) {
        error!("Backup failed: {}", error_msg);
        let record = BackupRecord {
            id: id.to_string(),
            timestamp,
            filename: filename.to_string(),
            size_bytes: 0,
            duration_secs,
            status: BackupStatus::Failed(error_msg.to_string()),
        };
        let _ = self.config_store.add_record(record).await;
    }

    fn find_newest_db_backup(&self) -> Result<PathBuf, BackupError> {
        let entries = fs::read_dir(&self.vw_data_path)
            .map_err(|e| BackupError::IoError(format!("Failed to read data dir: {e}")))?;

        let mut newest_file: Option<(PathBuf, std::time::SystemTime)> = None;

        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type()
                && file_type.is_symlink()
            {
                continue;
            }
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
                && file_name.starts_with("db_") && file_name.ends_with(".sqlite3")
            {
                let metadata = entry.metadata().ok();
                let mtime = metadata
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                match &newest_file {
                    Some((_, prev_mtime)) if mtime > *prev_mtime => {
                        newest_file = Some((path, mtime));
                    }
                    None => {
                        newest_file = Some((path, mtime));
                    }
                    _ => {}
                }
            }
        }

        newest_file
            .map(|(p, _)| p)
            .ok_or_else(|| BackupError::DatabaseBackupNotFound("No db_*.sqlite3 snapshot found".to_string()))
    }

    pub fn start_cron_loop(self: Arc<Self>) {
        scheduler::start_cron_loop(self);
    }
}

