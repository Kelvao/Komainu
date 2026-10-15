use axum::extract::Request;
use axum::middleware::{from_fn, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;

use crate::infrastructure::http::handlers::*;
use crate::infrastructure::http::rate_limit::{rate_limit_middleware, RateLimiter};
use crate::infrastructure::http::AppState;

pub async fn security_headers_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(axum::http::header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    headers.insert(axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(axum::http::header::REFERRER_POLICY, "strict-origin-when-cross-origin".parse().unwrap());
    headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self' 'unsafe-inline' https://unpkg.com; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none';"
            .parse()
            .unwrap(),
    );
    response
}

pub async fn csrf_protection_middleware(
    state: AppState,
    request: Request,
    next: Next,
) -> Response {
    if request.method() == axum::http::Method::POST {
        let expected = state.rp_origin.trim_end_matches('/');
        if !expected.is_empty() {
            if let Some(origin) = request.headers().get("origin").and_then(|v| v.to_str().ok()) {
                if origin.trim_end_matches('/') != expected {
                    tracing::warn!("CSRF rejected: Origin '{origin}' does not match expected '{expected}'");
                    return (axum::http::StatusCode::FORBIDDEN, "Cross-site request rejected: untrusted Origin").into_response();
                }
            } else if let Some(referer) = request.headers().get("referer").and_then(|v| v.to_str().ok())
                && !referer.starts_with(expected) {
                    tracing::warn!("CSRF rejected: Referer '{referer}' does not match expected '{expected}'");
                    return (axum::http::StatusCode::FORBIDDEN, "Cross-site request rejected: untrusted Referer").into_response();
                }
        }
    }
    next.run(request).await
}

pub fn create_router(state: AppState) -> Router {
    let auth_limiter = RateLimiter::new(60, std::time::Duration::from_secs(60));

    let auth_routes = Router::new()
        .route("/login_start", get(login_start_handler))
        .route("/login_finish", post(login_finish_handler))
        .route("/qr_start", get(qr_start_handler))
        .route("/qr_poll", get(qr_poll_handler))
        .route("/qr_mobile_start", get(qr_mobile_start_handler))
        .route("/qr_mobile_finish", post(qr_mobile_finish_handler))
        .route("/register_start", get(register_start_handler))
        .route("/register_finish", post(register_finish_handler))
        .layer(from_fn(move |req, next| {
            let limiter = auth_limiter.clone();
            async move { rate_limit_middleware(limiter, req, next).await }
        }));

    Router::new()
        // Rotas públicas
        .route("/health", get(health_check))
        .route("/assets/webauthn-json.js", get(webauthn_json_script_handler))
        .route("/assets/theme.css", get(theme_css_asset_handler))
        .route("/set-lang", get(set_lang_handler))
        .route("/login", get(login_page))
        .route("/logout", post(logout_handler).get(logout_handler))
        .route("/qr-login", get(qr_login_page))
        .route("/register", get(register_page))
        .nest("/api/auth", auth_routes)
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
        .layer(from_fn(security_headers_middleware))
        .layer(from_fn({
            let csrf_state = state.clone();
            move |req, next| {
                let s = csrf_state.clone();
                async move { csrf_protection_middleware(s, req, next).await }
            }
        }))
        .with_state(state)
}
