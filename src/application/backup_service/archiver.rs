use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::domain::backup::BackupError;

pub fn is_already_compressed(path: &Path) -> bool {
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

pub fn build_zip_archive(
    vw_data_path: &Path,
    sqlite_path: &Path,
    out_path: &Path,
    is_temp_snapshot: bool,
) -> Result<(), BackupError> {
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
            && meta.file_type().is_symlink()
        {
            return Err(BackupError::IoError(
                "Sqlite database snapshot cannot be a symlink".to_string(),
            ));
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
                add_directory_to_zip(&mut zip, &sub_dir, dir_name, options)?;
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
                    && name.starts_with("rsa_key")
                    && file_type.is_file()
                {
                    let mut key_file = fs::File::open(&path)
                        .map_err(|e| BackupError::IoError(format!("Failed to open RSA key {name}: {e}")))?;
                    zip.start_file(name, options)
                        .map_err(|e| BackupError::ZipError(format!("Failed to start RSA key {name} in zip: {e}")))?;
                    std::io::copy(&mut key_file, &mut zip)
                        .map_err(|e| BackupError::IoError(format!("Failed to write RSA key {name} to zip: {e}")))?;
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
            if meta.is_file() {
                let mut env_file = fs::File::open(&env_candidate)
                    .map_err(|e| BackupError::IoError(format!("Failed to open env file {env_name}: {e}")))?;
                zip.start_file(*env_name, options)
                    .map_err(|e| BackupError::ZipError(format!("Failed to start env file {env_name} in zip: {e}")))?;
                std::io::copy(&mut env_file, &mut zip)
                    .map_err(|e| BackupError::IoError(format!("Failed to write env file {env_name} to zip: {e}")))?;
                break;
            }
        }
    }

    let mut writer = zip
        .finish()
        .map_err(|e| BackupError::ZipError(e.to_string()))?;
    writer
        .flush()
        .map_err(|e| BackupError::IoError(format!("Failed to flush backup zip buffer: {e}")))?;
    Ok(())
}

pub fn add_directory_to_zip<W: Write + std::io::Seek>(
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
            add_directory_to_zip(zip, &path, &zip_path, options)?;
        } else if file_type.is_file() {
            let file_options = if is_already_compressed(&path) {
                options.compression_method(zip::CompressionMethod::Stored)
            } else {
                options
            };
            let file = fs::File::open(&path)
                .map_err(|e| BackupError::IoError(format!("Failed to open file {:?}: {e}", path)))?;
            zip.start_file(&zip_path, file_options)
                .map_err(|e| BackupError::ZipError(format!("Failed to start file in zip {zip_path}: {e}")))?;
            let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
            std::io::copy(&mut reader, zip)
                .map_err(|e| BackupError::IoError(format!("Failed writing file {zip_path} to zip: {e}")))?;
        }
    }

    Ok(())
}

