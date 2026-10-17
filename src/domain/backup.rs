use std::fmt;
use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PCloudRegion {
    US,
    #[default]
    EU,
}

impl PCloudRegion {
    pub fn api_host(&self) -> &'static str {
        match self {
            Self::US => "api.pcloud.com",
            Self::EU => "eapi.pcloud.com",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupConfig {
    pub enabled: bool,
    pub cron_expression: String,
    pub encryption_password: String,
    pub pcloud_region: PCloudRegion,
    pub pcloud_token: String,
    pub pcloud_folder: String,
    #[serde(default)]
    pub pcloud_client_id: String,
    #[serde(default)]
    pub pcloud_client_secret: String,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cron_expression: "0 3 * * *".to_string(), // Daily at 03:00
            encryption_password: String::new(),
            pcloud_region: PCloudRegion::EU,
            pcloud_token: String::new(),
            pcloud_folder: "0".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackupStatus {
    Success,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupRecord {
    pub id: String,
    pub timestamp: u64,
    pub filename: String,
    pub size_bytes: u64,
    pub duration_secs: u64,
    pub status: BackupStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupError {
    ContainerExecFailed(String),
    DataPathNotFound(String),
    DatabaseBackupNotFound(String),
    ZipError(String),
    EncryptionError(String),
    PCloudUploadFailed(String),
    ConfigError(String),
    IoError(String),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContainerExecFailed(msg) => write!(f, "Vaultwarden backup command failed: {msg}"),
            Self::DataPathNotFound(p) => write!(f, "Vaultwarden data directory not found: {p}"),
            Self::DatabaseBackupNotFound(msg) => write!(f, "SQLite backup file not found after command: {msg}"),
            Self::ZipError(msg) => write!(f, "ZIP packaging error: {msg}"),
            Self::EncryptionError(msg) => write!(f, "Encryption error: {msg}"),
            Self::PCloudUploadFailed(msg) => write!(f, "pCloud upload failed: {msg}"),
            Self::ConfigError(msg) => write!(f, "Configuration error: {msg}"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for BackupError {}

use ring::aead::{Aad, LessSafeKey, Nonce, Tag, UnboundKey, AES_256_GCM};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::num::NonZeroU32;

fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(100_000).unwrap(),
        salt,
        password.as_bytes(),
        &mut key,
    );
    key
}

fn generate_random<const N: usize>(rng: &SystemRandom) -> Result<[u8; N], BackupError> {
    let mut bytes = [0u8; N];
    rng.fill(&mut bytes)
        .map_err(|e| BackupError::EncryptionError(format!("Secure random generation failed: {e}")))?;
    Ok(bytes)
}

/// Encrypt arbitrary data using AES-256-GCM with key derived via PBKDF2-HMAC-SHA256 (100k rounds)
/// Output binary format: [16-byte SALT | 12-byte NONCE | 16-byte AUTH TAG | CIPHERTEXT]
pub fn encrypt_aes_256_gcm(data: &[u8], password: &str) -> Result<Vec<u8>, BackupError> {
    if password.is_empty() {
        return Err(BackupError::EncryptionError("Encryption password cannot be empty".to_string()));
    }

    let rng = SystemRandom::new();
    let salt: [u8; 16] = generate_random(&rng)?;
    let key_bytes = derive_key(password, &salt);
    let nonce_bytes: [u8; 12] = generate_random(&rng)?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
        .map_err(|_| BackupError::EncryptionError("Failed to initialize AES-256-GCM key".to_string()))?;
    let key = LessSafeKey::new(unbound_key);

    let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
        .map_err(|_| BackupError::EncryptionError("Invalid nonce".to_string()))?;

    let mut in_out = data.to_vec();
    let tag = key
        .seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| BackupError::EncryptionError("Encryption failed".to_string()))?;

    let mut out = Vec::with_capacity(16 + 12 + 16 + in_out.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(tag.as_ref());
    out.extend_from_slice(&in_out);
    Ok(out)
}

/// Stream-encrypt a file on disk using AES-256-GCM.
pub fn encrypt_file_aes_256_gcm(
    src_path: &std::path::Path,
    dst_path: &std::path::Path,
    password: &str,
) -> Result<u64, BackupError> {
    use std::fs::File;
    use std::io::{BufWriter, Read, Write};

    if password.is_empty() {
        return Err(BackupError::EncryptionError("Encryption password cannot be empty".to_string()));
    }

    let rng = SystemRandom::new();
    let salt: [u8; 16] = generate_random(&rng)?;
    let key_bytes = derive_key(password, &salt);
    let nonce_bytes: [u8; 12] = generate_random(&rng)?;

    let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
        .map_err(|_| BackupError::EncryptionError("Failed to initialize AES-256-GCM key".to_string()))?;
    let key = LessSafeKey::new(unbound_key);

    let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
        .map_err(|_| BackupError::EncryptionError("Invalid nonce".to_string()))?;

    let mut src_file = File::open(src_path)
        .map_err(|e| BackupError::IoError(format!("Failed to open file for encryption: {e}")))?;
    let file_len = src_file
        .metadata()
        .map_err(|e| BackupError::IoError(format!("Failed reading file metadata: {e}")))?
        .len() as usize;

    let mut in_out = Vec::with_capacity(file_len);
    src_file
        .read_to_end(&mut in_out)
        .map_err(|e| BackupError::IoError(format!("Failed reading file for encryption: {e}")))?;

    let tag = key
        .seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| BackupError::EncryptionError("Encryption failed".to_string()))?;

    let dst_file = File::create(dst_path)
        .map_err(|e| BackupError::IoError(format!("Failed to create encrypted file: {e}")))?;
    let mut writer = BufWriter::with_capacity(128 * 1024, dst_file);

    writer
        .write_all(&salt)
        .map_err(|e| BackupError::IoError(format!("Failed to write salt: {e}")))?;
    writer
        .write_all(&nonce_bytes)
        .map_err(|e| BackupError::IoError(format!("Failed to write nonce: {e}")))?;
    writer
        .write_all(tag.as_ref())
        .map_err(|e| BackupError::IoError(format!("Failed to write tag: {e}")))?;
    writer
        .write_all(&in_out)
        .map_err(|e| BackupError::IoError(format!("Failed writing ciphertext: {e}")))?;
    writer
        .flush()
        .map_err(|e| BackupError::IoError(format!("Failed flushing encrypted file: {e}")))?;

    let total_len = (16 + 12 + 16 + in_out.len()) as u64;
    Ok(total_len)
}

/// Decrypt data produced by `encrypt_aes_256_gcm`
pub fn decrypt_aes_256_gcm(payload: &[u8], password: &str) -> Result<Vec<u8>, BackupError> {
    if password.is_empty() {
        return Err(BackupError::EncryptionError("Decryption password cannot be empty".to_string()));
    }
    if payload.len() < 16 + 12 + 16 {
        return Err(BackupError::EncryptionError("Ciphertext payload too short".to_string()));
    }

    let salt = &payload[..16];
    let nonce_bytes = &payload[16..28];
    let tag_bytes = &payload[28..44];
    let ciphertext = &payload[44..];

    let key_bytes = derive_key(password, salt);

    let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
        .map_err(|_| BackupError::EncryptionError("Failed to initialize AES-256-GCM key".to_string()))?;
    let key = LessSafeKey::new(unbound_key);

    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|_| BackupError::EncryptionError("Invalid nonce".to_string()))?;

    let tag = Tag::try_from(tag_bytes)
        .map_err(|_| BackupError::EncryptionError("Invalid authentication tag".to_string()))?;

    let mut in_out = ciphertext.to_vec();
    key.open_in_place_separate_tag(nonce, Aad::empty(), tag, &mut in_out, 0..)
        .map_err(|_| BackupError::EncryptionError("Decryption authentication failed".to_string()))?;

    Ok(in_out)
}

/// Stream-decrypt a file on disk using AES-256-GCM.
pub fn decrypt_file_aes_256_gcm(
    src_path: &std::path::Path,
    dst_path: &std::path::Path,
    password: &str,
) -> Result<u64, BackupError> {
    let result = (|| -> Result<u64, BackupError> {
        use std::fs::File;
        use std::io::{BufWriter, Read, Write};

        if password.is_empty() {
            return Err(BackupError::EncryptionError("Decryption password cannot be empty".to_string()));
        }

        let mut in_file = File::open(src_path)
            .map_err(|e| BackupError::IoError(format!("Failed to open file for decryption: {e}")))?;

        let mut header = [0u8; 16 + 12 + 16];
        in_file
            .read_exact(&mut header)
            .map_err(|e| BackupError::EncryptionError(format!("Ciphertext payload too short or invalid header: {e}")))?;

        let salt = &header[..16];
        let nonce_bytes = &header[16..28];
        let tag_bytes = &header[28..44];

        let mut in_out = Vec::new();
        in_file
            .read_to_end(&mut in_out)
            .map_err(|e| BackupError::IoError(format!("Failed reading encrypted stream: {e}")))?;

        let key_bytes = derive_key(password, salt);

        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
            .map_err(|_| BackupError::EncryptionError("Failed to initialize AES-256-GCM key".to_string()))?;
        let key = LessSafeKey::new(unbound_key);

        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| BackupError::EncryptionError("Invalid nonce".to_string()))?;

        let tag = Tag::try_from(tag_bytes)
            .map_err(|_| BackupError::EncryptionError("Invalid authentication tag".to_string()))?;

        key.open_in_place_separate_tag(nonce, Aad::empty(), tag, &mut in_out, 0..)
            .map_err(|_| BackupError::EncryptionError("Decryption authentication failed".to_string()))?;

        let out_file = File::create(dst_path)
            .map_err(|e| BackupError::IoError(format!("Failed to create decrypted file: {e}")))?;
        let mut writer = BufWriter::with_capacity(128 * 1024, out_file);
        writer
            .write_all(&in_out)
            .map_err(|e| BackupError::IoError(format!("Failed writing decrypted data: {e}")))?;
        writer
            .flush()
            .map_err(|e| BackupError::IoError(format!("Failed flushing decrypted file: {e}")))?;

        Ok(in_out.len() as u64)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(dst_path);
    }

    result
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PCloudRemoteFile {
    pub fileid: u64,
    pub name: String,
    pub size_bytes: u64,
    pub modified: String,
}

pub struct TempFileGuard(pub std::path::PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub trait BackupConfigStorePort: Send + Sync {
    fn load_config(&self) -> BoxFuture<'_, Result<BackupConfig, BackupError>>;
    fn save_config(&self, config: &BackupConfig) -> BoxFuture<'_, Result<(), BackupError>>;
    fn load_history(&self) -> BoxFuture<'_, Result<Vec<BackupRecord>, BackupError>>;
    fn add_record(&self, record: BackupRecord) -> BoxFuture<'_, Result<(), BackupError>>;
}

pub trait PCloudClientPort: Send + Sync {
    fn upload(
        &self,
        region: &PCloudRegion,
        token: &str,
        folder: &str,
        filename: &str,
        file_path: &std::path::Path,
    ) -> BoxFuture<'_, Result<(), BackupError>>;

    fn list_folder(
        &self,
        region: &PCloudRegion,
        token: &str,
        folder: &str,
    ) -> BoxFuture<'_, Result<Vec<PCloudRemoteFile>, BackupError>>;

    fn download_file(
        &self,
        region: &PCloudRegion,
        token: &str,
        fileid: u64,
        dest_path: &std::path::Path,
    ) -> BoxFuture<'_, Result<(), BackupError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_gcm_roundtrip() {
        let original_data = b"Vaultwarden super secret database and attachments payload";
        let password = "komainu-strong-backup-key-1234";

        let encrypted = encrypt_aes_256_gcm(original_data, password).expect("Encryption failed");
        assert_ne!(encrypted, original_data);
        assert!(encrypted.len() > original_data.len());

        let decrypted = decrypt_aes_256_gcm(&encrypted, password).expect("Decryption failed");
        assert_eq!(decrypted, original_data);
    }

    #[test]
    fn test_aes_gcm_wrong_password_fails() {
        let original_data = b"secret data";
        let encrypted = encrypt_aes_256_gcm(original_data, "correct-pass").unwrap();
        let result = decrypt_aes_256_gcm(&encrypted, "wrong-pass");
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_file_aes_256_gcm_roundtrip() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("test_enc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("plain.bin");
        let dst = dir.join("cipher.bin");

        let original_data = vec![42u8; 150_000]; // 150KB to test multi-chunk encryption
        let mut f = std::fs::File::create(&src).unwrap();
        f.write_all(&original_data).unwrap();
        drop(f);

        let pass = "test-strong-password";
        let written = encrypt_file_aes_256_gcm(&src, &dst, pass).expect("File encryption failed");
        assert!(written > original_data.len() as u64);

        let encrypted_bytes = std::fs::read(&dst).unwrap();
        let decrypted = decrypt_aes_256_gcm(&encrypted_bytes, pass).expect("Decryption failed");
        assert_eq!(decrypted, original_data);

        let decrypted_file = dir.join("decrypted.bin");
        decrypt_file_aes_256_gcm(&dst, &decrypted_file, pass).expect("File decryption failed");
        let decrypted_from_file = std::fs::read(&decrypted_file).unwrap();
        assert_eq!(decrypted_from_file, original_data);

        // Verify wrong password fails and immediately cleans up destination file
        let bad_decrypted_file = dir.join("bad_decrypted.bin");
        let err = decrypt_file_aes_256_gcm(&dst, &bad_decrypted_file, "wrong-password");
        assert!(err.is_err());
        assert!(!bad_decrypted_file.exists(), "Destination file must not remain on disk after decryption failure");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pcloud_region_api_host() {
        assert_eq!(PCloudRegion::US.api_host(), "api.pcloud.com");
        assert_eq!(PCloudRegion::EU.api_host(), "eapi.pcloud.com");
    }
}
