use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;
use passkey_auth::{
    AuthenticationChallenge, AuthenticationResponse, AuthenticationState,
    RegistrationChallenge, RegistrationResponse, RegistrationState, RpId, Webauthn,
};

use crate::domain::auth::{AuthError, CredentialStorePort};

const STATE_TTL_SECS: u64 = 300; // 5 minutos
const MAX_PENDING_STATES: usize = 50;

fn prune_timed_map<K: std::hash::Hash + Eq + Copy, V>(
    map: &mut HashMap<K, V>,
    get_instant: impl Fn(&V) -> Instant,
) {
    let now = Instant::now();
    map.retain(|_, v| now.duration_since(get_instant(v)).as_secs() < STATE_TTL_SECS);
    while map.len() >= MAX_PENDING_STATES {
        if let Some(oldest_key) = map.iter().min_by_key(|(_, v)| get_instant(v)).map(|(k, _)| *k) {
            map.remove(&oldest_key);
        } else {
            break;
        }
    }
}

fn prune_expired<T>(map: &mut HashMap<Uuid, (Instant, T)>) {
    prune_timed_map(map, |(ts, _)| *ts);
}

fn prune_expired_qr(map: &mut HashMap<Uuid, QrAuthState>) {
    prune_timed_map(map, |s| s.created_at);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QrAuthStatus {
    Pending,
    Scanned,
    Approved,
    Expired,
}

#[derive(Debug, Clone)]
pub struct QrAuthState {
    pub created_at: Instant,
    pub status: QrAuthStatus,
    pub session_admin_ts: Option<u64>,
    pub bind_secret: Option<String>,
}

pub struct AuthService {
    webauthn: Arc<Webauthn>,
    store: Arc<dyn CredentialStorePort>,
    active_reg_token: Arc<Mutex<Option<String>>>,
    reg_states: Arc<Mutex<HashMap<Uuid, (Instant, RegistrationState)>>>,
    auth_states: Arc<Mutex<HashMap<Uuid, (Instant, AuthenticationState)>>>,
    qr_states: Arc<Mutex<HashMap<Uuid, QrAuthState>>>,
    cached_credential_tag: Arc<std::sync::RwLock<Option<String>>>,
}

impl AuthService {
    pub fn new(
        rp_id: &str,
        rp_origin: &str,
        store: Arc<dyn CredentialStorePort>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let rp_id_parsed = RpId::try_from_url(rp_id).unwrap_or_else(|_| RpId::new(rp_id));
        let webauthn = Arc::new(
            Webauthn::new(rp_id_parsed.as_str(), "Komainu Admin", rp_origin)
                .authenticator_attachment(passkey_auth::Attachment::Any),
        );

        Ok(Self {
            webauthn,
            store,
            active_reg_token: Arc::new(Mutex::new(None)),
            reg_states: Arc::new(Mutex::new(HashMap::new())),
            auth_states: Arc::new(Mutex::new(HashMap::new())),
            qr_states: Arc::new(Mutex::new(HashMap::new())),
            cached_credential_tag: Arc::new(std::sync::RwLock::new(None)),
        })
    }

    pub fn credential_tag(&self) -> String {
        self.cached_credential_tag.read().unwrap().clone().unwrap_or_default()
    }

    pub fn set_credential_tag(&self, tag: String) {
        *self.cached_credential_tag.write().unwrap() = Some(tag);
    }

    pub async fn invalidate_sessions(&self) {
        let new_tag = Uuid::new_v4().to_string();
        *self.cached_credential_tag.write().unwrap() = Some(new_tag.clone());
        let _ = self.store.set_session_tag(&new_tag).await;
    }

    pub async fn init_credential_tag(&self) {
        if let Ok(Some(persisted)) = self.store.get_session_tag().await {
            *self.cached_credential_tag.write().unwrap() = Some(persisted);
            return;
        }

        if let Ok(keys) = self.store.load_credentials().await
            && let Some(first) = keys.first() {
                use sha2::{Digest, Sha256};
                let hash = Sha256::digest(first.id.as_bytes());
                let tag = format!("{:016x}", u64::from_be_bytes(hash[..8].try_into().unwrap()));
                *self.cached_credential_tag.write().unwrap() = Some(tag.clone());
                let _ = self.store.set_session_tag(&tag).await;
            }
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
    ) -> Result<(Uuid, RegistrationChallenge), AuthError> {
        if !self.is_valid_reg_token(token).await {
            return Err(AuthError::InvalidToken);
        }

        let user_id = Uuid::new_v4();
        let (ccr, reg_state) = self
            .webauthn
            .start_registration(user_id.as_bytes(), "admin", "Komainu Administrator", &[]);

        let reg_id = Uuid::new_v4();
        let mut states = self.reg_states.lock().await;
        prune_expired(&mut states);
        states.insert(reg_id, (Instant::now(), reg_state));

        Ok((reg_id, ccr))
    }

    pub async fn finish_registration(
        &self,
        reg_id: Uuid,
        reg_response: &RegistrationResponse,
    ) -> Result<(), AuthError> {
        let (_, reg_state) = {
            let mut states = self.reg_states.lock().await;
            states
                .remove(&reg_id)
                .ok_or(AuthError::SessionExpired)?
        };

        let passkey = self
            .webauthn
            .finish_registration(&reg_state, reg_response)
            .map_err(|e| AuthError::WebauthnError(e.to_string()))?;

        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(passkey.id.as_bytes());
        let tag = format!("{:016x}", u64::from_be_bytes(hash[..8].try_into().unwrap()));
        *self.cached_credential_tag.write().unwrap() = Some(tag.clone());
        let _ = self.store.set_session_tag(&tag).await;

        self.store.save_credential(passkey).await?;

        // Invalida o token de uso único
        let mut t = self.active_reg_token.lock().await;
        *t = None;

        Ok(())
    }

    pub async fn start_authentication(&self) -> Result<(Uuid, AuthenticationChallenge), AuthError> {
        let passkeys = self.store.load_credentials().await?;
        if passkeys.is_empty() {
            return Err(AuthError::NoAdminRegistered);
        }

        let (rcr, auth_state) = self
            .webauthn
            .start_authentication_with_creds(&passkeys);

        let auth_id = Uuid::new_v4();
        let mut states = self.auth_states.lock().await;
        prune_expired(&mut states);
        states.insert(auth_id, (Instant::now(), auth_state));

        Ok((auth_id, rcr))
    }

    pub async fn finish_authentication(
        &self,
        auth_id: Uuid,
        auth_response: &AuthenticationResponse,
    ) -> Result<(), AuthError> {
        let (_, auth_state) = {
            let mut states = self.auth_states.lock().await;
            states
                .remove(&auth_id)
                .ok_or(AuthError::SessionExpired)?
        };

        let passkeys = self.store.load_credentials().await?;
        let passkey = passkeys.first().ok_or(AuthError::NoAdminRegistered)?;

        let outcome = self
            .webauthn
            .finish_authentication(&auth_state, auth_response, passkey)
            .map_err(|e| AuthError::WebauthnError(e.to_string()))?;

        if outcome.new_counter > passkey.counter {
            let mut updated = passkey.clone();
            updated.counter = outcome.new_counter;
            let _ = self.store.save_credential(updated).await;
        }

        Ok(())
    }

    pub async fn create_qr_session(&self, bind_secret: Option<String>) -> Uuid {
        let qr_id = Uuid::new_v4();
        let mut states = self.qr_states.lock().await;
        prune_expired_qr(&mut states);
        states.insert(qr_id, QrAuthState {
            created_at: Instant::now(),
            status: QrAuthStatus::Pending,
            session_admin_ts: None,
            bind_secret,
        });
        qr_id
    }

    pub async fn get_qr_status(&self, qr_id: Uuid) -> QrAuthStatus {
        let states = self.qr_states.lock().await;
        if let Some(state) = states.get(&qr_id) {
            if Instant::now().duration_since(state.created_at).as_secs() >= STATE_TTL_SECS {
                QrAuthStatus::Expired
            } else {
                state.status
            }
        } else {
            QrAuthStatus::Expired
        }
    }

    pub async fn mark_qr_scanned(&self, qr_id: Uuid) -> bool {
        let mut states = self.qr_states.lock().await;
        if let Some(state) = states.get_mut(&qr_id)
            && Instant::now().duration_since(state.created_at).as_secs() < STATE_TTL_SECS {
                if state.status == QrAuthStatus::Pending {
                    state.status = QrAuthStatus::Scanned;
                }
                return true;
            }
        false
    }

    pub async fn approve_qr_session(&self, qr_id: Uuid, admin_ts: u64) -> bool {
        let mut states = self.qr_states.lock().await;
        if let Some(state) = states.get_mut(&qr_id)
            && Instant::now().duration_since(state.created_at).as_secs() < STATE_TTL_SECS {
                state.status = QrAuthStatus::Approved;
                state.session_admin_ts = Some(admin_ts);
                return true;
            }
        false
    }

    pub async fn consume_approved_qr_session(&self, qr_id: Uuid, provided_secret: Option<&str>) -> Option<u64> {
        let mut states = self.qr_states.lock().await;
        if let Some(state) = states.get(&qr_id)
            && state.status == QrAuthStatus::Approved {
                if let Some(ref required_secret) = state.bind_secret
                    && provided_secret != Some(required_secret.as_str()) {
                        return None;
                    }
                let ts = state.session_admin_ts;
                states.remove(&qr_id);
                return ts;
            }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prune_expired_evicts_oldest_when_saturated() {
        let mut map: HashMap<Uuid, (Instant, u32)> = HashMap::new();
        let now = Instant::now();

        // Insere MAX_PENDING_STATES itens
        let oldest_uuid = Uuid::new_v4();
        map.insert(oldest_uuid, (now - std::time::Duration::from_secs(60), 0));

        let mut latest_uuids = Vec::new();
        for i in 1..MAX_PENDING_STATES {
            let u = Uuid::new_v4();
            map.insert(u, (now - std::time::Duration::from_millis((MAX_PENDING_STATES - i) as u64 * 10), i as u32));
            latest_uuids.push(u);
        }

        assert_eq!(map.len(), MAX_PENDING_STATES);

        // Ao rodar prune_expired, o item mais antigo deve ser removido, deixando espaço para o novo
        prune_expired(&mut map);

        assert_eq!(map.len(), MAX_PENDING_STATES - 1);
        assert!(!map.contains_key(&oldest_uuid), "O item mais antigo deveria ter sido removido");

        for u in latest_uuids {
            assert!(map.contains_key(&u), "Os itens recentes devem ser preservados");
        }
    }

    #[test]
    fn test_prune_expired_qr_evicts_oldest() {
        let mut map: HashMap<Uuid, QrAuthState> = HashMap::new();
        let now = Instant::now();

        let oldest_uuid = Uuid::new_v4();
        map.insert(oldest_uuid, QrAuthState {
            created_at: now - std::time::Duration::from_secs(20),
            status: QrAuthStatus::Pending,
            session_admin_ts: None,
            bind_secret: None,
        });

        for _ in 1..MAX_PENDING_STATES {
            map.insert(Uuid::new_v4(), QrAuthState {
                created_at: now,
                status: QrAuthStatus::Pending,
                session_admin_ts: None,
                bind_secret: None,
            });
        }

        assert_eq!(map.len(), MAX_PENDING_STATES);
        prune_expired_qr(&mut map);

        assert_eq!(map.len(), MAX_PENDING_STATES - 1);
        assert!(!map.contains_key(&oldest_uuid));
    }
}

