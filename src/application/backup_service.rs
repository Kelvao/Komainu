use crate::domain::backup::{
    decrypt_file_aes_256_gcm, encrypt_file_aes_256_gcm, BackupConfig, BackupConfigStorePort,
    BackupError, BackupRecord, BackupStatus, PCloudClientPort, PCloudRemoteFile, TempFileGuard,
};
use crate::domain::container::ContainerPort;
use chrono::{Local, Timelike};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

struct BackupRunningGuard(Arc<AtomicBool>);

impl Drop for BackupRunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn is_already_compressed(path: &Path) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => return false,
    };
    matches!(
        ext.as_str(),
        "zip" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar"
            | "png" | "jpg" | "jpeg" | "webp" | "gif" | "ico"
            | "mp4" | "mkv" | "webm" | "avi" | "mov" | "mp3" | "ogg" | "flac" | "aac"
            | "pdf" | "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp"
            | "apk" | "jar" | "deb" | "rpm" | "iso"
    )
}

pub fn komainu_temp_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KOMAINU_TEMP_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if fs::create_dir_all(&p).is_ok() {
                return p;
            }
        }
    }

    #[cfg(unix)]
    {
        let var_tmp = PathBuf::from("/var/tmp/komainu");
        if fs::create_dir_all(&var_tmp).is_ok() {
            return var_tmp;
        }
    }

    std::env::temp_dir()
}

type RemoteFilesCache = Arc<Mutex<Option<(Instant, Vec<PCloudRemoteFile>)>>>;

pub struct BackupService {
    container_port: Arc<dyn ContainerPort>,
    config_store: Arc<dyn BackupConfigStorePort>,
    pcloud_client: Arc<dyn PCloudClientPort>,
    vw_container_name: String,
    vw_data_path: PathBuf,
    is_running: Arc<AtomicBool>,
    last_run_minute: Arc<Mutex<Option<i64>>>,
    remote_files_cache: RemoteFilesCache,
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
                && ts.elapsed() < std::time::Duration::from_secs(45) {
                    return Ok(files.clone());
                }
        }

        let config = self.config_store.load_config().await?;
        if config.pcloud_token.trim().is_empty() {
            return Ok(Vec::new());
        }
        let files = self.pcloud_client
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

        // 3. Compactar no arquivo ZIP gravando em arquivo temporário no disco
        let temp_zip_path = komainu_temp_dir().join(format!("vw_backup_{}.zip", record_id));
        let _temp_zip_guard = TempFileGuard(temp_zip_path.clone());

        info!("Packaging assets into temporary archive on disk...");
        let vw_data_path_clone = self.vw_data_path.clone();
        let sqlite_file_path_clone = sqlite_file_path.clone();
        let temp_zip_path_clone = temp_zip_path.clone();
        let zip_res = tokio::task::spawn_blocking(move || {
            Self::build_zip_archive(&vw_data_path_clone, &sqlite_file_path_clone, &temp_zip_path_clone, is_temp_snapshot)
        })
        .await
        .map_err(|e| BackupError::IoError(format!("Zip packaging task failed: {e}")))?;

        if let Err(e) = zip_res {
            if is_temp_snapshot {
                let _ = fs::remove_file(&sqlite_file_path);
            }
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
                && file_type.is_symlink() {
                    continue;
                }
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
                && file_name.starts_with("db_") && file_name.ends_with(".sqlite3") {
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

    fn build_zip_archive(vw_data_path: &Path, sqlite_path: &Path, out_path: &Path, is_temp_snapshot: bool) -> Result<(), BackupError> {
        let file = fs::File::create(out_path)
            .map_err(|e| BackupError::IoError(format!("Failed to create temporary backup zip: {e}")))?;
        let writer = std::io::BufWriter::with_capacity(128 * 1024, file);
        let mut zip = ZipWriter::new(writer);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        // 1. db.sqlite3
        if sqlite_path.exists() {
            if let Ok(meta) = fs::symlink_metadata(sqlite_path)
                && meta.file_type().is_symlink() {
                    return Err(BackupError::IoError("Sqlite database snapshot cannot be a symlink".to_string()));
                }
            let db_file = fs::File::open(sqlite_path)
                .map_err(|e| BackupError::IoError(format!("Failed to open database file: {e}")))?;
            let mut db_reader = std::io::BufReader::with_capacity(64 * 1024, db_file);
            zip.start_file("db.sqlite3", options)
                .map_err(|e| BackupError::ZipError(e.to_string()))?;
            std::io::copy(&mut db_reader, &mut zip)
                .map_err(|e| BackupError::IoError(format!("Failed to write database to zip: {e}")))?;
            drop(db_reader);
            if is_temp_snapshot {
                let _ = fs::remove_file(sqlite_path);
            }
        }

        // 2-4. attachments/, sends/, templates/
        for dir_name in &["attachments", "sends", "templates"] {
            let sub_dir = vw_data_path.join(dir_name);
            if let Ok(meta) = fs::symlink_metadata(&sub_dir) {
                if meta.file_type().is_symlink() {
                    tracing::warn!("Ignoring symlink root directory in backup: {:?}", sub_dir);
                    continue;
                }
                if meta.is_dir() {
                    Self::add_directory_to_zip(&mut zip, &sub_dir, dir_name, options)?;
                }
            }
        }

        // 5. rsa_key files
        if let Ok(entries) = fs::read_dir(vw_data_path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_symlink() {
                        continue;
                    }
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str())
                        && name.starts_with("rsa_key") && file_type.is_file()
                            && let Ok(mut key_file) = fs::File::open(&path)
                            && zip.start_file(name, options).is_ok() {
                                let _ = std::io::copy(&mut key_file, &mut zip);
                            }
                }
            }
        }

        // 6. vaultwarden.env or .env inside vw_data_path
        for env_name in &["vaultwarden.env", ".env"] {
            let env_candidate = vw_data_path.join(env_name);
            if let Ok(meta) = fs::symlink_metadata(&env_candidate) {
                if meta.file_type().is_symlink() {
                    tracing::warn!("Ignoring symlink env file in backup: {:?}", env_candidate);
                    continue;
                }
                if meta.is_file()
                    && let Ok(mut env_file) = fs::File::open(&env_candidate)
                    && zip.start_file(*env_name, options).is_ok() {
                        let _ = std::io::copy(&mut env_file, &mut zip);
                        break;
                    }
            }
        }

        let mut writer = zip.finish()
            .map_err(|e| BackupError::ZipError(e.to_string()))?;
        writer.flush()
            .map_err(|e| BackupError::IoError(format!("Failed to flush backup zip buffer: {e}")))?;
        Ok(())
    }

    fn add_directory_to_zip<W: Write + std::io::Seek>(
        zip: &mut ZipWriter<W>,
        src_dir: &Path,
        zip_prefix: &str,
        options: SimpleFileOptions,
    ) -> Result<(), BackupError> {
        if !src_dir.exists() {
            return Ok(());
        }

        let entries = match fs::read_dir(src_dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if file_type.is_symlink() {
                tracing::warn!("Ignoring symlink in backup source directory: {:?}", entry.path());
                continue;
            }

            let path = entry.path();
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            let zip_path = format!("{}/{}", zip_prefix, file_name);

            if file_type.is_dir() {
                Self::add_directory_to_zip(zip, &path, &zip_path, options)?;
            } else if file_type.is_file() {
                let file_options = if is_already_compressed(&path) {
                    options.compression_method(zip::CompressionMethod::Stored)
                } else {
                    options
                };
                if let Ok(file) = fs::File::open(&path)
                    && zip.start_file(zip_path, file_options).is_ok() {
                        let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
                        let _ = std::io::copy(&mut reader, zip);
                    }
            }
        }

        Ok(())
    }

    /// Background cron monitoring task
    pub fn start_cron_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            info!("Backup cron background worker started.");
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

                let config = match self.config_store.load_config().await {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Could not read backup configuration in cron loop: {e}");
                        continue;
                    }
                };

                if !config.enabled {
                    continue;
                }

                // If schedule format is valid 5 or 6/7 field expression
                let schedule = match cron::Schedule::from_str(&config.cron_expression) {
                    Ok(s) => s,
                    Err(_) => {
                        // In case expression has 5 fields, try converting to 6-field cron by prepending "0 "
                        let normalized = format!("0 {}", config.cron_expression.trim());
                        match cron::Schedule::from_str(&normalized) {
                            Ok(s) => s,
                            Err(e) => {
                                warn!("Invalid cron expression '{}': {e}", config.cron_expression);
                                continue;
                            }
                        }
                    }
                };

                let now_local = Local::now();
                let current_minute = now_local.timestamp() / 60;

                // Check if current minute should trigger using host local time
                let start_of_this_min = now_local
                    .date_naive()
                    .and_hms_opt(now_local.hour(), now_local.minute(), 0)
                    .and_then(|naive| naive.and_local_timezone(Local).single())
                    .unwrap_or(now_local);
                let mut should_run = false;

                let window_start = start_of_this_min
                    .checked_sub_signed(chrono::Duration::seconds(1))
                    .unwrap_or(start_of_this_min);

                // Look up scheduled events in this minute window
                if let Some(next) = schedule.after(&window_start).next() {
                    let next_minute = next.timestamp() / 60;
                    if next_minute == current_minute {
                        let mut last_min = self.last_run_minute.lock().await;
                        if *last_min != Some(current_minute) {
                            *last_min = Some(current_minute);
                            should_run = true;
                        }
                    }
                }

                if should_run {
                    info!("Triggering scheduled cron backup (minute={})...", current_minute);
                    let svc = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = svc.execute_backup().await {
                            error!("Scheduled cron backup failed: {e}");
                        }
                    });
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::backup::{decrypt_aes_256_gcm, BoxFuture, PCloudRegion};
    use crate::domain::container::{ContainerError, ContainerState, ContainerStatus};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockContainer;
    impl ContainerPort for MockContainer {
        fn get_status(&self, name: &str) -> BoxFuture<'_, Result<ContainerStatus, ContainerError>> {
            let name = name.to_string();
            Box::pin(async move {
                Ok(ContainerStatus {
                    name,
                    state: ContainerState::Running,
                    image: "vaultwarden:test".to_string(),
                    uptime: Some("Up 1h".to_string()),
                    cpu_usage_pct: None,
                    memory_used_mb: None,
                    memory_limit_mb: None,
                })
            })
        }
        fn start(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> { Box::pin(async { Ok(()) }) }
        fn stop(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> { Box::pin(async { Ok(()) }) }
        fn restart(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> { Box::pin(async { Ok(()) }) }
        fn update_image(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> { Box::pin(async { Ok(()) }) }
        fn rebuild(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> { Box::pin(async { Ok(()) }) }
        fn exec_command(&self, _: &str, _: Vec<String>) -> BoxFuture<'_, Result<String, ContainerError>> {
            Box::pin(async { Ok("Database backup completed".to_string()) })
        }
    }

    struct MockConfigStore {
        cfg: std::sync::Mutex<BackupConfig>,
        records: std::sync::Mutex<Vec<BackupRecord>>,
    }

    impl BackupConfigStorePort for MockConfigStore {
        fn load_config(&self) -> BoxFuture<'_, Result<BackupConfig, BackupError>> {
            Box::pin(async move { Ok(self.cfg.lock().unwrap().clone()) })
        }
        fn save_config(&self, config: &BackupConfig) -> BoxFuture<'_, Result<(), BackupError>> {
            let config = config.clone();
            Box::pin(async move {
                *self.cfg.lock().unwrap() = config;
                Ok(())
            })
        }
        fn load_history(&self) -> BoxFuture<'_, Result<Vec<BackupRecord>, BackupError>> {
            Box::pin(async move { Ok(self.records.lock().unwrap().clone()) })
        }
        fn add_record(&self, record: BackupRecord) -> BoxFuture<'_, Result<(), BackupError>> {
            Box::pin(async move {
                self.records.lock().unwrap().insert(0, record);
                Ok(())
            })
        }
    }

    struct MockPCloudClient {
        uploads: AtomicUsize,
        last_uploaded_data: std::sync::Mutex<Vec<u8>>,
    }

    impl PCloudClientPort for MockPCloudClient {
        fn upload(
            &self,
            _region: &PCloudRegion,
            _token: &str,
            _folder: &str,
            _filename: &str,
            file_path: &std::path::Path,
        ) -> BoxFuture<'_, Result<(), BackupError>> {
            self.uploads.fetch_add(1, Ordering::SeqCst);
            let content = std::fs::read(file_path).unwrap_or_default();
            *self.last_uploaded_data.lock().unwrap() = content;
            Box::pin(async { Ok(()) })
        }

        fn list_folder(
            &self,
            _region: &PCloudRegion,
            _token: &str,
            _folder: &str,
        ) -> BoxFuture<'_, Result<Vec<PCloudRemoteFile>, BackupError>> {
            Box::pin(async {
                Ok(vec![PCloudRemoteFile {
                    fileid: 99999,
                    name: "vaultwarden_backup_20261014_220000.zip.enc".to_string(),
                    size_bytes: 1024,
                    modified: "Sat, 14 Oct 2026 22:00:00 +0000".to_string(),
                }])
            })
        }

        fn download_file(
            &self,
            _region: &PCloudRegion,
            _token: &str,
            _fileid: u64,
            dest_path: &std::path::Path,
        ) -> BoxFuture<'_, Result<(), BackupError>> {
            let data = self.last_uploaded_data.lock().unwrap().clone();
            let dest_path = dest_path.to_path_buf();
            Box::pin(async move {
                fs::write(&dest_path, data)
                    .map_err(|e| BackupError::IoError(e.to_string()))
            })
        }
    }

    #[tokio::test]
    async fn test_full_backup_workflow_and_unpacking() {
        let tmp_root = std::env::temp_dir().join(format!("komainu_bkp_test_{}", Uuid::new_v4()));
        let data_dir = tmp_root.join("data");
        let env_file = data_dir.join("vaultwarden.env");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(data_dir.join("attachments")).unwrap();
        fs::create_dir_all(data_dir.join("sends")).unwrap();

        // Simulate created db snapshot by vaultwarden backup command
        fs::write(data_dir.join("db_20261014_220000.sqlite3"), b"SQLITE_DATA_CONTENT").unwrap();
        fs::write(data_dir.join("attachments").join("doc.pdf"), b"PDF_FILE_BYTES").unwrap();
        fs::write(data_dir.join("sends").join("send.txt"), b"SEND_FILE_BYTES").unwrap();
        fs::write(data_dir.join("rsa_key.pem"), b"RSA_KEY_PEM_CONTENT").unwrap();
        fs::write(&env_file, b"SIGNUPS_ALLOWED=false\nADMIN_TOKEN=secret").unwrap();

        // Host secret outside data dir, and symlinks pointing to it (SEC-12)
        let host_secret_file = tmp_root.join("host_secret.txt");
        fs::write(&host_secret_file, b"SUPER_SECRET_HOST_DATA").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&host_secret_file, data_dir.join("attachments").join("symlink_leak.txt")).unwrap();
            std::os::unix::fs::symlink(&host_secret_file, data_dir.join("rsa_key_symlink.pem")).unwrap();
        }

        let container_port = Arc::new(MockContainer);
        let config_store = Arc::new(MockConfigStore {
            cfg: std::sync::Mutex::new(BackupConfig {
                enabled: true,
                cron_expression: "0 3 * * *".to_string(),
                encryption_password: "my-vault-encryption-pass".to_string(),
                pcloud_region: PCloudRegion::EU,
                pcloud_token: "pcl_token_test".to_string(),
                pcloud_folder: "0".to_string(),
                pcloud_client_id: String::new(),
                pcloud_client_secret: String::new(),
            }),
            records: std::sync::Mutex::new(Vec::new()),
        });
        let pcloud_client = Arc::new(MockPCloudClient {
            uploads: AtomicUsize::new(0),
            last_uploaded_data: std::sync::Mutex::new(Vec::new()),
        });

        let service = BackupService::new(
            container_port,
            config_store.clone(),
            pcloud_client.clone(),
            "vaultwarden".to_string(),
            &data_dir,
        );

        let record = service.execute_backup().await.expect("Backup execution failed");
        assert_eq!(record.status, BackupStatus::Success);
        assert!(record.size_bytes > 0);
        assert_eq!(pcloud_client.uploads.load(Ordering::SeqCst), 1);

        // Verify snapshot was cleaned up
        assert!(!data_dir.join("db_20261014_220000.sqlite3").exists());

        // Verify history has record
        let history = service.get_history().await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, record.id);

        // Verify list_remote_backups
        let remote_files = service.list_remote_backups().await.unwrap();
        assert_eq!(remote_files.len(), 1);
        assert_eq!(remote_files[0].fileid, 99999);

        // Verify download_and_decrypt_backup
        let dest_decrypted = tmp_root.join("decrypted_test.zip");
        let bytes_written = service
            .download_and_decrypt_backup(99999, &dest_decrypted)
            .await
            .expect("Download and decrypt must succeed");
        assert!(bytes_written > 0);
        assert!(dest_decrypted.exists());

        // Inspect ZIP contents from downloaded and decrypted file
        let file = fs::File::open(&dest_decrypted).unwrap();
        let mut archive = zip::ZipArchive::new(file).expect("Must be valid ZIP");
        assert!(archive.by_name("db.sqlite3").is_ok());
        assert!(archive.by_name("attachments/doc.pdf").is_ok());
        assert!(archive.by_name("sends/send.txt").is_ok());
        assert!(archive.by_name("rsa_key.pem").is_ok());
        assert!(archive.by_name("vaultwarden.env").is_ok());

        #[cfg(unix)]
        {
            assert!(archive.by_name("attachments/symlink_leak.txt").is_err(), "Symlink in attachments must not be included");
            assert!(archive.by_name("rsa_key_symlink.pem").is_err(), "Symlink rsa_key must not be included");
        }

        // Verify decryption of the uploaded data directly
        let encrypted_bytes = pcloud_client.last_uploaded_data.lock().unwrap().clone();
        let decrypted_zip = decrypt_aes_256_gcm(&encrypted_bytes, "my-vault-encryption-pass")
            .expect("Decryption must succeed with correct password");

        let mut archive2 = zip::ZipArchive::new(std::io::Cursor::new(decrypted_zip)).expect("Must be valid ZIP");
        assert!(archive2.by_name("db.sqlite3").is_ok());

        let _ = fs::remove_dir_all(&tmp_root);
    }

    #[test]
    fn test_is_already_compressed() {
        assert!(is_already_compressed(Path::new("photo.jpg")));
        assert!(is_already_compressed(Path::new("doc.pdf")));
        assert!(is_already_compressed(Path::new("archive.zip")));
        assert!(is_already_compressed(Path::new("clip.MP4")));
        assert!(!is_already_compressed(Path::new("notes.txt")));
        assert!(!is_already_compressed(Path::new("db.sqlite3")));
        assert!(!is_already_compressed(Path::new("vaultwarden.env")));
    }

    #[test]
    fn test_komainu_temp_dir() {
        let temp_dir = komainu_temp_dir();
        assert!(temp_dir.exists());

        // Test with custom env var
        let custom_dir = std::env::temp_dir().join("komainu_custom_temp_test");
        unsafe {
            std::env::set_var("KOMAINU_TEMP_DIR", &custom_dir);
        }
        assert_eq!(komainu_temp_dir(), custom_dir);
        unsafe {
            std::env::remove_var("KOMAINU_TEMP_DIR");
        }
        let _ = fs::remove_dir_all(&custom_dir);
    }
}
