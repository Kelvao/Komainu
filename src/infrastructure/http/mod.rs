use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::cookie::{Cookie, Key, SignedCookieJar};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;
use webauthn_rs::prelude::{CreationChallengeResponse, PublicKeyCredential, RegisterPublicKeyCredential, RequestChallengeResponse};

use crate::application::auth_service::AuthService;
use crate::application::container_service::ContainerService;
use crate::application::system_service::SystemService;
use crate::domain::container::ContainerState;

use axum::extract::FromRef;

const SESSION_COOKIE: &str = "komainu_session";
const SESSION_DURATION_SECS: u64 = 7 * 24 * 3600; // 7 days

const WEBAUTHN_JSON_SCRIPT: &str = r#"!function(){"use strict";var e={};function t(e){const t="==".slice(0,(4-e.length%4)%4),n=e.replace(/-/g,"+").replace(/_/g,"/")+t,r=atob(n),c=new ArrayBuffer(r.length),o=new Uint8Array(c);for(let e=0;e<r.length;e++)o[e]=r.charCodeAt(e);return c}function n(e){const t=new Uint8Array(e);let n="";for(const e of t)n+=String.fromCharCode(e);return btoa(n).replace(/\+/g,"-").replace(/\//g,"_").replace(/=/g,"")}function r(e,t,n){if("copy"===t)return n;if("convert"===t)return e(n);if(t instanceof Array)return n.map((n=>r(e,t[0],n)));if(t instanceof Object){const c={};for(const[o,i]of Object.entries(t))if(o in n)null!=n[o]?c[o]=r(e,i.schema,n[o]):c[o]=null;else if(i.required)throw new Error("Missing key: "+o);return c}}function c(e){return{required:!0,schema:e}}function o(e){return{required:!1,schema:e}}const i={type:c("copy"),id:c("convert"),transports:o("copy")},a={appid:o("copy"),appidExclude:o("copy"),credProps:o("copy")},s={appid:o("copy"),appidExclude:o("copy"),credProps:o("copy")},l={publicKey:c({rp:c("copy"),user:c({id:c("convert"),name:c("copy"),displayName:c("copy")}),challenge:c("convert"),pubKeyCredParams:c("copy"),timeout:o("copy"),excludeCredentials:o([i]),authenticatorSelection:o("copy"),attestation:o("copy"),extensions:o(a)}),signal:o("copy")},p={type:c("copy"),id:c("copy"),rawId:c("convert"),response:c({clientDataJSON:c("convert"),attestationObject:c("convert")}),clientExtensionResults:c(s)},u={mediation:o("copy"),publicKey:c({challenge:c("convert"),timeout:o("copy"),rpId:o("copy"),allowCredentials:o([i]),userVerification:o("copy"),extensions:o(a)}),signal:o("copy")},y={type:c("copy"),id:c("copy"),rawId:c("convert"),response:c({clientDataJSON:c("convert"),authenticatorData:c("convert"),signature:c("convert"),userHandle:c("convert")}),clientExtensionResults:c(s)},d={credentialCreationOptions:l,publicKeyCredentialWithAttestation:p,credentialRequestOptions:u,publicKeyCredentialWithAssertion:y};e.get=async function(e){return function(e){const t=e;return t.clientExtensionResults=e.getClientExtensionResults(),r(n,y,t)}(await navigator.credentials.get(function(e){return r(t,u,e)}(e)))},e.create=async function(e){return function(e){const t=e;return t.clientExtensionResults=e.getClientExtensionResults(),r(n,p,t)}(await navigator.credentials.create(function(e){return r(t,l,e)}(e)))},e.supported=function(){return!!(navigator.credentials&&navigator.credentials.create&&navigator.credentials.get&&window.PublicKeyCredential)},e.schema=d,globalThis.webauthnJSON=e;}();"#;

#[derive(Clone)]
pub struct AppState {
    pub container_service: Arc<ContainerService>,
    pub system_service: Arc<SystemService>,
    pub auth_service: Arc<AuthService>,
    pub cookie_key: Key,
    pub i18n: Arc<crate::i18n::Translations>,
    pub base_path: String,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}

pub fn derive_cookie_key(secret: &str) -> Key {
    use sha2::{Digest, Sha512};
    let hash = Sha512::digest(secret.as_bytes());
    Key::from(&hash)
}

pub fn create_router(state: AppState) -> Router {
    let base_path = state.base_path.clone();
    let app = Router::new()
        // Rotas públicas
        .route("/health", get(health_check))
        .route("/login", get(login_page))
        .route("/logout", post(logout_handler).get(logout_handler))
        .route("/api/auth/login_start", get(login_start_handler))
        .route("/api/auth/login_finish", post(login_finish_handler))
        .route("/register", get(register_page))
        .route("/api/auth/register_start", get(register_start_handler))
        .route("/api/auth/register_finish", post(register_finish_handler))
        // Rotas protegidas por sessão
        .route("/", get(dashboard_page))
        .route("/api/dashboard-fragment", get(dashboard_fragment_handler))
        .route("/api/container/start", post(container_start_handler))
        .route("/api/container/stop", post(container_stop_handler))
        .route("/api/container/restart", post(container_restart_handler))
        .route("/api/container/update", post(container_update_handler))
        .route("/api/container/rebuild", post(container_rebuild_handler))
        .with_state(state);

    if !base_path.is_empty() {
        Router::new().nest(&base_path, app.clone()).merge(app)
    } else {
        app
    }
}

fn is_authenticated(jar: &SignedCookieJar) -> bool {
    let cookie = match jar.get(SESSION_COOKIE) {
        Some(c) => c,
        None => return false,
    };
    let val = cookie.value();
    if val == "admin" {
        return true;
    }
    if let Some(ts_str) = val.strip_prefix("admin:") {
        if let Ok(ts) = ts_str.parse::<u64>() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            return now.saturating_sub(ts) < SESSION_DURATION_SECS;
        }
    }
    false
}

async fn health_check() -> &'static str {
    "OK"
}

// ==========================================
// PÁGINAS E AUTENTICAÇÃO WEBAUTHN
// ==========================================

async fn login_page(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let target_root = if state.base_path.is_empty() {
        "/".to_string()
    } else {
        format!("{}/", state.base_path)
    };

    if is_authenticated(&jar) {
        return Redirect::to(&target_root).into_response();
    }

    let i18n = &state.i18n;
    let has_admin = state.auth_service.has_admin().await;
    let warning_box = if !has_admin {
        format!(
            r##"<div class="mb-6 p-4 rounded-xl bg-amber-500/10 border border-amber-500/20 text-amber-400 text-sm">
                {}
            </div>"##,
            i18n.login.no_admin_warning
        )
    } else {
        String::new()
    };

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{page_title}</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>{webauthn_script}</script>
</head>
<body class="bg-slate-950 text-slate-100 min-h-screen flex items-center justify-center p-4">
    <div class="max-w-md w-full bg-slate-900 border border-slate-800 rounded-2xl p-8 shadow-2xl">
        <div class="flex items-center gap-3 mb-6">
            <div class="w-10 h-10 rounded-xl bg-blue-600 flex items-center justify-center font-black text-2xl shadow-lg shadow-blue-500/20">
                狛
            </div>
            <div>
                <h1 class="text-xl font-bold tracking-tight">{header_title}</h1>
                <p class="text-xs text-slate-400">{header_subtitle}</p>
            </div>
        </div>

        {warning_box}

        <p class="text-sm text-slate-300 mb-6">
            {description}
        </p>

        <div id="error-msg" class="hidden mb-4 p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 text-sm"></div>

        <button 
            id="login-btn"
            onclick="startLogin()"
            class="w-full py-3 px-4 rounded-xl text-sm font-semibold transition shadow-lg flex items-center justify-center gap-2 bg-blue-600 hover:bg-blue-500 active:scale-95 text-white">
            <svg class="w-5 h-5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/></svg>
            {btn_text}
        </button>
    </div>

    <script>
    async function startLogin() {{
        const btn = document.getElementById('login-btn');
        const errDiv = document.getElementById('error-msg');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = '{waiting_passkey}';

        try {{
            const startRes = await fetch('{base_path}/api/auth/login_start');
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || '{start_error}');
            }}
            const data = await startRes.json();
            const credential = await webauthnJSON.get({{ publicKey: data.challenge }});

            const finishRes = await fetch('{base_path}/api/auth/login_finish', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ auth_id: data.auth_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || '{auth_error}');
            }}

            window.location.href = '{target_root}';
        }} catch (err) {{
            console.error(err);
            errDiv.innerText = err.message || '{default_error}';
            errDiv.classList.remove('hidden');
            btn.disabled = false;
            btn.innerText = '{btn_text}';
        }}
    }}
    </script>
</body>
</html>"##,
        lang = i18n.lang,
        page_title = i18n.login.page_title,
        header_title = i18n.login.header_title,
        header_subtitle = i18n.login.header_subtitle,
        warning_box = warning_box,
        description = i18n.login.description,
        btn_text = i18n.login.button,
        waiting_passkey = i18n.login.waiting_passkey,
        start_error = i18n.login.start_error,
        auth_error = i18n.login.auth_error,
        default_error = i18n.login.default_error,
        base_path = state.base_path,
        target_root = target_root,
        webauthn_script = WEBAUTHN_JSON_SCRIPT,
    );

    Html(html).into_response()
}

#[derive(Serialize)]
struct LoginStartResponse {
    auth_id: Uuid,
    challenge: RequestChallengeResponse,
}

async fn login_start_handler(State(state): State<AppState>) -> Response {
    match state.auth_service.start_authentication().await {
        Ok((auth_id, challenge)) => Json(LoginStartResponse { auth_id, challenge }).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_localized(&state.i18n)).into_response(),
    }
}

#[derive(Deserialize)]
struct LoginFinishPayload {
    auth_id: Uuid,
    credential: PublicKeyCredential,
}

async fn login_finish_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Json(payload): Json<LoginFinishPayload>,
) -> Response {
    match state
        .auth_service
        .finish_authentication(payload.auth_id, &payload.credential)
        .await
    {
        Ok(_) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}"));
            cookie.set_path("/");
            cookie.set_http_only(true);
            cookie.set_max_age(Some(time::Duration::seconds(SESSION_DURATION_SECS as i64)));
            cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);

            let jar = jar.add(cookie);
            (jar, state.i18n.common.authenticated_success.clone()).into_response()
        }
        Err(e) => (StatusCode::UNAUTHORIZED, e.to_localized(&state.i18n)).into_response(),
    }
}

async fn logout_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let mut remove_cookie = Cookie::new(SESSION_COOKIE, "");
    remove_cookie.set_path("/");
    let jar = jar.remove(remove_cookie);
    let target = format!("{}/login", state.base_path);
    (jar, Redirect::to(&target)).into_response()
}

// ==========================================
// CADASTRO DE PASSKEY VIA LINK TEMPORÁRIO (CLI)
// ==========================================

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

async fn register_page(
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

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{page_title}</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>{webauthn_script}</script>
</head>
<body class="bg-slate-950 text-slate-100 min-h-screen flex items-center justify-center p-4">
    <div class="max-w-md w-full bg-slate-900 border border-slate-800 rounded-2xl p-8 shadow-2xl">
        <div class="flex items-center gap-3 mb-6">
            <div class="w-10 h-10 rounded-xl bg-emerald-600 flex items-center justify-center font-black text-2xl shadow-lg shadow-emerald-500/20">
                狛
            </div>
            <div>
                <h1 class="text-xl font-bold tracking-tight">{header_title}</h1>
                <p class="text-xs text-slate-400">{header_subtitle}</p>
            </div>
        </div>

        <p class="text-sm text-slate-300 mb-6">
            {description}
        </p>

        <div id="error-msg" class="hidden mb-4 p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 text-sm"></div>

        <button 
            id="reg-btn"
            onclick="startRegistration('{token}')"
            class="w-full py-3 px-4 rounded-xl text-sm font-semibold transition shadow-lg flex items-center justify-center gap-2 bg-emerald-600 hover:bg-emerald-500 active:scale-95 text-white">
            <svg class="w-5 h-5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
            {btn_text}
        </button>
    </div>

    <script>
    async function startRegistration(token) {{
        const btn = document.getElementById('reg-btn');
        const errDiv = document.getElementById('error-msg');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = '{creating_credential}';

        try {{
            const startRes = await fetch(`{base_path}/api/auth/register_start?token=${{encodeURIComponent(token)}}`);
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || '{start_error}');
            }}
            const data = await startRes.json();
            const credential = await webauthnJSON.create({{ publicKey: data.challenge }});

            const finishRes = await fetch(`{base_path}/api/auth/register_finish?token=${{encodeURIComponent(token)}}`, {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ reg_id: data.reg_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || '{save_error}');
            }}

            alert('{success_alert}');
            window.location.href = '{base_path}/login';
        }} catch (err) {{
            console.error(err);
            errDiv.innerText = err.message || '{default_error}';
            errDiv.classList.remove('hidden');
            btn.disabled = false;
            btn.innerText = '{btn_text}';
        }}
    }}
    </script>
</body>
</html>"##,
        lang = i18n.lang,
        page_title = i18n.register.page_title,
        header_title = i18n.register.header_title,
        header_subtitle = i18n.register.header_subtitle,
        description = i18n.register.description,
        btn_text = i18n.register.button,
        token = token,
        creating_credential = i18n.register.creating_credential,
        start_error = i18n.register.start_error,
        save_error = i18n.register.save_error,
        success_alert = i18n.register.success_alert,
        default_error = i18n.register.default_error,
        base_path = state.base_path,
        webauthn_script = WEBAUTHN_JSON_SCRIPT,
    );

    Html(html).into_response()
}

#[derive(Serialize)]
struct RegisterStartResponse {
    reg_id: Uuid,
    challenge: CreationChallengeResponse,
}

async fn register_start_handler(
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
struct RegisterFinishPayload {
    reg_id: Uuid,
    credential: RegisterPublicKeyCredential,
}

async fn register_finish_handler(
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

    match state
        .auth_service
        .finish_registration(payload.reg_id, &payload.credential)
        .await
    {
        Ok(_) => (StatusCode::OK, state.i18n.common.passkey_registered_success.clone()).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_localized(&state.i18n)).into_response(),
    }
}

// ==========================================
// ROTAS DO DASHBOARD (PROTEGIDAS POR SESSÃO)
// ==========================================

async fn dashboard_page(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        let login_url = format!("{}/login", state.base_path);
        return Redirect::to(&login_url).into_response();
    }

    let i18n = &state.i18n;
    let fragment = render_dashboard_fragment(&state, None).await;
    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{page_title}</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script src="https://unpkg.com/htmx.org@2.0.4"></script>
    <script>
      tailwind.config = {{
        darkMode: 'class',
        theme: {{
          extend: {{
            colors: {{
              brand: '#3b82f6',
            }}
          }}
        }}
      }}
    </script>
</head>
<body class="bg-slate-950 text-slate-100 min-h-screen flex flex-col font-sans antialiased selection:bg-blue-600 selection:text-white">
    <header class="border-b border-slate-800 bg-slate-900/50 backdrop-blur sticky top-0 z-50">
        <div class="max-w-6xl mx-auto px-4 py-4 flex items-center justify-between">
            <div class="flex items-center gap-3">
                <div class="w-9 h-9 rounded-lg bg-blue-600 flex items-center justify-center font-black text-xl shadow-lg shadow-blue-500/20">
                    狛
                </div>
                <div>
                    <h1 class="text-lg font-bold tracking-tight">{app_title}</h1>
                    <p class="text-xs text-slate-400">{app_subtitle}</p>
                </div>
            </div>
            <div class="flex items-center gap-3">
                <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                    <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                    {active_monitoring}
                </span>
                <a href="{base_path}/logout" class="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 transition">
                    {logout}
                </a>
            </div>
        </div>
    </header>

    <main class="flex-1 max-w-6xl w-full mx-auto px-4 py-8">
        <div id="dashboard-content" hx-get="{base_path}/api/dashboard-fragment" hx-trigger="every 5s" hx-swap="innerHTML">
            {fragment}
        </div>
    </main>

    <footer class="border-t border-slate-800/80 py-4 text-center text-xs text-slate-500">
        {footer}
    </footer>
</body>
</html>"##,
        lang = i18n.lang,
        page_title = i18n.dashboard.page_title,
        app_title = i18n.common.app_title,
        app_subtitle = i18n.common.app_subtitle,
        active_monitoring = i18n.common.active_monitoring,
        logout = i18n.common.logout,
        fragment = fragment,
        footer = i18n.common.footer,
        base_path = state.base_path,
    );
    Html(html).into_response()
}

async fn dashboard_fragment_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        let mut headers = HeaderMap::new();
        let login_url = format!("{}/login", state.base_path);
        headers.insert("HX-Redirect", login_url.parse().unwrap());
        return (StatusCode::UNAUTHORIZED, headers, state.i18n.common.session_expired.clone()).into_response();
    }
    Html(render_dashboard_fragment(&state, None).await).into_response()
}

async fn container_start_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, state.i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.start().await.err().map(|e| e.to_localized(&state.i18n));
    Html(render_dashboard_fragment(&state, err).await).into_response()
}

async fn container_stop_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, state.i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.stop().await.err().map(|e| e.to_localized(&state.i18n));
    Html(render_dashboard_fragment(&state, err).await).into_response()
}

async fn container_restart_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, state.i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.restart().await.err().map(|e| e.to_localized(&state.i18n));
    Html(render_dashboard_fragment(&state, err).await).into_response()
}

async fn container_update_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, state.i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.update_image().await.err().map(|e| e.to_localized(&state.i18n));
    Html(render_dashboard_fragment(&state, err).await).into_response()
}

async fn container_rebuild_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, state.i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.rebuild().await.err().map(|e| e.to_localized(&state.i18n));
    Html(render_dashboard_fragment(&state, err).await).into_response()
}

async fn render_dashboard_fragment(state: &AppState, action_error: Option<String>) -> String {
    let i18n = &state.i18n;
    let status_res = state.container_service.status().await;
    let metrics = state.system_service.get_metrics().await;

    let error_banner = match action_error {
        Some(e) => format!(
            r##"<div id="action-error-banner" class="lg:col-span-3 p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-400 text-sm flex items-center justify-between gap-3 shadow-lg">
                <div class="flex items-center gap-2">
                    <svg class="w-5 h-5 fill-current shrink-0" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/></svg>
                    <span><strong>{}</strong> {}</span>
                </div>
                <button type="button" onclick="document.getElementById('action-error-banner').remove()" class="text-rose-400/70 hover:text-rose-300 transition p-1 rounded-lg hover:bg-rose-500/20" title="Dismiss">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
            </div>"##,
            i18n.common.operation_failed,
            e
        ),
        None => String::new(),
    };

    let (status_badge, image_name, uptime_str, container_cpu_str, container_ram_str) = match &status_res {
        Ok(status) => {
            let badge = match status.state {
                ContainerState::Running => {
                    format!(
                        r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 flex items-center gap-1.5">
                        <span class="w-2 h-2 rounded-full bg-emerald-400"></span> {}
                    </span>"##,
                        i18n.dashboard.status_running
                    )
                }
                ContainerState::Stopped => {
                    format!(
                        r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-rose-500/20 text-rose-400 border border-rose-500/30 flex items-center gap-1.5">
                        <span class="w-2 h-2 rounded-full bg-rose-400"></span> {}
                    </span>"##,
                        i18n.dashboard.status_stopped
                    )
                }
                ContainerState::Restarting => {
                    format!(
                        r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-amber-500/20 text-amber-400 border border-amber-500/30 flex items-center gap-1.5">
                        <span class="w-2 h-2 rounded-full bg-amber-400 animate-spin"></span> {}
                    </span>"##,
                        i18n.dashboard.status_restarting
                    )
                }
                _ => {
                    format!(
                        r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-slate-700 text-slate-300">
                        {}
                    </span>"##,
                        i18n.dashboard.status_unknown
                    )
                }
            };
            (
                badge,
                status.image.clone(),
                status.uptime.clone().unwrap_or_else(|| "N/A".to_string()),
                status.cpu_usage_pct.map(|c| format!("{c:.1}%")).unwrap_or_else(|| "N/A".to_string()),
                status.memory_used_mb.map(|m| format!("{m} MB")).unwrap_or_else(|| "N/A".to_string()),
            )
        }
        Err(e) => (
            format!(
                r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-rose-500/20 text-rose-400 border border-rose-500/30">
                    {} {}
                </span>"##,
                i18n.common.operation_failed,
                e
            ),
            "N/A".to_string(),
            "N/A".to_string(),
            "N/A".to_string(),
            "N/A".to_string(),
        ),
    };

    let is_running = matches!(status_res.as_ref().map(|s| &s.state), Ok(ContainerState::Running));

    format!(
        r##"<div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
    {error_banner}
    <!-- Card do Container -->
    <div class="lg:col-span-3 bg-slate-900/60 border border-slate-800 rounded-2xl p-6 shadow-xl backdrop-blur">
        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-slate-800">
            <div>
                <div class="flex items-center gap-3">
                    <h2 class="text-xl font-bold tracking-tight">{instance_title}</h2>
                    {status_badge}
                </div>
                <p class="text-sm text-slate-400 mt-1 font-mono text-xs">{image_prefix} {image_name} &bull; {started_prefix} {uptime_str}</p>
            </div>
            
            <div class="flex flex-wrap items-center gap-2">
                <button 
                    hx-post="{base_path}/api/container/start" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    {disabled_start}
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-emerald-600 hover:bg-emerald-500 active:scale-95 disabled:opacity-40 disabled:cursor-not-allowed text-white">
                    <svg class="w-3.5 h-3.5 fill-current" viewBox="0 0 20 20"><path d="M6.3 2.841A1.5 1.5 0 004 4.11v11.78a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z"/></svg>
                    {btn_start}
                </button>
                <button 
                    hx-post="{base_path}/api/container/stop" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    {disabled_stop}
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-rose-600 hover:bg-rose-500 active:scale-95 disabled:opacity-40 disabled:cursor-not-allowed text-white">
                    <svg class="w-3.5 h-3.5 fill-current" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M4.5 4.5A1.5 1.5 0 016 3h8a1.5 1.5 0 011.5 1.5v11a1.5 1.5 0 01-1.5 1.5H6a1.5 1.5 0 01-1.5-1.5v-11z" clip-rule="evenodd"/></svg>
                    {btn_stop}
                </button>
                <button 
                    hx-post="{base_path}/api/container/restart" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-amber-600 hover:bg-amber-500 active:scale-95 text-white">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
                    {btn_restart}
                </button>
                <button 
                    hx-post="{base_path}/api/container/update" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-sky-600 hover:bg-sky-500 active:scale-95 text-white">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/></svg>
                    {btn_update}
                </button>
                <button 
                    hx-post="{base_path}/api/container/rebuild" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    hx-confirm="{confirm_rebuild}"
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-indigo-600 hover:bg-indigo-500 active:scale-95 text-white">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/></svg>
                    {btn_rebuild}
                </button>
            </div>
        </div>
    </div>

    <!-- Métrica CPU -->
    <div class="bg-slate-900/60 border border-slate-800 rounded-2xl p-6 shadow-xl backdrop-blur flex flex-col justify-between">
        <div>
            <div class="flex items-center justify-between mb-4">
                <span class="text-sm font-medium text-slate-400">{cpu_title}</span>
                <span class="text-xs font-mono font-semibold px-2 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/20">{cpu_usage:.1}%</span>
            </div>
            <div class="w-full bg-slate-800 h-2.5 rounded-full overflow-hidden">
                <div class="bg-blue-500 h-2.5 rounded-full transition-all duration-500" style="width: {cpu_usage:.1}%"></div>
            </div>
        </div>
        <div class="mt-4 pt-3 border-t border-slate-800/60 flex items-center justify-between text-xs">
            <span class="text-slate-500">{container_label}</span>
            <span class="font-mono font-semibold text-blue-400">{container_cpu_str}</span>
        </div>
    </div>

    <!-- Métrica RAM -->
    <div class="bg-slate-900/60 border border-slate-800 rounded-2xl p-6 shadow-xl backdrop-blur flex flex-col justify-between">
        <div>
            <div class="flex items-center justify-between mb-4">
                <span class="text-sm font-medium text-slate-400">{ram_title}</span>
                <span class="text-xs font-mono font-semibold px-2 py-0.5 rounded bg-purple-500/10 text-purple-400 border border-purple-500/20">{ram_used} / {ram_total} MB</span>
            </div>
            <div class="w-full bg-slate-800 h-2.5 rounded-full overflow-hidden">
                <div class="bg-purple-500 h-2.5 rounded-full transition-all duration-500" style="width: {ram_pct:.1}%"></div>
            </div>
        </div>
        <div class="mt-4 pt-3 border-t border-slate-800/60 flex items-center justify-between text-xs">
            <span class="text-slate-500">{container_label}</span>
            <span class="font-mono font-semibold text-purple-400">{container_ram_str}</span>
        </div>
    </div>

    <!-- Métrica Disco -->
    <div class="bg-slate-900/60 border border-slate-800 rounded-2xl p-6 shadow-xl backdrop-blur flex flex-col justify-between">
        <div>
            <div class="flex items-center justify-between mb-4">
                <span class="text-sm font-medium text-slate-400">{disk_title}</span>
                <span class="text-xs font-mono font-semibold px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">{disk_used} / {disk_total} GB</span>
            </div>
            <div class="w-full bg-slate-800 h-2.5 rounded-full overflow-hidden">
                <div class="bg-emerald-500 h-2.5 rounded-full transition-all duration-500" style="width: {disk_pct:.1}%"></div>
            </div>
        </div>
        <div class="mt-4 pt-3 border-t border-slate-800/60 flex items-center justify-between text-xs">
            <span class="text-slate-500">{host_label}</span>
            <span class="font-mono font-semibold text-emerald-400">{disk_pct:.1}% {occupied_suffix}</span>
        </div>
    </div>
</div>"##,
        base_path = &state.base_path,
        error_banner = error_banner,
        instance_title = i18n.dashboard.instance_title,
        status_badge = status_badge,
        image_prefix = i18n.dashboard.image_prefix,
        image_name = image_name,
        started_prefix = i18n.dashboard.started_prefix,
        uptime_str = uptime_str,
        disabled_start = if is_running { "disabled" } else { "" },
        disabled_stop = if !is_running { "disabled" } else { "" },
        btn_start = i18n.dashboard.btn_start,
        btn_stop = i18n.dashboard.btn_stop,
        btn_restart = i18n.dashboard.btn_restart,
        btn_update = i18n.dashboard.btn_update,
        btn_rebuild = i18n.dashboard.btn_rebuild,
        confirm_rebuild = i18n.dashboard.confirm_rebuild,
        cpu_title = i18n.metrics.cpu_title,
        cpu_usage = metrics.cpu_usage_pct,
        container_label = i18n.metrics.container_label,
        container_cpu_str = container_cpu_str,
        ram_title = i18n.metrics.ram_title,
        ram_used = metrics.ram_used_mb,
        ram_total = metrics.ram_total_mb,
        ram_pct = if metrics.ram_total_mb > 0 { (metrics.ram_used_mb as f32 / metrics.ram_total_mb as f32) * 100.0 } else { 0.0 },
        container_ram_str = container_ram_str,
        disk_title = i18n.metrics.disk_title,
        disk_used = metrics.disk_used_gb,
        disk_total = metrics.disk_total_gb,
        disk_pct = if metrics.disk_total_gb > 0 { (metrics.disk_used_gb as f32 / metrics.disk_total_gb as f32) * 100.0 } else { 0.0 },
        host_label = i18n.metrics.host_label,
        occupied_suffix = i18n.metrics.occupied_suffix,
    )
}

pub async fn run_server(host: &str, port: u16, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router(state);
    let addr_str = format!("{host}:{port}");
    let addr: SocketAddr = addr_str
        .parse()
        .map_err(|e| format!("Invalid bind address '{addr_str}': {e}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Komainu rodando em http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::auth::{AuthError, CredentialStorePort};
    use crate::domain::container::{BoxFuture, ContainerError, ContainerPort, ContainerStatus};
    use crate::domain::system::{SystemMetrics, SystemMonitorPort};
    use webauthn_rs::prelude::Passkey;

    struct MockContainerPort;
    impl ContainerPort for MockContainerPort {
        fn get_status(&self, name: &str) -> BoxFuture<'_, Result<ContainerStatus, ContainerError>> {
            let name = name.to_string();
            Box::pin(async move {
                Ok(ContainerStatus {
                    name,
                    state: ContainerState::Running,
                    image: "vaultwarden:test".to_string(),
                    uptime: Some("Up 5 hours".to_string()),
                    cpu_usage_pct: Some(0.8),
                    memory_used_mb: Some(32),
                })
            })
        }
        fn start(&self, _name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
            Box::pin(async { Ok(()) })
        }
        fn stop(&self, _name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
            Box::pin(async { Ok(()) })
        }
        fn restart(&self, _name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
            Box::pin(async { Ok(()) })
        }
        fn update_image(&self, _name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
            Box::pin(async { Ok(()) })
        }
        fn rebuild(&self, _name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
            Box::pin(async { Ok(()) })
        }
    }

    struct MockSystemMonitor;
    impl SystemMonitorPort for MockSystemMonitor {
        fn get_metrics(&self) -> BoxFuture<'_, SystemMetrics> {
            Box::pin(async {
                SystemMetrics {
                    cpu_usage_pct: 12.5,
                    ram_used_mb: 1024,
                    ram_total_mb: 4096,
                    disk_used_gb: 20,
                    disk_total_gb: 100,
                }
            })
        }
    }

    struct MockCredentialStore;
    impl CredentialStorePort for MockCredentialStore {
        fn load_credentials(&self) -> BoxFuture<'_, Result<Vec<Passkey>, AuthError>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn save_credential(&self, _passkey: Passkey) -> BoxFuture<'_, Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
        fn has_registered_admin(&self) -> BoxFuture<'_, bool> {
            Box::pin(async { false })
        }
    }

    #[tokio::test]
    async fn test_health_check_handler() {
        assert_eq!(health_check().await, "OK");
    }

    #[tokio::test]
    async fn test_dashboard_fragment_generation() {
        let container_service = Arc::new(ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string()));
        let system_service = Arc::new(SystemService::new(Arc::new(MockSystemMonitor)));
        let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
        let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
        let i18n = Arc::new(crate::i18n::I18n::load("pt-BR"));
        let state = AppState {
            container_service: container_service.clone(),
            system_service: system_service.clone(),
            auth_service: auth_service.clone(),
            cookie_key: cookie_key.clone(),
            i18n: i18n.clone(),
            base_path: String::new(),
        };

        let fragment = render_dashboard_fragment(&state, None).await;
        assert!(fragment.contains("Instância Vaultwarden"));
        assert!(fragment.contains("Uso de CPU"));
        assert!(fragment.contains("Memória RAM"));
        assert!(fragment.contains("Armazenamento"));
        assert!(fragment.contains("vaultwarden:test"));
        assert!(fragment.contains("hx-post=\"/api/container/start\""));

        let fragment_err = render_dashboard_fragment(&state, Some("Falha ao iniciar".to_string())).await;
        assert!(fragment_err.contains("Falha na operação:"));
        assert!(fragment_err.contains("Falha ao iniciar"));

        // Test base_path prefixing
        let subpath_state = AppState {
            container_service: container_service.clone(),
            system_service: system_service.clone(),
            auth_service: auth_service.clone(),
            cookie_key: cookie_key.clone(),
            i18n: i18n.clone(),
            base_path: "/komainu".to_string(),
        };
        let subpath_fragment = render_dashboard_fragment(&subpath_state, None).await;
        assert!(subpath_fragment.contains("hx-post=\"/komainu/api/container/start\""));
        assert!(subpath_fragment.contains("hx-post=\"/komainu/api/container/stop\""));

        // Test English fragment
        let en_state = AppState {
            container_service,
            system_service,
            auth_service,
            cookie_key,
            i18n: Arc::new(crate::i18n::I18n::load("en")),
            base_path: String::new(),
        };
        let en_fragment = render_dashboard_fragment(&en_state, None).await;
        assert!(en_fragment.contains("Vaultwarden Instance"));
        assert!(en_fragment.contains("CPU Usage"));
        assert!(en_fragment.contains("RAM Memory"));
        assert!(en_fragment.contains("Storage"));
    }

    #[tokio::test]
    async fn test_nested_base_path_routing() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let container_service = Arc::new(ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string()));
        let system_service = Arc::new(SystemService::new(Arc::new(MockSystemMonitor)));
        let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
        let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
        let i18n = Arc::new(crate::i18n::I18n::load("en"));
        let state = AppState {
            container_service,
            system_service,
            auth_service,
            cookie_key,
            i18n,
            base_path: "/komainu".to_string(),
        };

        let router = create_router(state);

        // 1. Nested path /komainu/health
        let req_nested = Request::builder()
            .uri("/komainu/health")
            .body(Body::empty())
            .unwrap();
        let res_nested = router.clone().oneshot(req_nested).await.unwrap();
        assert_eq!(res_nested.status(), StatusCode::OK);

        // 2. Direct root path /health (when reverse proxy strips prefix)
        let req_root = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let res_root = router.oneshot(req_root).await.unwrap();
        assert_eq!(res_root.status(), StatusCode::OK);
    }
}
