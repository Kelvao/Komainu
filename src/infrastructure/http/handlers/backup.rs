use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, SignedCookieJar};
use serde::Deserialize;
use uuid::Uuid;

use crate::infrastructure::http::session::{is_authenticated, resolve_i18n, PCLOUD_OAUTH_COOKIE};
use crate::infrastructure::http::views::backups::{render_backups_fragment, render_backups_page};
use crate::infrastructure::http::AppState;

#[derive(Deserialize, Default)]
pub struct BackupsPageQuery {
    pub pcloud_connected: Option<String>,
    pub pcloud_error: Option<String>,
}

pub async fn backups_page(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<BackupsPageQuery>,
) -> Response {
    if !is_authenticated(&jar, &state) {
        return Redirect::to("/login").into_response();
    }

    let i18n_arc = resolve_i18n(&jar, &state);
    let i18n = &*i18n_arc;

    let banner_msg = if query.pcloud_connected.as_deref() == Some("1") {
        Some((true, i18n.backups.pcloud_auth_success.clone()))
    } else if let Some(err) = query.pcloud_error.as_deref() {
        let msg = match err {
            "missing_client_id" => "Client ID da aplicação pCloud não foi configurado.",
            "missing_code" => "Código de autorização OAuth ausente.",
            "missing_app_credentials" => "Client ID ou Client Secret ausentes para troca de token.",
            "invalid_oauth_state" => "Estado da requisição OAuth inválido ou expirado (possível CSRF).",
            _ => err,
        };
        Some((false, format!("{}: {msg}", i18n.backups.pcloud_auth_failed)))
    } else {
        None
    };

    let fragment = render_backups_fragment(&state, i18n, banner_msg).await;
    let html = render_backups_page(i18n, &fragment);
    Html(html).into_response()
}

#[derive(Deserialize)]
pub struct BackupConfigForm {
    pub enabled: Option<String>,
    pub cron_expression: Option<String>,
    pub encryption_password: Option<String>,
    pub pcloud_region: Option<String>,
    pub pcloud_client_id: Option<String>,
    pub pcloud_client_secret: Option<String>,
    pub pcloud_folder: Option<String>,
}

pub async fn backup_save_config_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Form(form): Form<BackupConfigForm>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar, &state) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }

    let existing = state.backup_service.get_config().await.unwrap_or_default();

    let enabled = form.enabled.as_deref() == Some("on");
    let cron_expression = form.cron_expression.unwrap_or_else(|| "0 3 * * *".to_string()).trim().to_string();
    let submitted_pw = form.encryption_password.unwrap_or_default().trim().to_string();
    let encryption_password = if submitted_pw.is_empty() {
        existing.encryption_password
    } else {
        submitted_pw
    };
    let pcloud_region = match form.pcloud_region.as_deref() {
        Some("US") => crate::domain::backup::PCloudRegion::US,
        _ => crate::domain::backup::PCloudRegion::EU,
    };
    let submitted_cid = form.pcloud_client_id.unwrap_or_default().trim().to_string();
    let pcloud_client_id = if submitted_cid.is_empty() {
        existing.pcloud_client_id
    } else {
        submitted_cid
    };
    let submitted_csec = form.pcloud_client_secret.unwrap_or_default().trim().to_string();
    let pcloud_client_secret = if submitted_csec.is_empty() {
        existing.pcloud_client_secret
    } else {
        submitted_csec
    };
    let pcloud_token = existing.pcloud_token;
    let pcloud_folder = form.pcloud_folder.unwrap_or_else(|| "0".to_string()).trim().to_string();

    let new_cfg = crate::domain::backup::BackupConfig {
        enabled,
        cron_expression,
        encryption_password,
        pcloud_region,
        pcloud_token,
        pcloud_folder,
        pcloud_client_id,
        pcloud_client_secret,
    };

    let result = state.backup_service.save_config(&new_cfg).await;
    let msg = match result {
        Ok(_) => Some((true, i18n.backups.config_saved_success.clone())),
        Err(e) => Some((false, format!("{}: {e}", i18n.common.operation_failed))),
    };

    Html(render_backups_fragment(&state, &i18n, msg).await).into_response()
}

#[derive(Deserialize, Default)]
pub struct PCloudAuthorizeForm {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

pub async fn pcloud_oauth_authorize_post_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Form(form): Form<PCloudAuthorizeForm>,
) -> Response {
    pcloud_oauth_authorize_internal(jar, state, form.client_id, form.client_secret).await
}

pub async fn pcloud_oauth_authorize_get_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
) -> Response {
    pcloud_oauth_authorize_internal(jar, state, None, None).await
}

async fn pcloud_oauth_authorize_internal(
    jar: SignedCookieJar,
    state: AppState,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Response {
    if !is_authenticated(&jar, &state) {
        return Redirect::to("/login").into_response();
    }

    let mut cfg = state.backup_service.get_config().await.unwrap_or_default();

    let mut changed = false;
    if let Some(cid) = client_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        cfg.pcloud_client_id = cid;
        changed = true;
    }
    if let Some(csec) = client_secret.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        cfg.pcloud_client_secret = csec;
        changed = true;
    }
    if changed {
        let _ = state.backup_service.save_config(&cfg).await;
    }

    let effective_client_id = if !cfg.pcloud_client_id.is_empty() {
        &cfg.pcloud_client_id
    } else {
        &state.pcloud_client_id
    };

    if effective_client_id.is_empty() {
        return (jar, Redirect::to("/backups?pcloud_error=missing_client_id")).into_response();
    }

    let redirect_uri = format!("{}/api/backups/pcloud/callback", state.rp_origin.trim_end_matches('/'));
    let state_token = Uuid::new_v4().to_string();

    let mut oauth_cookie = Cookie::new(PCLOUD_OAUTH_COOKIE, state_token.clone());
    oauth_cookie.set_http_only(true);
    oauth_cookie.set_secure(state.rp_origin.starts_with("https://"));
    oauth_cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);
    oauth_cookie.set_path("/");
    oauth_cookie.set_max_age(time::Duration::minutes(10));
    let jar = jar.add(oauth_cookie);

    let auth_url = format!(
        "https://my.pcloud.com/oauth2/authorize?client_id={}&response_type=code&redirect_uri={}&state={}",
        crate::infrastructure::pcloud::urlencoding(effective_client_id),
        crate::infrastructure::pcloud::urlencoding(&redirect_uri),
        crate::infrastructure::pcloud::urlencoding(&state_token),
    );

    (jar, Redirect::to(&auth_url)).into_response()
}

#[derive(Deserialize, Default)]
pub struct PCloudCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub hostname: Option<String>,
    pub locationid: Option<u32>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

pub async fn pcloud_oauth_callback_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<PCloudCallbackQuery>,
) -> Response {
    if !is_authenticated(&jar, &state) {
        return Redirect::to("/login").into_response();
    }

    let saved_state = jar.get(PCLOUD_OAUTH_COOKIE).map(|c| c.value().to_string());
    let mut remove_cookie = Cookie::new(PCLOUD_OAUTH_COOKIE, "");
    remove_cookie.set_path("/");
    remove_cookie.set_max_age(time::Duration::seconds(0));
    let jar = jar.add(remove_cookie);

    if let Some(err) = query.error.or(query.error_description) {
        return (
            jar,
            Redirect::to(&format!(
                "/backups?pcloud_error={}",
                crate::infrastructure::pcloud::urlencoding(&err)
            )),
        )
            .into_response();
    }

    match (query.state.as_deref(), saved_state.as_deref()) {
        (Some(qs), Some(ss)) if qs == ss => {}
        _ => {
            return (jar, Redirect::to("/backups?pcloud_error=invalid_oauth_state")).into_response();
        }
    }

    let code = match query.code {
        Some(c) if !c.is_empty() => c,
        _ => return (jar, Redirect::to("/backups?pcloud_error=missing_code")).into_response(),
    };

    let mut cfg = state.backup_service.get_config().await.unwrap_or_default();
    let client_id = if !cfg.pcloud_client_id.is_empty() {
        cfg.pcloud_client_id.clone()
    } else {
        state.pcloud_client_id.clone()
    };
    let client_secret = if !cfg.pcloud_client_secret.is_empty() {
        cfg.pcloud_client_secret.clone()
    } else {
        state.pcloud_client_secret.clone()
    };

    if client_id.is_empty() || client_secret.is_empty() {
        return (jar, Redirect::to("/backups?pcloud_error=missing_app_credentials")).into_response();
    }

    let hostname = if let Some(h) = query.hostname.filter(|h| !h.is_empty()) {
        h
    } else if query.locationid == Some(1) {
        "api.pcloud.com".to_string()
    } else {
        "eapi.pcloud.com".to_string()
    };

    let region = if hostname.contains("api.pcloud.com") && !hostname.contains("eapi") && query.locationid != Some(2) {
        crate::domain::backup::PCloudRegion::US
    } else {
        crate::domain::backup::PCloudRegion::EU
    };

    let token_endpoint = format!("https://{hostname}/oauth2_token");
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let res = http_client
        .post(&token_endpoint)
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
        ])
        .send()
        .await;

    match res {
        Ok(response) => {
            #[derive(Deserialize)]
            struct TokenResponse {
                result: Option<i32>,
                access_token: Option<String>,
                error: Option<String>,
            }

            if let Ok(json) = response.json::<TokenResponse>().await {
                if let (Some(0), Some(access_token)) = (json.result, json.access_token) {
                    cfg.pcloud_token = access_token;
                    cfg.pcloud_region = region;
                    let _ = state.backup_service.save_config(&cfg).await;
                    (jar, Redirect::to("/backups?pcloud_connected=1")).into_response()
                } else {
                    let err = json.error.unwrap_or_else(|| format!("result {:?}", json.result));
                    (
                        jar,
                        Redirect::to(&format!(
                            "/backups?pcloud_error={}",
                            crate::infrastructure::pcloud::urlencoding(&err)
                        )),
                    )
                        .into_response()
                }
            } else {
                (jar, Redirect::to("/backups?pcloud_error=invalid_token_response")).into_response()
            }
        }
        Err(e) => (
            jar,
            Redirect::to(&format!(
                "/backups?pcloud_error={}",
                crate::infrastructure::pcloud::urlencoding(&e.to_string())
            )),
        )
            .into_response(),
    }
}

pub async fn pcloud_disconnect_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar, &state) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }

    let mut cfg = state.backup_service.get_config().await.unwrap_or_default();
    cfg.pcloud_token = String::new();
    let _ = state.backup_service.save_config(&cfg).await;

    let msg = Some((true, i18n.backups.pcloud_disconnected_success.clone()));
    Html(render_backups_fragment(&state, &i18n, msg).await).into_response()
}

pub async fn backup_run_now_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar, &state) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }

    let result = state.backup_service.execute_backup().await;
    let msg = match result {
        Ok(rec) => Some((true, format!("{} ({})", i18n.backups.backup_started_success, rec.filename))),
        Err(e) => Some((false, format!("{}: {e}", i18n.common.operation_failed))),
    };

    Html(render_backups_fragment(&state, &i18n, msg).await).into_response()
}

pub async fn backup_fragment_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar, &state) {
        let mut headers = HeaderMap::new();
        headers.insert("HX-Redirect", "/login".parse().unwrap());
        return (StatusCode::UNAUTHORIZED, headers, i18n.common.session_expired.clone()).into_response();
    }

    Html(render_backups_fragment(&state, &i18n, None).await).into_response()
}

#[derive(Deserialize)]
pub struct BackupDownloadQuery {
    pub fileid: u64,
    pub filename: Option<String>,
}

pub async fn backup_download_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<BackupDownloadQuery>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar, &state) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }

    if query.fileid == 0 {
        return (StatusCode::BAD_REQUEST, i18n.common.operation_failed.clone()).into_response();
    }

    let temp_dec_path = crate::application::backup_service::komainu_temp_dir().join(format!("komainu_dl_{}_{}.zip", query.fileid, Uuid::new_v4()));
    let _guard = crate::domain::backup::TempFileGuard(temp_dec_path.clone());

    if let Err(e) = state
        .backup_service
        .download_and_decrypt_backup(query.fileid, &temp_dec_path)
        .await
    {
        tracing::error!("Download and decrypt failed for fileid {}: {e}", query.fileid);
        return (
            StatusCode::BAD_REQUEST,
            format!("{}: {e}", i18n.common.operation_failed),
        )
            .into_response();
    }

    let file_bytes = match tokio::fs::read(&temp_dec_path).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Failed to read decrypted file: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("{}: {e}", i18n.common.operation_failed),
            )
                .into_response();
        }
    };

    let raw_name = query.filename.unwrap_or_else(|| format!("backup_{}.zip", query.fileid));
    let mut clean_name = raw_name
        .trim_end_matches(".enc")
        .to_string();
    if !clean_name.ends_with(".zip") {
        clean_name.push_str(".zip");
    }
    let mut safe_name: String = clean_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' { c } else { '_' })
        .collect();

    if safe_name == ".zip" || safe_name.starts_with('.') {
        safe_name = format!("backup_{}.zip", query.fileid);
    }

    let content_disposition = format!("attachment; filename=\"{}\"", safe_name);

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/zip"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                content_disposition.as_str(),
            ),
        ],
        file_bytes,
    )
        .into_response()
}
