use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, SignedCookieJar};
use serde::Deserialize;

use crate::infrastructure::http::session::LANG_COOKIE;
use crate::infrastructure::http::theme;
use crate::infrastructure::http::views::auth::WEBAUTHN_JSON_SCRIPT;
use crate::infrastructure::http::AppState;

#[derive(Deserialize)]
pub struct SetLangQuery {
    pub lang: Option<String>,
    pub redirect: Option<String>,
}

pub async fn set_lang_handler(
    jar: SignedCookieJar,
    State(_state): State<AppState>,
    Query(query): Query<SetLangQuery>,
) -> (SignedCookieJar, Response) {
    let target = query.lang.unwrap_or_default();
    let normalized = match target.to_lowercase().as_str() {
        "en" | "en-us" => "en",
        _ => "pt-BR",
    };
    let mut cookie = Cookie::new(LANG_COOKIE, normalized);
    cookie.set_path("/");
    cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);
    cookie.set_max_age(time::Duration::days(365));

    let redirect_url = query
        .redirect
        .filter(|r| r.starts_with('/') && !r.starts_with("//"))
        .unwrap_or_else(|| "/".to_string());

    (jar.add(cookie), Redirect::to(&redirect_url).into_response())
}

pub async fn health_check() -> &'static str {
    "OK"
}

pub async fn webauthn_json_script_handler() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        WEBAUTHN_JSON_SCRIPT,
    )
}

pub async fn theme_css_asset_handler() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        theme::KOMAINU_THEME_CSS,
    )
}

pub const HTMX_JS: &str = include_str!("../htmx.min.js");

pub async fn htmx_script_handler() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        HTMX_JS,
    )
}


