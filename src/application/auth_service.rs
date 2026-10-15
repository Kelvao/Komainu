use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;
use webauthn_rs::prelude::*;

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
}

pub struct AuthService {
    webauthn: Arc<Webauthn>,
    store: Arc<dyn CredentialStorePort>,
    active_reg_token: Arc<Mutex<Option<String>>>,
    reg_states: Arc<Mutex<HashMap<Uuid, (Instant, PasskeyRegistration)>>>,
    auth_states: Arc<Mutex<HashMap<Uuid, (Instant, PasskeyAuthentication)>>>,
    qr_states: Arc<Mutex<HashMap<Uuid, QrAuthState>>>,
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
            qr_states: Arc::new(Mutex::new(HashMap::new())),
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

    pub async fn create_qr_session(&self) -> Uuid {
        let qr_id = Uuid::new_v4();
        let mut states = self.qr_states.lock().await;
        prune_expired_qr(&mut states);
        states.insert(qr_id, QrAuthState {
            created_at: Instant::now(),
            status: QrAuthStatus::Pending,
            session_admin_ts: None,
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
        if let Some(state) = states.get_mut(&qr_id) {
            if Instant::now().duration_since(state.created_at).as_secs() < STATE_TTL_SECS {
                if state.status == QrAuthStatus::Pending {
                    state.status = QrAuthStatus::Scanned;
                }
                return true;
            }
        }
        false
    }

    pub async fn approve_qr_session(&self, qr_id: Uuid, admin_ts: u64) -> bool {
        let mut states = self.qr_states.lock().await;
        if let Some(state) = states.get_mut(&qr_id) {
            if Instant::now().duration_since(state.created_at).as_secs() < STATE_TTL_SECS {
                state.status = QrAuthStatus::Approved;
                state.session_admin_ts = Some(admin_ts);
                return true;
            }
        }
        false
    }

    pub async fn consume_approved_qr_session(&self, qr_id: Uuid) -> Option<u64> {
        let mut states = self.qr_states.lock().await;
        if let Some(state) = states.get(&qr_id) {
            if state.status == QrAuthStatus::Approved {
                let ts = state.session_admin_ts;
                states.remove(&qr_id);
                return ts;
            }
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
        });

        for _ in 1..MAX_PENDING_STATES {
            map.insert(Uuid::new_v4(), QrAuthState {
                created_at: now,
                status: QrAuthStatus::Pending,
                session_admin_ts: None,
            });
        }

        assert_eq!(map.len(), MAX_PENDING_STATES);
        prune_expired_qr(&mut map);

        assert_eq!(map.len(), MAX_PENDING_STATES - 1);
        assert!(!map.contains_key(&oldest_uuid));
    }
}

