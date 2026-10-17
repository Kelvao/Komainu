use super::*;
use crate::domain::backup::{
    decrypt_aes_256_gcm, BackupConfig, BackupConfigStorePort, BackupError, BackupRecord,
    BackupStatus, BoxFuture, PCloudClientPort, PCloudRegion, PCloudRemoteFile,
};
use crate::domain::container::{ContainerError, ContainerPort, ContainerState, ContainerStatus};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

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
    fn start(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        Box::pin(async { Ok(()) })
    }
    fn stop(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        Box::pin(async { Ok(()) })
    }
    fn restart(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        Box::pin(async { Ok(()) })
    }
    fn update_image(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        Box::pin(async { Ok(()) })
    }
    fn rebuild(&self, _: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        Box::pin(async { Ok(()) })
    }
    fn exec_command(
        &self,
        _: &str,
        _: Vec<String>,
    ) -> BoxFuture<'_, Result<String, ContainerError>> {
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
            fs::write(&dest_path, data).map_err(|e| BackupError::IoError(e.to_string()))
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
    fs::write(
        data_dir.join("db_20261014_220000.sqlite3"),
        b"SQLITE_DATA_CONTENT",
    )
    .unwrap();
    fs::write(
        data_dir.join("attachments").join("doc.pdf"),
        b"PDF_FILE_BYTES",
    )
    .unwrap();
    fs::write(data_dir.join("sends").join("send.txt"), b"SEND_FILE_BYTES").unwrap();
    fs::write(data_dir.join("rsa_key.pem"), b"RSA_KEY_PEM_CONTENT").unwrap();
    fs::write(&env_file, b"SIGNUPS_ALLOWED=false\nADMIN_TOKEN=secret").unwrap();

    // Host secret outside data dir, and symlinks pointing to it (SEC-12)
    let host_secret_file = tmp_root.join("host_secret.txt");
    fs::write(&host_secret_file, b"SUPER_SECRET_HOST_DATA").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            &host_secret_file,
            data_dir.join("attachments").join("symlink_leak.txt"),
        )
        .unwrap();
        std::os::unix::fs::symlink(&host_secret_file, data_dir.join("rsa_key_symlink.pem"))
            .unwrap();
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

    let record = service
        .execute_backup()
        .await
        .expect("Backup execution failed");
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
        assert!(
            archive.by_name("attachments/symlink_leak.txt").is_err(),
            "Symlink in attachments must not be included"
        );
        assert!(
            archive.by_name("rsa_key_symlink.pem").is_err(),
            "Symlink rsa_key must not be included"
        );
    }

    // Verify decryption of the uploaded data directly
    let encrypted_bytes = pcloud_client.last_uploaded_data.lock().unwrap().clone();
    let decrypted_zip = decrypt_aes_256_gcm(&encrypted_bytes, "my-vault-encryption-pass")
        .expect("Decryption must succeed with correct password");

    let mut archive2 =
        zip::ZipArchive::new(std::io::Cursor::new(decrypted_zip)).expect("Must be valid ZIP");
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

