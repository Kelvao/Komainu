use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::SignedCookieJar;

use crate::infrastructure::http::session::{is_authenticated, resolve_i18n};
use crate::infrastructure::http::views::dashboard::{render_dashboard_fragment, render_dashboard_page};
use crate::infrastructure::http::AppState;

pub async fn dashboard_page(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar, &state) {
        return Redirect::to("/login").into_response();
    }

    let i18n_arc = resolve_i18n(&jar, &state);
    let i18n = &*i18n_arc;
    let fragment = render_dashboard_fragment(&state, i18n, None).await;
    let html = render_dashboard_page(i18n, &fragment);
    Html(html).into_response()
}

pub async fn dashboard_fragment_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar, &state) {
        let mut headers = HeaderMap::new();
        headers.insert("HX-Redirect", "/login".parse().unwrap());
        return (StatusCode::UNAUTHORIZED, headers, i18n.common.session_expired.clone()).into_response();
    }
    Html(render_dashboard_fragment(&state, &i18n, None).await).into_response()
}

