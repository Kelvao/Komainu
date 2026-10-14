use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::domain::auth::{AuthError, CredentialStorePort};

const STATE_TTL_SECS: u64 = 300; // 5 minutos
const MAX_PENDING_STATES: usize = 50;

fn prune_expired<T>(map: &mut HashMap<Uuid, (Instant, T)>) {
    let now = Instant::now();
    map.retain(|_, (created_at, _)| now.duration_since(*created_at).as_secs() < STATE_TTL_SECS);
    if map.len() >= MAX_PENDING_STATES {
        map.clear();
    }
}

pub struct AuthService {
    webauthn: Arc<Webauthn>,
    store: Arc<dyn CredentialStorePort>,
    active_reg_token: Arc<Mutex<Option<String>>>,
    reg_states: Arc<Mutex<HashMap<Uuid, (Instant, PasskeyRegistration)>>>,
    auth_states: Arc<Mutex<HashMap<Uuid, (Instant, PasskeyAuthentication)>>>,
}

impl AuthService {
    pub fn new(
        rp_id: &str,
        rp_origin: &str,
        store: Arc<dyn CredentialStorePort>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let url = Url::parse(rp_origin)?;
        let builder = WebauthnBuilder::new(rp_id, &url)?
            .rp_name("Komainu Admin");
        let webauthn = Arc::new(builder.build()?);

        Ok(Self {
            webauthn,
            store,
            active_reg_token: Arc::new(Mutex::new(None)),
            reg_states: Arc::new(Mutex::new(HashMap::new())),
            auth_states: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn set_registration_token(&self, token: String) {
        let mut t = self.active_reg_token.lock().await;
        *t = Some(token);
    }

    pub async fn is_valid_reg_token(&self, token: &str) -> bool {
        let t = self.active_reg_token.lock().await;
        t.as_deref() == Some(token)
    }

    pub async fn has_admin(&self) -> bool {
        self.store.has_registered_admin().await
    }

    pub async fn start_registration(
        &self,
        token: &str,
    ) -> Result<(Uuid, CreationChallengeResponse), AuthError> {
        if !self.is_valid_reg_token(token).await {
            return Err(AuthError::InvalidToken);
        }

        let user_id = Uuid::new_v4();
        let (ccr, reg_state) = self
            .webauthn
            .start_passkey_registration(user_id, "admin", "Komainu Administrator", None)
            .map_err(|e| AuthError::WebauthnError(e.to_string()))?;

        let reg_id = Uuid::new_v4();
        let mut states = self.reg_states.lock().await;
        prune_expired(&mut states);
        states.insert(reg_id, (Instant::now(), reg_state));

        Ok((reg_id, ccr))
    }

    pub async fn finish_registration(
        &self,
        reg_id: Uuid,
        reg_response: &RegisterPublicKeyCredential,
    ) -> Result<(), AuthError> {
        let (_, reg_state) = {
            let mut states = self.reg_states.lock().await;
            states
                .remove(&reg_id)
                .ok_or(AuthError::SessionExpired)?
        };

        let passkey = self
            .webauthn
            .finish_passkey_registration(reg_response, &reg_state)
            .map_err(|e| AuthError::WebauthnError(e.to_string()))?;

        self.store.save_credential(passkey).await?;

        // Invalida o token de uso único
        let mut t = self.active_reg_token.lock().await;
        *t = None;

        Ok(())
    }

    pub async fn start_authentication(&self) -> Result<(Uuid, RequestChallengeResponse), AuthError> {
        let passkeys = self.store.load_credentials().await?;
        if passkeys.is_empty() {
            return Err(AuthError::NoAdminRegistered);
        }

        let (rcr, auth_state) = self
            .webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|e| AuthError::WebauthnError(e.to_string()))?;

        let auth_id = Uuid::new_v4();
        let mut states = self.auth_states.lock().await;
        prune_expired(&mut states);
        states.insert(auth_id, (Instant::now(), auth_state));

        Ok((auth_id, rcr))
    }

    pub async fn finish_authentication(
        &self,
        auth_id: Uuid,
        auth_response: &PublicKeyCredential,
    ) -> Result<(), AuthError> {
        let (_, auth_state) = {
            let mut states = self.auth_states.lock().await;
            states
                .remove(&auth_id)
                .ok_or(AuthError::SessionExpired)?
        };

        self.webauthn
            .finish_passkey_authentication(auth_response, &auth_state)
            .map_err(|e| AuthError::WebauthnError(e.to_string()))?;

        Ok(())
    }
}
