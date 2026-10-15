use axum_extra::extract::cookie::Key;
use axum_extra::extract::SignedCookieJar;
use std::sync::Arc;
use crate::infrastructure::http::AppState;

pub const SESSION_COOKIE: &str = "komainu_session";
pub const LANG_COOKIE: &str = "komainu_lang";
pub const PCLOUD_OAUTH_COOKIE: &str = "komainu_pcloud_oauth";
pub const SESSION_DURATION_SECS: u64 = 7 * 24 * 3600; // 7 days

pub fn derive_cookie_key(secret: &str) -> Key {
    use sha2::{Digest, Sha512};
    let hash = Sha512::digest(secret.as_bytes());
    Key::from(&hash)
}

pub fn is_authenticated(jar: &SignedCookieJar, state: &AppState) -> bool {
    let cookie = match jar.get(SESSION_COOKIE) {
        Some(c) => c,
        None => return false,
    };
    let val = cookie.value();
    let parts: Vec<&str> = val.split(':').collect();
    if parts.len() == 3 && parts[0] == "admin"
        && let Ok(ts) = parts[1].parse::<u64>() {
            let cookie_tag = parts[2];
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now.saturating_sub(ts) >= SESSION_DURATION_SECS {
                return false;
            }
            let current_tag = state.auth_service.credential_tag();
            return !current_tag.is_empty() && cookie_tag == current_tag;
        }
    false
}

pub fn resolve_i18n(jar: &SignedCookieJar, state: &AppState) -> Arc<crate::i18n::Translations> {
    if let Some(cookie) = jar.get(LANG_COOKIE) {
        return crate::i18n::I18n::get(cookie.value());
    }
    state.i18n.clone()
}
