use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum_extra::extract::cookie::SignedCookieJar;

use crate::infrastructure::http::session::{is_authenticated, resolve_i18n};
use crate::infrastructure::http::views::dashboard::render_dashboard_fragment;
use crate::infrastructure::http::AppState;

async fn handle_container_action<F, Fut>(
    jar: SignedCookieJar,
    state: AppState,
    action: F,
) -> Response
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), crate::domain::container::ContainerError>>,
{
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar, &state) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }
    let err = action().await.err().map(|e| e.to_localized(&i18n));
    Html(render_dashboard_fragment(&state, &i18n, err).await).into_response()
}

pub async fn container_start_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    handle_container_action(jar, state.clone(), || async move {
        state.container_port.start(&state.target_container).await
    })
    .await
}

pub async fn container_stop_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    handle_container_action(jar, state.clone(), || async move {
        state.container_port.stop(&state.target_container).await
    })
    .await
}

pub async fn container_restart_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    handle_container_action(jar, state.clone(), || async move {
        state.container_port.restart(&state.target_container).await
    })
    .await
}

pub async fn container_update_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    handle_container_action(jar, state.clone(), || async move {
        state.container_port.update_image(&state.target_container).await
    })
    .await
}

pub async fn container_rebuild_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    handle_container_action(jar, state.clone(), || async move {
        state.container_port.rebuild(&state.target_container).await
    })
    .await
}

