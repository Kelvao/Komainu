use std::path::{Path, PathBuf};
use tokio::fs;
use webauthn_rs::prelude::Passkey;

use crate::domain::auth::{AuthError, BoxFuture, CredentialStorePort};

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

            if let Some(parent) = self.path.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent).await;
                }
            }

            fs::write(&self.path, json)
                .await
                .map_err(|e| AuthError::StoreError(format!("Falha ao salvar credencial: {e}")))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600)).await;
            }

            Ok(())
        })
    }

    fn has_registered_admin(&self) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            matches!(self.load_credentials().await, Ok(keys) if !keys.is_empty())
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
}

