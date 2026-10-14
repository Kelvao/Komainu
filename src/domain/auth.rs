use std::fmt;
use std::future::Future;
use std::pin::Pin;
use webauthn_rs::prelude::Passkey;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone)]
pub enum AuthError {
    NoAdminRegistered,
    AlreadyRegistered,
    InvalidToken,
    StoreError(String),
    WebauthnError(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdminRegistered => write!(f, "Nenhum administrador cadastrado."),
            Self::AlreadyRegistered => write!(f, "Administrador já cadastrado."),
            Self::InvalidToken => write!(f, "Token de registro inválido ou expirado."),
            Self::StoreError(e) => write!(f, "Erro no armazenamento de credenciais: {e}"),
            Self::WebauthnError(e) => write!(f, "Erro WebAuthn: {e}"),
        }
    }
}

impl std::error::Error for AuthError {}

pub trait CredentialStorePort: Send + Sync {
    fn load_credentials(&self) -> BoxFuture<'_, Result<Vec<Passkey>, AuthError>>;
    fn save_credential(&self, passkey: Passkey) -> BoxFuture<'_, Result<(), AuthError>>;
    fn has_registered_admin(&self) -> BoxFuture<'_, bool>;
}

