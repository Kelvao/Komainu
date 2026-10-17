use std::path::{Path, PathBuf};
use tokio::fs;
use crate::domain::auth::{AuthError, BoxFuture, CredentialStorePort, Passkey};

pub struct FileCredentialStore {
    path: PathBuf,
}

impl FileCredentialStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl CredentialStorePort for FileCredentialStore {
    fn load_credentials(&self) -> BoxFuture<'_, Result<Vec<Passkey>, AuthError>> {
        Box::pin(async move {
            if !self.path.exists() {
                return Ok(Vec::new());
            }

            let data = fs::read(&self.path)
                .await
                .map_err(|e| AuthError::StoreError(format!("Falha ao ler arquivo: {e}")))?;

            if data.is_empty() {
                return Ok(Vec::new());
            }

            let keys: Vec<Passkey> = serde_json::from_slice(&data)
                .map_err(|e| AuthError::StoreError(format!("Falha ao deserializar credenciais: {e}")))?;

            Ok(keys)
        })
    }

    fn save_credential(&self, passkey: Passkey) -> BoxFuture<'_, Result<(), AuthError>> {
        Box::pin(async move {
            let keys = vec![passkey]; // Exclusivo para um único administrador

            let json = serde_json::to_string_pretty(&keys)
                .map_err(|e| AuthError::StoreError(format!("Falha ao serializar credencial: {e}")))?;

            if let Some(parent) = self.path.parent()
                && !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent).await;
                }

            let tmp_path = format!("{}.tmp", self.path.display());

            fs::write(&tmp_path, json)
                .await
                .map_err(|e| AuthError::StoreError(format!("Falha ao salvar credencial temporária: {e}")))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600)).await;
            }

            fs::rename(&tmp_path, &self.path)
                .await
                .map_err(|e| AuthError::StoreError(format!("Falha ao aplicar credencial atomicamente: {e}")))?;

            Ok(())
        })
    }

    fn has_registered_admin(&self) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            matches!(self.load_credentials().await, Ok(keys) if !keys.is_empty())
        })
    }

    fn get_session_tag(&self) -> BoxFuture<'_, Result<Option<String>, AuthError>> {
        Box::pin(async move {
            let tag_path = self.path.with_extension("session_tag");
            if !tag_path.exists() {
                return Ok(None);
            }
            let data = fs::read_to_string(&tag_path)
                .await
                .map_err(|e| AuthError::StoreError(format!("Falha ao ler session tag: {e}")))?;
            let trimmed = data.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        })
    }

    fn set_session_tag(&self, tag: &str) -> BoxFuture<'_, Result<(), AuthError>> {
        let tag = tag.to_string();
        Box::pin(async move {
            let tag_path = self.path.with_extension("session_tag");
            let tmp_path = self.path.with_extension("session_tag.tmp");

            if let Some(parent) = self.path.parent()
                && !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent).await;
                }

            fs::write(&tmp_path, &tag)
                .await
                .map_err(|e| AuthError::StoreError(format!("Falha ao gravar session tag temporária: {e}")))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600)).await;
            }

            fs::rename(&tmp_path, &tag_path)
                .await
                .map_err(|e| AuthError::StoreError(format!("Falha ao aplicar session tag atomicamente: {e}")))?;

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_file_credential_store() {
        let temp_path = std::env::temp_dir().join(format!("komainu_test_{}.json", uuid::Uuid::new_v4()));
        let store = FileCredentialStore::new(&temp_path);

        assert!(!store.has_registered_admin().await);
        let creds = store.load_credentials().await.expect("Failed to load");
        assert!(creds.is_empty());

        let _ = tokio::fs::remove_file(&temp_path).await;
    }

    #[tokio::test]
    async fn test_session_tag_persistence() {
        let temp_path = std::env::temp_dir().join(format!("komainu_tag_test_{}.json", uuid::Uuid::new_v4()));
        let store = FileCredentialStore::new(&temp_path);

        assert_eq!(store.get_session_tag().await.unwrap(), None);

        store.set_session_tag("test_epoch_tag_123").await.unwrap();
        assert_eq!(store.get_session_tag().await.unwrap(), Some("test_epoch_tag_123".to_string()));

        let tag_file = temp_path.with_extension("session_tag");
        let _ = tokio::fs::remove_file(&tag_file).await;
        let _ = tokio::fs::remove_file(&temp_path).await;
    }
}


