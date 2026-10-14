use std::fmt;
use std::future::Future;
use std::pin::Pin;
use webauthn_rs::prelude::Passkey;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    NoAdminRegistered,
    AlreadyRegistered,
    InvalidToken,
    SessionExpired,
    StoreError(String),
    WebauthnError(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdminRegistered => write!(f, "No administrator registered."),
            Self::AlreadyRegistered => write!(f, "Administrator already registered."),
            Self::InvalidToken => write!(f, "Invalid or expired registration token."),
            Self::SessionExpired => write!(f, "Authentication session expired or not found."),
            Self::StoreError(e) => write!(f, "Credential store error: {e}"),
            Self::WebauthnError(e) => write!(f, "WebAuthn error: {e}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl AuthError {
    pub fn to_localized(&self, i18n: &crate::i18n::Translations) -> String {
        match self {
            Self::NoAdminRegistered => i18n.errors.no_admin_registered.clone(),
            Self::AlreadyRegistered => i18n.errors.already_registered.clone(),
            Self::InvalidToken => i18n.errors.invalid_token.clone(),
            Self::SessionExpired => i18n.errors.session_expired.clone(),
            Self::StoreError(e) => format!("{}: {e}", i18n.errors.store_error),
            Self::WebauthnError(e) => format!("{}: {e}", i18n.errors.webauthn_error),
        }
    }
}

pub trait CredentialStorePort: Send + Sync {
    fn load_credentials(&self) -> BoxFuture<'_, Result<Vec<Passkey>, AuthError>>;
    fn save_credential(&self, passkey: Passkey) -> BoxFuture<'_, Result<(), AuthError>>;
    fn has_registered_admin(&self) -> BoxFuture<'_, bool>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::I18n;

    #[test]
    fn test_auth_error_localization() {
        let i18n_pt = I18n::load("pt-BR");
        let i18n_en = I18n::load("en");

        let err = AuthError::NoAdminRegistered;
        assert_eq!(err.to_localized(&i18n_pt), "Nenhum administrador cadastrado. Gere uma chave de acesso com './komainu --register'.");
        assert_eq!(err.to_localized(&i18n_en), "No administrator registered. Generate an access key using './komainu --register'.");

        let exp_err = AuthError::SessionExpired;
        assert_eq!(exp_err.to_localized(&i18n_pt), "Sessão de autenticação expirada ou inexistente. Tente novamente.");
        assert_eq!(exp_err.to_localized(&i18n_en), "Authentication session expired or not found. Please try again.");
    }
}

