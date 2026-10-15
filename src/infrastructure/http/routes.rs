use axum::routing::{get, post};
use axum::Router;

use crate::infrastructure::http::handlers::*;
use crate::infrastructure::http::AppState;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Rotas públicas
        .route("/health", get(health_check))
        .route("/assets/webauthn-json.js", get(webauthn_json_script_handler))
        .route("/assets/theme.css", get(theme_css_asset_handler))
        .route("/set-lang", get(set_lang_handler))
        .route("/login", get(login_page))
        .route("/logout", post(logout_handler).get(logout_handler))
        .route("/api/auth/login_start", get(login_start_handler))
        .route("/api/auth/login_finish", post(login_finish_handler))
        .route("/api/auth/qr_start", get(qr_start_handler))
        .route("/api/auth/qr_poll", get(qr_poll_handler))
        .route("/qr-login", get(qr_login_page))
        .route("/api/auth/qr_mobile_start", get(qr_mobile_start_handler))
        .route("/api/auth/qr_mobile_finish", post(qr_mobile_finish_handler))
        .route("/register", get(register_page))
        .route("/api/auth/register_start", get(register_start_handler))
        .route("/api/auth/register_finish", post(register_finish_handler))
        // Rotas protegidas por sessão
        .route("/", get(dashboard_page))
        .route("/backups", get(backups_page))
        .route("/api/backups/config", post(backup_save_config_handler))
        .route("/api/backups/run", post(backup_run_now_handler))
        .route("/api/backups/fragment", get(backup_fragment_handler))
        .route("/api/backups/download", get(backup_download_handler))
        .route("/api/backups/pcloud/authorize", post(pcloud_oauth_authorize_post_handler).get(pcloud_oauth_authorize_get_handler))
        .route("/api/backups/pcloud/callback", get(pcloud_oauth_callback_handler))
        .route("/api/backups/pcloud/disconnect", post(pcloud_disconnect_handler))
        .route("/api/dashboard-fragment", get(dashboard_fragment_handler))
        .route("/api/container/start", post(container_start_handler))
        .route("/api/container/stop", post(container_stop_handler))
        .route("/api/container/restart", post(container_restart_handler))
        .route("/api/container/update", post(container_update_handler))
        .route("/api/container/rebuild", post(container_rebuild_handler))
        .with_state(state)
}
