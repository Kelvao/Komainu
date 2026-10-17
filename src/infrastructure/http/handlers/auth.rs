use axum::extract::{Json, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, SignedCookieJar};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use passkey_auth::{AuthenticationChallenge, AuthenticationResponse, RegistrationChallenge, RegistrationResponse};

use crate::infrastructure::http::session::{
    is_authenticated, resolve_i18n, QR_BIND_COOKIE, SESSION_COOKIE, SESSION_DURATION_SECS,
};
use crate::infrastructure::http::views::auth::{
    render_login_page, render_qr_expired_page, render_qr_login_page, render_register_page,
};
use crate::infrastructure::http::AppState;

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

pub async fn login_page(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if is_authenticated(&jar, &state) {
        return Redirect::to("/").into_response();
    }

    let i18n_arc = resolve_i18n(&jar, &state);
    let i18n = &*i18n_arc;
    let has_admin = state.auth_service.has_admin().await;
    let html = render_login_page(i18n, has_admin);
    Html(html).into_response()
}

#[derive(Serialize)]
pub struct LoginStartResponse {
    pub auth_id: Uuid,
    pub challenge: AuthenticationChallenge,
}

pub async fn login_start_handler(State(state): State<AppState>) -> Response {
    match state.auth_service.start_authentication().await {
        Ok((auth_id, challenge)) => Json(LoginStartResponse { auth_id, challenge }).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_localized(&state.i18n)).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum IncomingAuthenticationPayload {
    Nested {
        id: String,
        response: NestedAuthResponse,
    },
    Flat(AuthenticationResponse),
}

#[derive(Deserialize)]
pub struct NestedAuthResponse {
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    #[serde(rename = "authenticatorData")]
    pub authenticator_data: String,
    pub signature: String,
    #[serde(rename = "userHandle", default)]
    pub user_handle: Option<String>,
}

impl From<IncomingAuthenticationPayload> for AuthenticationResponse {
    fn from(p: IncomingAuthenticationPayload) -> Self {
        match p {
            IncomingAuthenticationPayload::Nested { id, response } => AuthenticationResponse {
                id,
                authenticator_data: response.authenticator_data,
                signature: response.signature,
                client_data_json: response.client_data_json,
                user_handle: response.user_handle,
            },
            IncomingAuthenticationPayload::Flat(f) => f,
        }
    }
}

#[derive(Deserialize)]
pub struct LoginFinishPayload {
    pub auth_id: Uuid,
    pub credential: IncomingAuthenticationPayload,
}

pub async fn login_finish_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Json(payload): Json<LoginFinishPayload>,
) -> Response {
    let credential = AuthenticationResponse::from(payload.credential);
    match state
        .auth_service
        .finish_authentication(payload.auth_id, &credential)
        .await
    {
        Ok(_) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let tag = state.auth_service.credential_tag();
            let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}:{tag}"));
            cookie.set_path("/");
            cookie.set_http_only(true);
            cookie.set_max_age(Some(time::Duration::seconds(SESSION_DURATION_SECS as i64)));
            cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);
            cookie.set_secure(state.rp_origin.starts_with("https://"));

            let jar = jar.add(cookie);
            (jar, state.i18n.common.authenticated_success.clone()).into_response()
        }
        Err(e) => (StatusCode::UNAUTHORIZED, e.to_localized(&state.i18n)).into_response(),
    }
}

pub async fn logout_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    state.auth_service.invalidate_sessions().await;
    let mut remove_cookie = Cookie::new(SESSION_COOKIE, "");
    remove_cookie.set_path("/");
    let jar = jar.remove(remove_cookie);
    (jar, Redirect::to("/login")).into_response()
}

#[derive(Deserialize)]
pub struct QrStartQuery {
    pub origin: Option<String>,
}

#[derive(Serialize)]
pub struct QrStartResponse {
    pub qr_id: Uuid,
    pub qr_url: String,
    pub svg: String,
}

pub async fn qr_start_handler(
    jar: SignedCookieJar,
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<QrStartQuery>,
) -> Response {
    let bind_secret = Uuid::new_v4().to_string();
    let qr_id = state.auth_service.create_qr_session(Some(bind_secret.clone())).await;

    let host_origin = {
        let proto = headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("http");
        let host = headers
            .get("x-forwarded-host")
            .or_else(|| headers.get("host"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("localhost");
        format!("{proto}://{host}")
    };

    let origin = if let Some(q_origin) = query.origin.filter(|o| !o.is_empty()) {
        if (!state.rp_origin.is_empty() && q_origin == state.rp_origin) || q_origin == host_origin {
            q_origin
        } else {
            tracing::warn!("Untrusted QR origin rejected ({q_origin}), falling back to safe origin");
            if !state.rp_origin.is_empty() {
                state.rp_origin.clone()
            } else {
                host_origin
            }
        }
    } else if !state.rp_origin.is_empty() {
        state.rp_origin.clone()
    } else {
        host_origin
    };

    let qr_url = format!("{origin}/qr-login?token={qr_id}");

    let svg = match qrcode::QrCode::new(qr_url.as_bytes()) {
        Ok(code) => code
            .render()
            .min_dimensions(150, 150)
            .quiet_zone(false)
            .dark_color(qrcode::render::svg::Color("#090d16"))
            .light_color(qrcode::render::svg::Color("#ffffff"))
            .build(),
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("QR generation error: {e}")).into_response();
        }
    };

    let mut bind_cookie = Cookie::new(QR_BIND_COOKIE, format!("{qr_id}:{bind_secret}"));
    bind_cookie.set_path("/");
    bind_cookie.set_http_only(true);
    bind_cookie.set_max_age(Some(time::Duration::seconds(300)));
    bind_cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);
    bind_cookie.set_secure(state.rp_origin.starts_with("https://"));

    let jar = jar.add(bind_cookie);
    (jar, Json(QrStartResponse { qr_id, qr_url, svg })).into_response()
}

#[derive(Deserialize)]
pub struct QrPollQuery {
    pub id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct QrPollResponse {
    pub status: &'static str,
}

pub async fn qr_poll_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<QrPollQuery>,
) -> Response {
    let qr_id = match query.id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, "Missing id").into_response(),
    };

    let bind_secret = match jar.get(QR_BIND_COOKIE).and_then(|c| {
        let val = c.value();
        let (id_str, secret) = val.split_once(':')?;
        if id_str == qr_id.to_string() {
            Some(secret.to_string())
        } else {
            None
        }
    }) {
        Some(s) => s,
        None => return (StatusCode::FORBIDDEN, "Invalid or missing QR initiator session").into_response(),
    };

    if let Some(now) = state.auth_service.consume_approved_qr_session(qr_id, Some(&bind_secret)).await {
        let tag = state.auth_service.credential_tag();
        let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}:{tag}"));
        cookie.set_path("/");
        cookie.set_http_only(true);
        cookie.set_max_age(Some(time::Duration::seconds(SESSION_DURATION_SECS as i64)));
        cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);
        cookie.set_secure(state.rp_origin.starts_with("https://"));

        let mut remove_bind = Cookie::new(QR_BIND_COOKIE, "");
        remove_bind.set_path("/");
        remove_bind.set_max_age(Some(time::Duration::seconds(0)));

        let jar = jar.remove(remove_bind).add(cookie);
        return (jar, Json(QrPollResponse { status: "approved" })).into_response();
    }

    let status = state.auth_service.get_qr_status(qr_id).await;
    let status_str = match status {
        crate::application::auth_service::QrAuthStatus::Pending => "pending",
        crate::application::auth_service::QrAuthStatus::Scanned => "scanned",
        crate::application::auth_service::QrAuthStatus::Approved => "approved",
        crate::application::auth_service::QrAuthStatus::Expired => "expired",
    };

    Json(QrPollResponse { status: status_str }).into_response()
}

pub async fn qr_login_page(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let token_str = match query.token {
        Some(t) => t,
        None => return (StatusCode::BAD_REQUEST, "Token missing").into_response(),
    };

    let qr_id = match token_str.parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid token").into_response(),
    };

    let i18n_arc = resolve_i18n(&jar, &state);
    let i18n = &*i18n_arc;

    let status = state.auth_service.get_qr_status(qr_id).await;
    if status == crate::application::auth_service::QrAuthStatus::Expired {
        let html = render_qr_expired_page(i18n);
        return Html(html).into_response();
    }

    state.auth_service.mark_qr_scanned(qr_id).await;
    let html = render_qr_login_page(i18n, &token_str);
    Html(html).into_response()
}

pub async fn qr_mobile_start_handler(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let token_str = match query.token {
        Some(t) => t,
        None => return (StatusCode::BAD_REQUEST, "Token missing").into_response(),
    };
    let qr_id = match token_str.parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid token").into_response(),
    };
    let status = state.auth_service.get_qr_status(qr_id).await;
    if status == crate::application::auth_service::QrAuthStatus::Expired {
        return (StatusCode::BAD_REQUEST, "QR session expired").into_response();
    }

    state.auth_service.mark_qr_scanned(qr_id).await;

    match state.auth_service.start_authentication().await {
        Ok((auth_id, challenge)) => Json(LoginStartResponse { auth_id, challenge }).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_localized(&state.i18n)).into_response(),
    }
}

#[derive(Deserialize)]
pub struct QrMobileFinishPayload {
    pub token: String,
    pub auth_id: Uuid,
    pub credential: IncomingAuthenticationPayload,
}

pub async fn qr_mobile_finish_handler(
    State(state): State<AppState>,
    Json(payload): Json<QrMobileFinishPayload>,
) -> Response {
    let qr_id = match payload.token.parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid token").into_response(),
    };

    let status = state.auth_service.get_qr_status(qr_id).await;
    if status == crate::application::auth_service::QrAuthStatus::Expired {
        return (StatusCode::BAD_REQUEST, "QR session expired").into_response();
    }

    let credential = AuthenticationResponse::from(payload.credential);
    match state.auth_service.finish_authentication(payload.auth_id, &credential).await {
        Ok(_) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            state.auth_service.approve_qr_session(qr_id, now).await;
            (StatusCode::OK, "Approved").into_response()
        }
        Err(e) => (StatusCode::UNAUTHORIZED, e.to_localized(&state.i18n)).into_response(),
    }
}

pub async fn register_page(
    Query(query): Query<TokenQuery>,
    State(state): State<AppState>,
) -> Response {
    let i18n = &state.i18n;
    let token = match query.token {
        Some(t) => t,
        None => return (StatusCode::FORBIDDEN, i18n.register.token_missing.clone()).into_response(),
    };

    if !state.auth_service.is_valid_reg_token(&token).await {
        return (StatusCode::FORBIDDEN, i18n.register.token_invalid.clone()).into_response();
    }

    let html = render_register_page(i18n, &token);
    Html(html).into_response()
}

#[derive(Serialize)]
pub struct RegisterStartResponse {
    pub reg_id: Uuid,
    pub challenge: RegistrationChallenge,
}

pub async fn register_start_handler(
    Query(query): Query<TokenQuery>,
    State(state): State<AppState>,
) -> Response {
    let token = match query.token {
        Some(t) => t,
        None => return (StatusCode::FORBIDDEN, state.i18n.register.token_missing.clone()).into_response(),
    };

    match state.auth_service.start_registration(&token).await {
        Ok((reg_id, challenge)) => Json(RegisterStartResponse { reg_id, challenge }).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_localized(&state.i18n)).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum IncomingRegistrationPayload {
    Nested {
        id: String,
        response: NestedRegResponse,
    },
    Flat(RegistrationResponse),
}

#[derive(Deserialize)]
pub struct NestedRegResponse {
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    #[serde(rename = "attestationObject")]
    pub attestation_object: String,
    #[serde(default)]
    pub transports: Vec<String>,
}

impl From<IncomingRegistrationPayload> for RegistrationResponse {
    fn from(p: IncomingRegistrationPayload) -> Self {
        match p {
            IncomingRegistrationPayload::Nested { id, response } => RegistrationResponse {
                id,
                transports: response.transports,
                attestation_object: response.attestation_object,
                client_data_json: response.client_data_json,
            },
            IncomingRegistrationPayload::Flat(f) => f,
        }
    }
}

#[derive(Deserialize)]
pub struct RegisterFinishPayload {
    pub reg_id: Uuid,
    pub credential: IncomingRegistrationPayload,
}

pub async fn register_finish_handler(
    Query(query): Query<TokenQuery>,
    State(state): State<AppState>,
    Json(payload): Json<RegisterFinishPayload>,
) -> Response {
    let token = match query.token {
        Some(t) => t,
        None => return (StatusCode::FORBIDDEN, state.i18n.register.token_missing.clone()).into_response(),
    };

    if !state.auth_service.is_valid_reg_token(&token).await {
        return (StatusCode::FORBIDDEN, state.i18n.register.token_invalid.clone()).into_response();
    }

    let credential = RegistrationResponse::from(payload.credential);
    match state
        .auth_service
        .finish_registration(payload.reg_id, &credential)
        .await
    {
        Ok(_) => (StatusCode::OK, state.i18n.common.passkey_registered_success.clone()).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_localized(&state.i18n)).into_response(),
    }
}

