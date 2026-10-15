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

/// Encrypt arbitrary data using AES-256-GCM with key derived via PBKDF2-HMAC-SHA256 (100k rounds)
/// Output binary format: [16-byte SALT | 12-byte NONCE | 16-byte AUTH TAG | CIPHERTEXT]
pub fn encrypt_aes_256_gcm(data: &[u8], password: &str) -> Result<Vec<u8>, BackupError> {
    use openssl::hash::MessageDigest;
    use openssl::pkcs5::pbkdf2_hmac;
    use openssl::rand::rand_bytes;
    use openssl::symm::{encrypt_aead, Cipher};

    if password.is_empty() {
        return Err(BackupError::EncryptionError("Encryption password cannot be empty".to_string()));
    }

    let mut salt = [0u8; 16];
    rand_bytes(&mut salt).map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let mut key = [0u8; 32];
    pbkdf2_hmac(password.as_bytes(), &salt, 100_000, MessageDigest::sha256(), &mut key)
        .map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let mut nonce = [0u8; 12];
    rand_bytes(&mut nonce).map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let cipher = Cipher::aes_256_gcm();
    let mut tag = [0u8; 16];
    let ciphertext = encrypt_aead(cipher, &key, Some(&nonce), &[], data, &mut tag)
        .map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let mut out = Vec::with_capacity(16 + 12 + 16 + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&tag);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Stream-encrypt a file on disk using AES-256-GCM.
/// Memory usage is constant (bounded to a 64KB buffer) regardless of file size.
pub fn encrypt_file_aes_256_gcm(
    src_path: &std::path::Path,
    dst_path: &std::path::Path,
    password: &str,
) -> Result<u64, BackupError> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom, Write};
    use openssl::hash::MessageDigest;
    use openssl::pkcs5::pbkdf2_hmac;
    use openssl::rand::rand_bytes;
    use openssl::symm::{Cipher, Crypter, Mode};

    if password.is_empty() {
        return Err(BackupError::EncryptionError("Encryption password cannot be empty".to_string()));
    }

    let mut salt = [0u8; 16];
    rand_bytes(&mut salt).map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let mut key = [0u8; 32];
    pbkdf2_hmac(password.as_bytes(), &salt, 100_000, MessageDigest::sha256(), &mut key)
        .map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let mut nonce = [0u8; 12];
    rand_bytes(&mut nonce).map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let mut in_file = File::open(src_path)
        .map_err(|e| BackupError::IoError(format!("Failed to open file for encryption: {e}")))?;
    let mut out_file = File::create(dst_path)
        .map_err(|e| BackupError::IoError(format!("Failed to create encrypted file: {e}")))?;

    // Write header: salt (16), nonce (12), placeholder tag (16)
    out_file.write_all(&salt)
        .map_err(|e| BackupError::IoError(format!("Failed to write salt: {e}")))?;
    out_file.write_all(&nonce)
        .map_err(|e| BackupError::IoError(format!("Failed to write nonce: {e}")))?;
    let placeholder_tag = [0u8; 16];
    out_file.write_all(&placeholder_tag)
        .map_err(|e| BackupError::IoError(format!("Failed to write placeholder tag: {e}")))?;

    let cipher = Cipher::aes_256_gcm();
    let mut crypter = Crypter::new(cipher, Mode::Encrypt, &key, Some(&nonce))
        .map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let mut in_buf = [0u8; 64 * 1024];
    let mut out_buf = vec![0u8; in_buf.len() + cipher.block_size()];

    loop {
        let n = in_file.read(&mut in_buf)
            .map_err(|e| BackupError::IoError(format!("Failed reading input for encryption: {e}")))?;
        if n == 0 {
            break;
        }
        let count = crypter.update(&in_buf[..n], &mut out_buf)
            .map_err(|e| BackupError::EncryptionError(e.to_string()))?;
        out_file.write_all(&out_buf[..count])
            .map_err(|e| BackupError::IoError(format!("Failed writing ciphertext chunk: {e}")))?;
    }

    let final_count = crypter.finalize(&mut out_buf)
        .map_err(|e| BackupError::EncryptionError(e.to_string()))?;
    if final_count > 0 {
        out_file.write_all(&out_buf[..final_count])
            .map_err(|e| BackupError::IoError(format!("Failed writing final ciphertext chunk: {e}")))?;
    }

    let mut tag = [0u8; 16];
    crypter.get_tag(&mut tag)
        .map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    // Seek back to tag position (offset 28) and write actual auth tag
    out_file.seek(SeekFrom::Start(28))
        .map_err(|e| BackupError::IoError(format!("Failed to seek to tag offset: {e}")))?;
    out_file.write_all(&tag)
        .map_err(|e| BackupError::IoError(format!("Failed to write auth tag: {e}")))?;
    out_file.flush()
        .map_err(|e| BackupError::IoError(format!("Failed to flush encrypted file: {e}")))?;

    let total_len = out_file.metadata().map(|m| m.len()).unwrap_or(0);
    Ok(total_len)
}

/// Decrypt data produced by `encrypt_aes_256_gcm`
pub fn decrypt_aes_256_gcm(payload: &[u8], password: &str) -> Result<Vec<u8>, BackupError> {
    use openssl::hash::MessageDigest;
    use openssl::pkcs5::pbkdf2_hmac;
    use openssl::symm::{decrypt_aead, Cipher};

    if payload.len() < 16 + 12 + 16 {
        return Err(BackupError::EncryptionError("Ciphertext payload too short".to_string()));
    }

    let salt = &payload[..16];
    let nonce = &payload[16..28];
    let tag = &payload[28..44];
    let ciphertext = &payload[44..];

    let mut key = [0u8; 32];
    pbkdf2_hmac(password.as_bytes(), salt, 100_000, MessageDigest::sha256(), &mut key)
        .map_err(|e| BackupError::EncryptionError(e.to_string()))?;

    let cipher = Cipher::aes_256_gcm();
    let plaintext = decrypt_aead(cipher, &key, Some(nonce), &[], ciphertext, tag)
        .map_err(|e| BackupError::EncryptionError(format!("Decryption authentication failed: {e}")))?;

    Ok(plaintext)
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
        data: Vec<u8>,
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

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pcloud_region_api_host() {
        assert_eq!(PCloudRegion::US.api_host(), "api.pcloud.com");
        assert_eq!(PCloudRegion::EU.api_host(), "eapi.pcloud.com");
    }
}
