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
use std::str::FromStr;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;
use webauthn_rs::prelude::{CreationChallengeResponse, PublicKeyCredential, RegisterPublicKeyCredential, RequestChallengeResponse};

pub mod theme;

use crate::application::auth_service::AuthService;
use crate::application::container_service::ContainerService;
use crate::application::system_service::SystemService;
use crate::domain::container::ContainerState;

use axum::extract::FromRef;

const SESSION_COOKIE: &str = "komainu_session";
const LANG_COOKIE: &str = "komainu_lang";
const SESSION_DURATION_SECS: u64 = 7 * 24 * 3600; // 7 days

const WEBAUTHN_JSON_SCRIPT: &str = r#"!function(){"use strict";var e={};function t(e){const t="==".slice(0,(4-e.length%4)%4),n=e.replace(/-/g,"+").replace(/_/g,"/")+t,r=atob(n),c=new ArrayBuffer(r.length),o=new Uint8Array(c);for(let e=0;e<r.length;e++)o[e]=r.charCodeAt(e);return c}function n(e){const t=new Uint8Array(e);let n="";for(const e of t)n+=String.fromCharCode(e);return btoa(n).replace(/\+/g,"-").replace(/\//g,"_").replace(/=/g,"")}function r(e,t,n){if("copy"===t)return n;if("convert"===t)return e(n);if(t instanceof Array)return n.map((n=>r(e,t[0],n)));if(t instanceof Object){const c={};for(const[o,i]of Object.entries(t))if(o in n)null!=n[o]?c[o]=r(e,i.schema,n[o]):c[o]=null;else if(i.required)throw new Error("Missing key: "+o);return c}}function c(e){return{required:!0,schema:e}}function o(e){return{required:!1,schema:e}}const i={type:c("copy"),id:c("convert"),transports:o("copy")},a={appid:o("copy"),appidExclude:o("copy"),credProps:o("copy")},s={appid:o("copy"),appidExclude:o("copy"),credProps:o("copy")},l={publicKey:c({rp:c("copy"),user:c({id:c("convert"),name:c("copy"),displayName:c("copy")}),challenge:c("convert"),pubKeyCredParams:c("copy"),timeout:o("copy"),excludeCredentials:o([i]),authenticatorSelection:o("copy"),attestation:o("copy"),extensions:o(a)}),signal:o("copy")},p={type:c("copy"),id:c("copy"),rawId:c("convert"),response:c({clientDataJSON:c("convert"),attestationObject:c("convert")}),clientExtensionResults:c(s)},u={mediation:o("copy"),publicKey:c({challenge:c("convert"),timeout:o("copy"),rpId:o("copy"),allowCredentials:o([i]),userVerification:o("copy"),extensions:o(a)}),signal:o("copy")},y={type:c("copy"),id:c("copy"),rawId:c("convert"),response:c({clientDataJSON:c("convert"),authenticatorData:c("convert"),signature:c("convert"),userHandle:c("convert")}),clientExtensionResults:c(s)},d={credentialCreationOptions:l,publicKeyCredentialWithAttestation:p,credentialRequestOptions:u,publicKeyCredentialWithAssertion:y};e.get=async function(e){return function(e){const t=e;return t.clientExtensionResults=e.getClientExtensionResults(),r(n,y,t)}(await navigator.credentials.get(function(e){return r(t,u,e)}(e)))},e.create=async function(e){return function(e){const t=e;return t.clientExtensionResults=e.getClientExtensionResults(),r(n,p,t)}(await navigator.credentials.create(function(e){return r(t,l,e)}(e)))},e.supported=function(){return!!(navigator.credentials&&navigator.credentials.create&&navigator.credentials.get&&window.PublicKeyCredential)},e.schema=d,globalThis.webauthnJSON=e;}();"#;

#[derive(Clone)]
pub struct AppState {
    pub container_service: Arc<ContainerService>,
    pub system_service: Arc<SystemService>,
    pub auth_service: Arc<AuthService>,
    pub backup_service: Arc<crate::application::backup_service::BackupService>,
    pub cookie_key: Key,
    pub i18n: Arc<crate::i18n::Translations>,
    pub rp_origin: String,
    pub pcloud_client_id: String,
    pub pcloud_client_secret: String,
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
    Router::new()
        // Rotas públicas
        .route("/health", get(health_check))
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
        .route("/api/backups/pcloud/authorize", get(pcloud_oauth_authorize_handler))
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

fn is_authenticated(jar: &SignedCookieJar) -> bool {
    let cookie = match jar.get(SESSION_COOKIE) {
        Some(c) => c,
        None => return false,
    };
    let val = cookie.value();
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

#[derive(Deserialize)]
struct SetLangQuery {
    lang: Option<String>,
    redirect: Option<String>,
}

async fn set_lang_handler(
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

fn resolve_i18n(jar: &SignedCookieJar, state: &AppState) -> std::sync::Arc<crate::i18n::Translations> {
    if let Some(cookie) = jar.get(LANG_COOKIE) {
        let val = cookie.value();
        if val == "en" {
            if state.i18n.lang == "en" {
                return state.i18n.clone();
            } else {
                return std::sync::Arc::new(crate::i18n::I18n::load("en"));
            }
        } else if val == "pt" || val == "pt-BR" || val == "pt_BR" {
            if state.i18n.lang == "pt-BR" {
                return state.i18n.clone();
            } else {
                return std::sync::Arc::new(crate::i18n::I18n::load("pt-BR"));
            }
        }
    }
    state.i18n.clone()
}

async fn health_check() -> &'static str {
    "OK"
}

// ==========================================
// PÁGINAS E AUTENTICAÇÃO WEBAUTHN
// ==========================================

async fn login_page(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if is_authenticated(&jar) {
        return Redirect::to("/").into_response();
    }

    let i18n_arc = resolve_i18n(&jar, &state);
    let i18n = &*i18n_arc;
    let (next_lang, lang_btn_label, lang_title) = if i18n.lang == "en" {
        ("pt-BR", "PT", "Mudar para Português")
    } else {
        ("en", "EN", "Switch to English")
    };

    let has_admin = state.auth_service.has_admin().await;
    let warning_box = if !has_admin {
        format!(
            r##"<div class="mb-6 p-4 rounded-lg bg-amber-500/10 border border-amber-500/30 text-amber-300 text-xs font-mono flex items-start gap-3">
                <span class="px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-400 font-bold shrink-0">WARN</span>
                <span class="leading-relaxed">{}</span>
            </div>"##,
            i18n.login.no_admin_warning
        )
    } else {
        String::new()
    };

    let head_tags = theme::render_head(
        &i18n.login.page_title,
        &format!("<script>{WEBAUTHN_JSON_SCRIPT}</script>"),
    );

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex items-center justify-center p-3 sm:p-6 antialiased font-sans selection:bg-rose-600 selection:text-white">
    <div class="w-full max-w-md sm:max-w-xl mx-auto my-auto py-2 sm:py-4">
        <!-- Interactive Auth Card -->
        <div class="w-full bg-surface border border-rose-500/20 rounded-2xl p-4 sm:p-6 shadow-2xl relative overflow-hidden">
            <!-- Top accent rule -->
            <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/60 to-transparent"></div>

            <!-- Top Header: Brand & Language -->
            <div class="flex items-center justify-between gap-3 mb-4 pb-3 border-b border-white/[0.06]">
                <div class="flex items-center gap-2.5">
                    <div class="w-7 h-7 sm:w-8 sm:h-8 rounded-lg bg-rose-600/10 border border-rose-500/30 flex items-center justify-center font-display font-bold text-base sm:text-lg text-rose-400 shrink-0">
                        狛
                    </div>
                    <div>
                        <span class="font-display font-bold text-sm sm:text-base text-white tracking-tight leading-none block">Komainu</span>
                        <span class="text-[10px] text-slate-400 font-mono">Gateway</span>
                    </div>
                </div>
                <a href="/set-lang?lang={next_lang}&redirect=/login" class="px-2 sm:px-2.5 py-1 rounded-md text-xs font-mono font-medium text-slate-300 hover:text-white bg-white/[0.04] hover:bg-white/[0.08] transition-colors duration-150 flex items-center gap-1.5 border border-white/[0.08] hover:border-rose-500/40" title="{lang_title}">
                    <svg class="w-3.5 h-3.5 text-slate-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0112 16.5c-3.162 0-6.133-.815-8.716-2.247m0 0A9.015 9.015 0 013 12c0-.778.099-1.533.284-2.253"/></svg>
                    <span>{lang_btn_label}</span>
                </a>
            </div>

            <div class="mb-4">
                <h1 class="text-lg sm:text-xl font-display font-bold tracking-tight text-white">{header_title}</h1>
                <p class="text-xs text-slate-400 mt-0.5 font-sans">{header_subtitle}</p>
            </div>

            {warning_box}

            <!-- Segmented Tab Switcher -->
            <div class="grid grid-cols-2 gap-1 p-1 bg-canvas rounded-lg border border-white/[0.08] mb-4">
                <button type="button" id="tab-btn-local" onclick="switchLoginTab('local')" class="py-2 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 text-white bg-rose-500/20 shadow-sm flex items-center justify-center gap-1.5 border border-rose-500/35">
                    <svg class="w-3.5 h-3.5 text-rose-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/></svg>
                    <span>{tab_local}</span>
                </button>
                <button type="button" id="tab-btn-qr" onclick="switchLoginTab('qr')" class="py-2 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 text-slate-400 hover:text-white flex items-center justify-center gap-1.5 border border-transparent">
                    <svg class="w-3.5 h-3.5 text-slate-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 4v1m6 11h2m-6 0h-2v4m0-11v3m0 0h.01M12 12h4.01M16 20h4M4 12h4m12 0h.01M5 8h2a1 1 0 001-1V5a1 1 0 00-1-1H5a1 1 0 00-1 1v2a1 1 0 001 1zm12 0h2a1 1 0 001-1V5a1 1 0 00-1-1h-2a1 1 0 00-1 1v2a1 1 0 001 1zM5 20h2a1 1 0 001-1v-2a1 1 0 00-1-1H5a1 1 0 00-1 1v2a1 1 0 001 1z"/></svg>
                    <span>{tab_qr}</span>
                </button>
            </div>

            <div id="error-msg" class="hidden mb-3 p-3 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono"></div>

            <!-- View 1: Local Passkey -->
            <div id="view-local" class="space-y-4">
                <p class="text-xs sm:text-sm text-slate-300/90 leading-relaxed font-sans mb-4">
                    {description}
                </p>

                <button 
                    id="login-btn"
                    onclick="startLogin()"
                    class="w-full py-3 px-4 rounded-lg text-xs sm:text-sm font-semibold transition-colors duration-150 shadow-sm flex items-center justify-center gap-2.5 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-40 disabled:cursor-wait text-white border border-rose-400/30">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/></svg>
                    <span>{btn_text}</span>
                </button>
            </div>

            <!-- View 2: QR Code Mobile Key -->
            <div id="view-qr" class="hidden">
                <div class="flex flex-col sm:flex-row items-center sm:items-stretch gap-4 sm:gap-5">
                    <!-- QR Box with scanning brackets and slim crisp border -->
                    <div class="relative p-1.5 bg-white rounded-xl shadow-xl inline-block shrink-0">
                        <div id="qr-container" class="w-[140px] h-[140px] sm:w-[150px] sm:h-[150px] flex items-center justify-center overflow-hidden [&>svg]:w-full [&>svg]:h-full">
                            <div class="w-6 h-6 border-2 border-rose-600 border-t-transparent rounded-full animate-spin"></div>
                        </div>
                        <div class="absolute -top-1 -left-1 w-3 h-3 border-t-2 border-l-2 border-rose-500"></div>
                        <div class="absolute -top-1 -right-1 w-3 h-3 border-t-2 border-r-2 border-rose-500"></div>
                        <div class="absolute -bottom-1 -left-1 w-3 h-3 border-b-2 border-l-2 border-rose-500"></div>
                        <div class="absolute -bottom-1 -right-1 w-3 h-3 border-b-2 border-r-2 border-rose-500"></div>
                    </div>

                    <!-- Instructions & Live Status Bar -->
                    <div class="flex-1 flex flex-col justify-between space-y-2.5 text-center sm:text-left min-w-0">
                        <p class="text-xs text-slate-300 font-sans leading-relaxed">
                            {qr_desc}
                        </p>

                        <div class="space-y-2">
                            <!-- Live Status Bar -->
                            <div id="qr-status-box" class="w-full py-2 px-3 rounded-lg bg-canvas border border-white/[0.08] flex items-center justify-center sm:justify-start gap-2 font-mono text-[11px] sm:text-xs text-slate-300">
                                <span id="qr-status-dot" class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse shrink-0"></span>
                                <span id="qr-status-text" class="truncate">{qr_waiting}</span>
                            </div>

                            <!-- Reload Button -->
                            <button type="button" onclick="loadQrSession()" class="w-full text-xs font-mono text-slate-400 hover:text-white transition flex items-center justify-center gap-1.5 py-1.5 px-3 rounded-lg bg-white/[0.04] hover:bg-white/[0.08] border border-white/[0.06] hover:border-rose-500/40">
                                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
                                <span>{qr_reload}</span>
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <script>
    let currentQrId = null;
    let qrPollTimer = null;

    function switchLoginTab(tab) {{
        const btnLocal = document.getElementById('tab-btn-local');
        const btnQr = document.getElementById('tab-btn-qr');
        const viewLocal = document.getElementById('view-local');
        const viewQr = document.getElementById('view-qr');
        const errDiv = document.getElementById('error-msg');
        errDiv.classList.add('hidden');

        if (tab === 'local') {{
            btnLocal.className = "py-2 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 text-white bg-rose-500/20 shadow-sm flex items-center justify-center gap-1.5 border border-rose-500/35";
            btnQr.className = "py-2 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 text-slate-400 hover:text-white flex items-center justify-center gap-1.5 border border-transparent";
            viewLocal.classList.remove('hidden');
            viewQr.classList.add('hidden');
            stopQrPolling();
        }} else {{
            btnQr.className = "py-2 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 text-white bg-rose-500/20 shadow-sm flex items-center justify-center gap-1.5 border border-rose-500/35";
            btnLocal.className = "py-2 px-3 rounded-md text-xs font-mono font-medium transition-colors duration-150 text-slate-400 hover:text-white flex items-center justify-center gap-1.5 border border-transparent";
            viewQr.classList.remove('hidden');
            viewLocal.classList.add('hidden');
            if (!currentQrId) {{
                loadQrSession();
            }} else {{
                startQrPolling();
            }}
        }}
    }}

    async function loadQrSession() {{
        stopQrPolling();
        const qrContainer = document.getElementById('qr-container');
        const statusText = document.getElementById('qr-status-text');
        const statusDot = document.getElementById('qr-status-dot');
        qrContainer.innerHTML = '<div class="w-6 h-6 border-2 border-rose-600 border-t-transparent rounded-full animate-spin"></div>';
        statusDot.className = "w-2 h-2 rounded-full bg-emerald-400 animate-pulse";
        statusText.innerText = '{qr_waiting}';

        try {{
            const origin = window.location.origin;
            const res = await fetch(`/api/auth/qr_start?origin=${{encodeURIComponent(origin)}}`);
            if (!res.ok) throw new Error("Falha ao gerar QR Code");
            const data = await res.json();
            currentQrId = data.qr_id;
            qrContainer.innerHTML = data.svg;
            startQrPolling();
        }} catch (e) {{
            qrContainer.innerHTML = '<span class="text-xs text-rose-500 font-mono">Erro ao carregar</span>';
        }}
    }}

    function stopQrPolling() {{
        if (qrPollTimer) {{
            clearInterval(qrPollTimer);
            qrPollTimer = null;
        }}
    }}

    function startQrPolling() {{
        stopQrPolling();
        if (!currentQrId) return;

        qrPollTimer = setInterval(async () => {{
            try {{
                const res = await fetch(`/api/auth/qr_poll?id=${{currentQrId}}`);
                if (!res.ok) return;
                const data = await res.json();
                const statusText = document.getElementById('qr-status-text');
                const statusDot = document.getElementById('qr-status-dot');

                if (data.status === 'approved') {{
                    stopQrPolling();
                    statusDot.className = "w-2 h-2 rounded-full bg-emerald-400";
                    statusText.innerText = '{qr_approved}';
                    setTimeout(() => {{
                        window.location.href = '/';
                    }}, 400);
                }} else if (data.status === 'scanned') {{
                    statusDot.className = "w-2 h-2 rounded-full bg-cyan-400 animate-ping";
                    statusText.innerText = '{qr_scanned}';
                }} else if (data.status === 'expired') {{
                    stopQrPolling();
                    statusDot.className = "w-2 h-2 rounded-full bg-amber-400";
                    statusText.innerText = '{qr_expired}';
                }}
            }} catch (e) {{
                console.error("Poll error", e);
            }}
        }}, 1500);
    }}

    async function startLogin() {{
        const btn = document.getElementById('login-btn');
        const errDiv = document.getElementById('error-msg');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = '{waiting_passkey}';

        try {{
            const startRes = await fetch('/api/auth/login_start');
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || '{start_error}');
            }}
            const data = await startRes.json();
            const opts = data.challenge.publicKey ? data.challenge : {{ publicKey: data.challenge }};
            if (opts.publicKey) {{
                opts.publicKey.userVerification = "preferred";
                delete opts.publicKey.extensions;
                if (opts.publicKey.allowCredentials) {{
                    for (const cred of opts.publicKey.allowCredentials) {{
                        delete cred.transports;
                    }}
                }}
            }}
            const credential = await webauthnJSON.get(opts);

            const finishRes = await fetch('/api/auth/login_finish', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ auth_id: data.auth_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || '{auth_error}');
            }}

            window.location.href = '/';
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
        head_tags = head_tags,
        header_title = i18n.login.header_title,
        header_subtitle = i18n.login.header_subtitle,
        warning_box = warning_box,
        description = i18n.login.description,
        btn_text = i18n.login.button,
        waiting_passkey = i18n.login.waiting_passkey,
        start_error = i18n.login.start_error,
        auth_error = i18n.login.auth_error,
        default_error = i18n.login.default_error,
        tab_local = i18n.login.tab_local,
        tab_qr = i18n.login.tab_qr,
        qr_desc = i18n.login.qr_desc,
        qr_waiting = i18n.login.qr_waiting,
        qr_scanned = i18n.login.qr_scanned,
        qr_approved = i18n.login.qr_approved,
        qr_expired = i18n.login.qr_expired,
        qr_reload = i18n.login.qr_reload,
        next_lang = next_lang,
        lang_btn_label = lang_btn_label,
        lang_title = lang_title,
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

async fn logout_handler(jar: SignedCookieJar, State(_state): State<AppState>) -> Response {
    let mut remove_cookie = Cookie::new(SESSION_COOKIE, "");
    remove_cookie.set_path("/");
    let jar = jar.remove(remove_cookie);
    (jar, Redirect::to("/login")).into_response()
}

// ==========================================
// AUTENTICAÇÃO VIA QR CODE (SMARTPHONE COMO CHAVE)
// ==========================================

#[derive(Deserialize)]
struct QrStartQuery {
    origin: Option<String>,
}

#[derive(Serialize)]
struct QrStartResponse {
    qr_id: Uuid,
    qr_url: String,
    svg: String,
}

async fn qr_start_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<QrStartQuery>,
) -> Response {
    let qr_id = state.auth_service.create_qr_session().await;

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

    Json(QrStartResponse { qr_id, qr_url, svg }).into_response()
}

#[derive(Deserialize)]
struct QrPollQuery {
    id: Option<Uuid>,
}

#[derive(Serialize)]
struct QrPollResponse {
    status: &'static str,
}

async fn qr_poll_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<QrPollQuery>,
) -> Response {
    let qr_id = match query.id {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, "Missing id").into_response(),
    };

    if let Some(now) = state.auth_service.consume_approved_qr_session(qr_id).await {
        let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}"));
        cookie.set_path("/");
        cookie.set_http_only(true);
        cookie.set_max_age(Some(time::Duration::seconds(SESSION_DURATION_SECS as i64)));
        cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);

        let jar = jar.add(cookie);
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

async fn qr_login_page(
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
        let head_tags = theme::render_head(&i18n.login.mobile_title, "");
        let html = format!(
            r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex items-center justify-center p-4 antialiased font-sans">
    <div class="max-w-md w-full bg-surface border border-rose-500/20 rounded-xl p-6 sm:p-8 shadow-2xl relative overflow-hidden text-center">
        <div class="w-12 h-12 rounded-full bg-amber-500/10 border border-amber-500/30 flex items-center justify-center text-amber-400 text-2xl mx-auto mb-4">
            ⚠️
        </div>
        <h1 class="text-xl font-display font-bold text-white mb-2">{qr_expired}</h1>
        <p class="text-xs text-slate-400 font-sans leading-relaxed mb-6">
            Esta solicitação de acesso não é mais válida. Por favor, volte ao computador e recarregue o QR Code na tela de login.
        </p>
    </div>
</body>
</html>"##,
            lang = i18n.lang,
            head_tags = head_tags,
            qr_expired = i18n.login.qr_expired,
        );
        return Html(html).into_response();
    }

    state.auth_service.mark_qr_scanned(qr_id).await;

    let head_tags = theme::render_head(
        &i18n.login.mobile_title,
        &format!("<script>{WEBAUTHN_JSON_SCRIPT}</script>"),
    );

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex items-center justify-center p-4 antialiased font-sans selection:bg-rose-600 selection:text-white">
    <div class="max-w-md w-full bg-surface border border-rose-500/20 rounded-xl p-6 sm:p-8 shadow-2xl relative overflow-hidden">
        <!-- Top accent rule -->
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/60 to-transparent"></div>

        <div id="auth-box">
            <div class="flex items-center gap-3 mb-5">
                <div class="w-10 h-10 rounded-lg bg-rose-600/10 border border-rose-500/30 flex items-center justify-center font-display font-bold text-xl text-rose-400 shrink-0">
                    狛
                </div>
                <div>
                    <h1 class="text-lg sm:text-xl font-display font-bold tracking-tight text-white">{mobile_title}</h1>
                    <p class="text-xs text-slate-400 mt-0.5">{mobile_subtitle}</p>
                </div>
            </div>

            <div class="p-3.5 rounded-lg bg-canvas border border-white/[0.06] mb-6 text-xs font-mono text-slate-300 flex items-center gap-2.5">
                <svg class="w-4 h-4 text-rose-400 shrink-0" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/></svg>
                <span>Sessão de login pendente no computador</span>
            </div>

            <div id="error-msg" class="hidden mb-4 p-3 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono"></div>

            <button 
                id="approve-btn"
                onclick="startMobileApproval('{token}')"
                class="w-full py-3.5 px-4 rounded-lg text-sm font-semibold transition-colors duration-150 shadow-sm flex items-center justify-center gap-2.5 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-40 disabled:cursor-wait text-white border border-rose-400/30">
                <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"/></svg>
                <span>{mobile_btn}</span>
            </button>
        </div>

        <div id="success-box" class="hidden text-center py-4">
            <div class="w-16 h-16 rounded-full bg-emerald-500/10 border border-emerald-500/30 flex items-center justify-center text-emerald-400 text-3xl mx-auto mb-4">
                ✓
            </div>
            <h2 class="text-lg font-display font-bold text-white mb-2">{mobile_success_title}</h2>
            <p class="text-xs text-slate-300 font-sans leading-relaxed mb-6">{mobile_success_desc}</p>
            <div class="py-2 px-3 rounded-lg bg-canvas border border-emerald-500/30 text-[11px] font-mono text-emerald-400 inline-block">
                SESSÃO DESBLOQUEADA NO COMPUTADOR
            </div>
        </div>
    </div>

    <script>
    async function startMobileApproval(token) {{
        const btn = document.getElementById('approve-btn');
        const errDiv = document.getElementById('error-msg');
        const authBox = document.getElementById('auth-box');
        const successBox = document.getElementById('success-box');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = '{waiting_passkey}';

        try {{
            const startRes = await fetch(`/api/auth/qr_mobile_start?token=${{encodeURIComponent(token)}}`);
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || '{start_error}');
            }}
            const data = await startRes.json();
            const opts = data.challenge.publicKey ? data.challenge : {{ publicKey: data.challenge }};
            if (opts.publicKey) {{
                opts.publicKey.userVerification = "preferred";
                delete opts.publicKey.extensions;
                if (opts.publicKey.allowCredentials) {{
                    for (const cred of opts.publicKey.allowCredentials) {{
                        delete cred.transports;
                    }}
                }}
            }}
            const credential = await webauthnJSON.get(opts);

            const finishRes = await fetch('/api/auth/qr_mobile_finish', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ token, auth_id: data.auth_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || '{auth_error}');
            }}

            authBox.classList.add('hidden');
            successBox.classList.remove('hidden');
        }} catch (err) {{
            console.error(err);
            errDiv.innerText = err.message || '{default_error}';
            errDiv.classList.remove('hidden');
            btn.disabled = false;
            btn.innerText = '{mobile_btn}';
        }}
    }}
    </script>
</body>
</html>"##,
        lang = i18n.lang,
        head_tags = head_tags,
        mobile_title = i18n.login.mobile_title,
        mobile_subtitle = i18n.login.mobile_subtitle,
        mobile_btn = i18n.login.mobile_btn,
        mobile_success_title = i18n.login.mobile_success_title,
        mobile_success_desc = i18n.login.mobile_success_desc,
        waiting_passkey = i18n.login.waiting_passkey,
        start_error = i18n.login.start_error,
        auth_error = i18n.login.auth_error,
        default_error = i18n.login.default_error,
        token = token_str,
    );

    Html(html).into_response()
}

async fn qr_mobile_start_handler(
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
struct QrMobileFinishPayload {
    token: String,
    auth_id: Uuid,
    credential: PublicKeyCredential,
}

async fn qr_mobile_finish_handler(
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

    match state.auth_service.finish_authentication(payload.auth_id, &payload.credential).await {
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

    let head_tags = theme::render_head(
        &i18n.register.page_title,
        &format!("<script>{WEBAUTHN_JSON_SCRIPT}</script>"),
    );

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex items-center justify-center p-4 antialiased font-sans selection:bg-rose-600 selection:text-white">
    <div class="max-w-md w-full bg-surface border border-rose-500/20 rounded-xl p-6 sm:p-8 shadow-2xl relative overflow-hidden">
        <!-- Top accent rule -->
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/60 to-transparent"></div>

        <div class="flex items-center gap-2.5 mb-5 pb-4 border-b border-white/[0.06]">
            <div class="w-8 h-8 rounded-lg bg-rose-600/10 border border-rose-500/30 flex items-center justify-center font-display font-bold text-lg text-rose-400 shrink-0">
                狛
            </div>
            <div>
                <span class="font-display font-bold text-base text-white tracking-tight leading-none block">Komainu</span>
                <span class="text-[10px] text-rose-400 font-mono">Setup de Acesso</span>
            </div>
        </div>

        <div class="mb-5">
            <h1 class="text-xl font-display font-bold tracking-tight text-white">{header_title}</h1>
            <p class="text-xs text-slate-400 mt-0.5">{header_subtitle}</p>
        </div>

        <p class="text-sm text-slate-300/90 leading-relaxed mb-6 font-sans">
            {description}
        </p>

        <div id="error-msg" class="hidden mb-4 p-3 rounded-lg bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono"></div>

        <button 
            id="reg-btn"
            onclick="startRegistration('{token}')"
            class="w-full py-3.5 px-4 rounded-lg text-sm font-semibold transition-colors duration-150 shadow-sm flex items-center justify-center gap-2.5 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-40 disabled:cursor-wait text-white border border-rose-400/30">
            <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
            <span>{btn_text}</span>
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
            const startRes = await fetch(`/api/auth/register_start?token=${{encodeURIComponent(token)}}`);
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || '{start_error}');
            }}
            const data = await startRes.json();
            const opts = data.challenge.publicKey ? data.challenge : {{ publicKey: data.challenge }};
            if (opts.publicKey) {{
                delete opts.publicKey.extensions;
                opts.publicKey.authenticatorSelection = {{
                    residentKey: "preferred",
                    userVerification: "required"
                }};
            }}
            const credential = await webauthnJSON.create(opts);

            const finishRes = await fetch(`/api/auth/register_finish?token=${{encodeURIComponent(token)}}`, {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ reg_id: data.reg_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || '{save_error}');
            }}

            alert('{success_alert}');
            window.location.href = '/login';
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
        head_tags = head_tags,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveNav {
    Home,
    Backups,
}

fn render_navbar(
    i18n: &crate::i18n::Translations,
    active: ActiveNav,
    current_path: &str,
) -> String {
    let (next_lang, lang_btn_label, lang_title) = if i18n.lang == "en" {
        ("pt-BR", "PT", "Mudar para Português")
    } else {
        ("en", "EN", "Switch to English")
    };

    let home_url = "/";
    let backups_url = "/backups";

    let home_classes = if active == ActiveNav::Home {
        "bg-rose-500/15 text-rose-300 border border-rose-500/30 shadow-sm"
    } else {
        "text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent"
    };

    let backups_classes = if active == ActiveNav::Backups {
        "bg-rose-500/15 text-rose-300 border border-rose-500/30 shadow-sm"
    } else {
        "text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent"
    };

    let lang_redirect = current_path;

    format!(
        r##"<header class="border-b border-rose-500/15 bg-canvas/90 backdrop-blur-md sticky top-0 z-50">
        <div class="max-w-6xl mx-auto px-4 py-2.5 sm:py-3 flex items-center justify-between gap-3">
            <div class="flex items-center gap-3 sm:gap-6">
                <a href="{home_url}" class="flex items-center gap-2.5 sm:gap-3 group">
                    <div class="w-8 h-8 sm:w-9 sm:h-9 rounded-lg bg-rose-600/10 border border-rose-500/30 flex items-center justify-center font-display font-bold text-lg sm:text-xl text-rose-400 shrink-0 group-hover:border-rose-500/50 transition-colors">
                        狛
                    </div>
                    <div>
                        <div class="flex items-center gap-2">
                            <h1 class="text-sm sm:text-base font-display font-bold tracking-tight text-white">{app_title}</h1>
                            <span class="px-1.5 py-0.2 rounded text-[10px] font-mono uppercase bg-white/[0.05] text-slate-400 border border-white/[0.08]">v0.2</span>
                        </div>
                        <p class="hidden sm:block text-[11px] text-slate-400 font-normal">{app_subtitle}</p>
                    </div>
                </a>

                <!-- Navigation Tabs -->
                <nav class="flex items-center gap-1 sm:gap-1.5 p-1 bg-surface-elevated/80 rounded-lg border border-white/[0.06]">
                    <a href="{home_url}" class="px-2.5 sm:px-3 py-1 rounded-md text-xs font-mono font-medium transition-colors duration-150 {home_classes}">
                        {nav_home}
                    </a>
                    <a href="{backups_url}" class="px-2.5 sm:px-3 py-1 rounded-md text-xs font-mono font-medium transition-colors duration-150 {backups_classes}">
                        {nav_backups}
                    </a>
                </nav>
            </div>

            <div class="flex items-center gap-2 sm:gap-3 shrink-0">
                <span class="inline-flex items-center gap-1.5 px-2 sm:px-2.5 py-1 rounded-md text-xs font-mono font-medium bg-rose-500/10 text-rose-300 border border-rose-500/25">
                    <span class="w-1.5 h-1.5 rounded-full bg-rose-400 animate-pulse"></span>
                    <span class="hidden sm:inline">{active_monitoring}</span>
                </span>

                <a href="/set-lang?lang={next_lang}&redirect={lang_redirect}" class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold text-slate-300 hover:text-white bg-white/[0.04] hover:bg-white/[0.08] transition-colors duration-150 flex items-center gap-1.5 border border-white/[0.08] hover:border-rose-500/40" title="{lang_title}">
                    <svg class="w-3.5 h-3.5 text-slate-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A17.919 17.919 0 0112 16.5c-3.162 0-6.133-.815-8.716-2.247m0 0A9.015 9.015 0 013 12c0-.778.099-1.533.284-2.253"/></svg>
                    <span>{lang_btn_label}</span>
                </a>

                <a href="/logout" class="px-2.5 py-1 rounded-md text-xs font-mono font-medium text-slate-300 hover:text-white bg-white/[0.04] hover:bg-white/[0.08] transition-colors duration-150 flex items-center gap-1.5 border border-white/[0.08] hover:border-rose-500/40">
                    <svg class="w-3.5 h-3.5 text-slate-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"/></svg>
                    <span>{logout}</span>
                </a>
            </div>
        </div>
    </header>"##,
        home_url = home_url,
        backups_url = backups_url,
        app_title = i18n.common.app_title,
        app_subtitle = i18n.common.app_subtitle,
        home_classes = home_classes,
        backups_classes = backups_classes,
        nav_home = i18n.dashboard.nav_home,
        nav_backups = i18n.dashboard.nav_backups,
        active_monitoring = i18n.common.active_monitoring,
        next_lang = next_lang,
        lang_redirect = lang_redirect,
        lang_btn_label = lang_btn_label,
        lang_title = lang_title,
        logout = i18n.common.logout,
    )
}

async fn dashboard_page(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return Redirect::to("/login").into_response();
    }

    let i18n_arc = resolve_i18n(&jar, &state);
    let i18n = &*i18n_arc;
    let navbar = render_navbar(i18n, ActiveNav::Home, "/");
    let fragment = render_dashboard_fragment(&state, i18n, None).await;
    let head_tags = theme::render_head(
        &i18n.dashboard.page_title,
        r#"<script src="https://unpkg.com/htmx.org@2.0.4"></script>"#,
    );
    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex flex-col font-sans antialiased selection:bg-rose-600 selection:text-white">
    {navbar}

    <main class="flex-1 max-w-6xl w-full mx-auto px-4 py-6 sm:py-8">
        <div id="dashboard-content" hx-get="/api/dashboard-fragment" hx-trigger="every 2s" hx-swap="innerHTML">
            {fragment}
        </div>
    </main>

    <footer class="border-t border-rose-500/10 py-4 text-center text-xs font-mono text-slate-500">
        {footer}
    </footer>
</body>
</html>"##,
        lang = i18n.lang,
        head_tags = head_tags,
        navbar = navbar,
        fragment = fragment,
        footer = i18n.common.footer,
    );
    Html(html).into_response()
}

#[derive(Deserialize, Default)]
struct BackupsPageQuery {
    pcloud_connected: Option<String>,
    pcloud_error: Option<String>,
}

async fn backups_page(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<BackupsPageQuery>,
) -> Response {
    if !is_authenticated(&jar) {
        return Redirect::to("/login").into_response();
    }

    let i18n_arc = resolve_i18n(&jar, &state);
    let i18n = &*i18n_arc;
    let navbar = render_navbar(i18n, ActiveNav::Backups, "/backups");
    let head_tags = theme::render_head(
        &format!("{} - Komainu", i18n.dashboard.nav_backups),
        r#"<script src="https://unpkg.com/htmx.org@2.0.4"></script>"#,
    );

    let banner_msg = if query.pcloud_connected.as_deref() == Some("1") {
        Some((true, i18n.backups.pcloud_auth_success.clone()))
    } else if let Some(err) = query.pcloud_error.as_deref() {
        let msg = match err {
            "missing_client_id" => "Client ID da aplicação pCloud não foi configurado.",
            "missing_code" => "Código de autorização OAuth ausente.",
            "missing_app_credentials" => "Client ID ou Client Secret ausentes para troca de token.",
            _ => err,
        };
        Some((false, format!("{}: {msg}", i18n.backups.pcloud_auth_failed)))
    } else {
        None
    };

    let fragment = render_backups_fragment(&state, i18n, banner_msg).await;

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="{lang}" class="dark">
<head>
    {head_tags}
</head>
<body class="text-slate-100 min-h-screen flex flex-col font-sans antialiased selection:bg-rose-600 selection:text-white">
    {navbar}

    <main class="flex-1 max-w-6xl w-full mx-auto px-4 py-6 sm:py-8">
        <div id="backups-content" hx-get="/api/backups/fragment" hx-trigger="backupCompleted from:body" hx-swap="innerHTML">
            {fragment}
        </div>
    </main>

    <footer class="border-t border-rose-500/10 py-4 text-center text-xs font-mono text-slate-500">
        {footer}
    </footer>
</body>
</html>"##,
        lang = i18n.lang,
        head_tags = head_tags,
        navbar = navbar,
        fragment = fragment,
        footer = i18n.common.footer,
    );

    Html(html).into_response()
}

#[derive(Deserialize)]
struct BackupConfigForm {
    enabled: Option<String>,
    cron_expression: Option<String>,
    encryption_password: Option<String>,
    pcloud_region: Option<String>,
    pcloud_client_id: Option<String>,
    pcloud_client_secret: Option<String>,
    pcloud_folder: Option<String>,
}

async fn backup_save_config_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    axum::extract::Form(form): axum::extract::Form<BackupConfigForm>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
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
struct PCloudAuthorizeQuery {
    client_id: Option<String>,
    client_secret: Option<String>,
}

async fn pcloud_oauth_authorize_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<PCloudAuthorizeQuery>,
) -> Response {
    if !is_authenticated(&jar) {
        return Redirect::to("/login").into_response();
    }

    let mut cfg = state.backup_service.get_config().await.unwrap_or_default();

    let mut changed = false;
    if let Some(cid) = query.client_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        cfg.pcloud_client_id = cid;
        changed = true;
    }
    if let Some(csec) = query.client_secret.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
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
        return Redirect::to("/backups?pcloud_error=missing_client_id").into_response();
    }

    let redirect_uri = format!("{}/api/backups/pcloud/callback", state.rp_origin.trim_end_matches('/'));
    let state_token = Uuid::new_v4().to_string();

    let auth_url = format!(
        "https://my.pcloud.com/oauth2/authorize?client_id={}&response_type=code&redirect_uri={}&state={}",
        crate::infrastructure::pcloud::urlencoding(effective_client_id),
        crate::infrastructure::pcloud::urlencoding(&redirect_uri),
        crate::infrastructure::pcloud::urlencoding(&state_token),
    );

    Redirect::to(&auth_url).into_response()
}

#[derive(Deserialize, Default)]
struct PCloudCallbackQuery {
    code: Option<String>,
    hostname: Option<String>,
    locationid: Option<u32>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn pcloud_oauth_callback_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Query(query): Query<PCloudCallbackQuery>,
) -> Response {
    if !is_authenticated(&jar) {
        return Redirect::to("/login").into_response();
    }

    if let Some(err) = query.error.or(query.error_description) {
        return Redirect::to(&format!(
            "/backups?pcloud_error={}",
            crate::infrastructure::pcloud::urlencoding(&err)
        ))
        .into_response();
    }

    let code = match query.code {
        Some(c) if !c.is_empty() => c,
        _ => return Redirect::to("/backups?pcloud_error=missing_code").into_response(),
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
        return Redirect::to("/backups?pcloud_error=missing_app_credentials").into_response();
    }

    let hostname = if let Some(h) = query.hostname.filter(|h| !h.is_empty()) {
        h
    } else if query.locationid == Some(2) {
        "eapi.pcloud.com".to_string()
    } else {
        "api.pcloud.com".to_string()
    };

    let region = if hostname.contains("eapi") || query.locationid == Some(2) {
        crate::domain::backup::PCloudRegion::EU
    } else {
        crate::domain::backup::PCloudRegion::US
    };

    let token_endpoint = format!("https://{hostname}/oauth2_token");
    let http_client = reqwest::Client::new();
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
                    Redirect::to("/backups?pcloud_connected=1").into_response()
                } else {
                    let err = json.error.unwrap_or_else(|| format!("result {:?}", json.result));
                    Redirect::to(&format!(
                        "/backups?pcloud_error={}",
                        crate::infrastructure::pcloud::urlencoding(&err)
                    ))
                    .into_response()
                }
            } else {
                Redirect::to("/backups?pcloud_error=invalid_token_response").into_response()
            }
        }
        Err(e) => {
            Redirect::to(&format!(
                "/backups?pcloud_error={}",
                crate::infrastructure::pcloud::urlencoding(&e.to_string())
            ))
            .into_response()
        }
    }
}

async fn pcloud_disconnect_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }

    let mut cfg = state.backup_service.get_config().await.unwrap_or_default();
    cfg.pcloud_token = String::new();
    let _ = state.backup_service.save_config(&cfg).await;

    let msg = Some((true, i18n.backups.pcloud_disconnected_success.clone()));
    Html(render_backups_fragment(&state, &i18n, msg).await).into_response()
}

async fn backup_run_now_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }

    let result = state.backup_service.execute_backup().await;
    let msg = match result {
        Ok(rec) => Some((true, format!("{} ({})", i18n.backups.backup_started_success, rec.filename))),
        Err(e) => Some((false, format!("{}: {e}", i18n.common.operation_failed))),
    };

    Html(render_backups_fragment(&state, &i18n, msg).await).into_response()
}

async fn backup_fragment_handler(
    jar: SignedCookieJar,
    State(state): State<AppState>,
) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        let mut headers = HeaderMap::new();
        headers.insert("HX-Redirect", "/login".parse().unwrap());
        return (StatusCode::UNAUTHORIZED, headers, i18n.common.session_expired.clone()).into_response();
    }

    Html(render_backups_fragment(&state, &i18n, None).await).into_response()
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

async fn render_backups_fragment(
    state: &AppState,
    i18n: &crate::i18n::Translations,
    action_msg: Option<(bool, String)>,
) -> String {
    let config = state.backup_service.get_config().await.unwrap_or_default();
    let history = state.backup_service.get_history().await.unwrap_or_default();
    let is_in_progress = state.backup_service.is_in_progress().await;
    let data_exists = state.backup_service.data_path_exists();
    let env_exists = state.backup_service.env_path_exists();

    let alert_banner = match action_msg {
        Some((true, msg)) => format!(
            r##"<div id="backup-action-alert" class="p-4 rounded-xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 text-xs font-mono flex items-center justify-between gap-3 shadow-lg">
                <div class="flex items-center gap-2.5">
                    <span class="px-1.5 py-0.5 rounded bg-emerald-500/20 text-emerald-400 font-bold shrink-0">OK</span>
                    <span>{}</span>
                </div>
                <button type="button" onclick="document.getElementById('backup-action-alert').remove()" class="text-emerald-400/70 hover:text-emerald-200 transition p-1 rounded-md hover:bg-emerald-500/20" title="Dismiss">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
            </div>"##,
            msg
        ),
        Some((false, msg)) => format!(
            r##"<div id="backup-action-alert" class="p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono flex items-center justify-between gap-3 shadow-lg">
                <div class="flex items-center gap-2.5">
                    <span class="px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-400 font-bold shrink-0">FAIL</span>
                    <span>{}</span>
                </div>
                <button type="button" onclick="document.getElementById('backup-action-alert').remove()" class="text-rose-400/70 hover:text-rose-200 transition p-1 rounded-md hover:bg-rose-500/20" title="Dismiss">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
            </div>"##,
            msg
        ),
        None => String::new(),
    };

    let next_run_str = if config.enabled {
        let schedule = cron::Schedule::from_str(&config.cron_expression)
            .or_else(|_| cron::Schedule::from_str(&format!("0 {}", config.cron_expression.trim())));
        match schedule {
            Ok(s) => s
                .upcoming(chrono::Local)
                .next()
                .map(|dt| dt.format("%d/%m/%Y às %H:%M:%S").to_string())
                .unwrap_or_else(|| i18n.backups.no_scheduled_run.clone()),
            Err(_) => i18n.backups.no_scheduled_run.clone(),
        }
    } else {
        i18n.backups.no_scheduled_run.clone()
    };

    let status_badge = if config.enabled {
        format!(
            r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/25 flex items-center gap-1.5">
                <span class="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span>
                {} &bull; {}
            </span>"##,
            i18n.backups.cron_enabled, config.cron_expression
        )
    } else {
        format!(
            r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-white/[0.05] text-slate-400 border border-white/[0.08] flex items-center gap-1.5">
                <span class="w-1.5 h-1.5 rounded-full bg-slate-500"></span>
                {}
            </span>"##,
            i18n.backups.cron_disabled
        )
    };

    let run_btn_disabled = if is_in_progress { "disabled" } else { "" };
    let run_btn_text = if is_in_progress {
        &i18n.backups.btn_running
    } else {
        &i18n.backups.btn_backup_now
    };

    let cron_checked = if config.enabled { "checked" } else { "" };
    let region_eu_selected = if config.pcloud_region == crate::domain::backup::PCloudRegion::EU { "selected" } else { "" };
    let region_us_selected = if config.pcloud_region == crate::domain::backup::PCloudRegion::US { "selected" } else { "" };

    let data_path_badge = if data_exists {
        format!(r##"<span class="px-2 py-0.5 rounded text-[11px] font-mono bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">{}</span>"##, i18n.backups.path_found)
    } else {
        format!(r##"<span class="px-2 py-0.5 rounded text-[11px] font-mono bg-rose-500/10 text-rose-400 border border-rose-500/20">{}</span>"##, i18n.backups.path_missing)
    };

    let env_path_badge = if env_exists {
        format!(r##"<span class="px-2 py-0.5 rounded text-[11px] font-mono bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">{}</span>"##, i18n.backups.path_found)
    } else {
        format!(r##"<span class="px-2 py-0.5 rounded text-[11px] font-mono bg-rose-500/10 text-rose-400 border border-rose-500/20">{}</span>"##, i18n.backups.path_missing)
    };

    let mut history_rows = String::new();
    if history.is_empty() {
        history_rows.push_str(&format!(
            r##"<tr><td colspan="5" class="py-8 text-center text-xs text-slate-500 font-mono">{}</td></tr>"##,
            i18n.backups.history_empty
        ));
    } else {
        for rec in history.iter().take(20) {
            let dt_str = chrono::DateTime::from_timestamp(rec.timestamp as i64, 0)
                .map(|dt| dt.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M:%S").to_string())
                .unwrap_or_else(|| "N/A".to_string());
            let (badge, badge_color) = match &rec.status {
                crate::domain::backup::BackupStatus::Success => (i18n.backups.status_success.as_str(), "bg-emerald-500/10 text-emerald-400 border-emerald-500/25"),
                crate::domain::backup::BackupStatus::Failed(e) => (e.as_str(), "bg-rose-500/10 text-rose-400 border-rose-500/25"),
            };
            let sz_str = format_bytes(rec.size_bytes);
            let dur_str = format!("{}s", rec.duration_secs);
            history_rows.push_str(&format!(
                r##"<tr class="border-b border-white/[0.04] hover:bg-white/[0.02] transition-colors text-xs font-mono">
                    <td class="py-3 px-3 text-slate-300 whitespace-nowrap">{dt_str}</td>
                    <td class="py-3 px-3 text-white font-medium break-all">{filename}</td>
                    <td class="py-3 px-3 text-slate-300 tabular-nums whitespace-nowrap">{sz_str}</td>
                    <td class="py-3 px-3 text-slate-400 tabular-nums whitespace-nowrap">{dur_str}</td>
                    <td class="py-3 px-3 whitespace-nowrap">
                        <span class="px-2 py-0.5 rounded text-[11px] font-semibold border {badge_color}">{badge}</span>
                    </td>
                </tr>"##,
                dt_str = dt_str,
                filename = rec.filename,
                sz_str = sz_str,
                dur_str = dur_str,
                badge = badge,
                badge_color = badge_color,
            ));
        }
    }

    let enc_pass_ph = if config.encryption_password.is_empty() {
        i18n.backups.encryption_password_placeholder.as_str()
    } else {
        "••••••••••••"
    };

    let client_id_val = if !config.pcloud_client_id.is_empty() {
        &config.pcloud_client_id
    } else {
        &state.pcloud_client_id
    };

    let client_secret_ph = if config.pcloud_client_secret.is_empty() && state.pcloud_client_secret.is_empty() {
        i18n.backups.pcloud_client_secret_placeholder.as_str()
    } else {
        "••••••••••••"
    };

    let callback_url = format!("{}/api/backups/pcloud/callback", state.rp_origin.trim_end_matches('/'));

    let is_pcloud_connected = !config.pcloud_token.is_empty();
    let pcloud_status_card = if is_pcloud_connected {
        let region_str = match config.pcloud_region {
            crate::domain::backup::PCloudRegion::EU => "EU",
            crate::domain::backup::PCloudRegion::US => "US",
        };
        format!(
            r##"<div class="p-3.5 rounded-lg bg-emerald-500/10 border border-emerald-500/25 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div class="flex items-center gap-2.5">
                    <span class="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-pulse"></span>
                    <div>
                        <p class="text-xs font-mono font-semibold text-emerald-300">{} ({})</p>
                        <p class="text-[11px] text-slate-400 font-sans">Token OAuth ativo e pronto para backups.</p>
                    </div>
                </div>
                <div class="flex items-center gap-2">
                    <button type="button" onclick="startPCloudOAuth()" class="px-3 py-1.5 rounded-md bg-white/[0.06] hover:bg-white/[0.1] text-xs font-mono text-slate-200 border border-white/[0.08] transition-colors flex items-center gap-1.5">
                        <span>🔄</span> <span>{}</span>
                    </button>
                    <button type="button" hx-post="/api/backups/pcloud/disconnect" hx-target="#backups-content" hx-swap="innerHTML" class="px-3 py-1.5 rounded-md bg-rose-500/10 hover:bg-rose-500/20 text-xs font-mono text-rose-300 border border-rose-500/20 transition-colors flex items-center gap-1.5">
                        <span>✕</span> <span>{}</span>
                    </button>
                </div>
            </div>"##,
            i18n.backups.pcloud_connected,
            region_str,
            i18n.backups.pcloud_reconnect_btn,
            i18n.backups.pcloud_disconnect_btn
        )
    } else {
        format!(
            r##"<div class="p-3.5 rounded-lg bg-white/[0.02] border border-white/[0.08] flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div class="flex items-center gap-2.5">
                    <span class="w-2.5 h-2.5 rounded-full bg-slate-500"></span>
                    <div>
                        <p class="text-xs font-mono font-semibold text-slate-300">{}</p>
                        <p class="text-[11px] text-slate-400 font-sans">Faça login com seu app pCloud para vincular o acesso aos backups.</p>
                    </div>
                </div>
                <div>
                    <button type="button" onclick="startPCloudOAuth()" class="w-full sm:w-auto px-4 py-2 rounded-lg text-xs font-semibold font-sans bg-rose-600 hover:bg-rose-500 active:translate-y-px text-white shadow-md flex items-center justify-center gap-2 transition-all">
                        <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/></svg>
                        <span>{}</span>
                    </button>
                </div>
            </div>"##,
            i18n.backups.pcloud_not_connected,
            i18n.backups.pcloud_oauth_btn
        )
    };

    format!(
        r##"<div class="space-y-6">
    {alert_banner}

    <!-- CARD 1: STATUS DO AGENDAMENTO E EXECUÇÃO RÁPIDA -->
    <div class="bg-surface border border-rose-500/20 rounded-xl p-5 sm:p-6 shadow-2xl relative overflow-hidden">
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/50 to-transparent"></div>

        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
            <div>
                <div class="flex items-center gap-3">
                    <h2 class="text-xl font-display font-bold tracking-tight text-white">{backups_title}</h2>
                    {status_badge}
                </div>
                <p class="text-xs text-slate-400 font-mono mt-1.5 flex items-center gap-2">
                    <span>{next_run_label}</span>
                    <strong class="text-rose-300">{next_run_val}</strong>
                </p>
            </div>

            <div>
                <button 
                    hx-post="/api/backups/run" 
                    hx-target="#backups-content" 
                    hx-swap="innerHTML"
                    {run_btn_disabled}
                    class="w-full sm:w-auto px-4 py-2.5 rounded-lg text-xs font-semibold font-sans transition-all duration-150 flex items-center justify-center gap-2 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-40 disabled:cursor-wait text-white border border-rose-400/30 shadow-md">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"/></svg>
                    <span>{run_btn_text}</span>
                </button>
            </div>
        </div>
    </div>

    <!-- CARD 2: CONFIGURAÇÕES DA ROTINA, CRON E PCLOUD -->
    <div class="bg-surface border border-rose-500/20 rounded-xl p-5 sm:p-6 shadow-2xl relative overflow-hidden">
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/50 to-transparent"></div>

        <div class="mb-5 pb-4 border-b border-white/[0.06]">
            <h3 class="text-lg font-display font-bold tracking-tight text-white">{section_config}</h3>
            <p class="text-xs text-slate-400 mt-0.5">{backups_subtitle}</p>
        </div>

        <form hx-post="/api/backups/config" hx-target="#backups-content" hx-swap="innerHTML" class="space-y-6">
            <!-- Bloco 1: Cronjob -->
            <div class="space-y-3">
                <div class="flex items-center gap-2.5">
                    <input type="checkbox" id="cron-enabled-check" name="enabled" {cron_checked} class="w-4 h-4 rounded bg-canvas border-white/[0.1] text-rose-600 focus:ring-rose-500">
                    <label for="cron-enabled-check" class="text-xs font-mono font-semibold text-slate-200 cursor-pointer">{enable_cron}</label>
                </div>

                <div>
                    <label for="cron-input" class="block text-xs font-mono text-slate-400 mb-1.5">{cron_expression_label}</label>
                    <input type="text" id="cron-input" name="cron_expression" value="{cron_val}" placeholder="0 3 * * *" class="w-full sm:max-w-md px-3 py-2 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                    <p class="text-[11px] text-slate-500 font-mono mt-1">{cron_hint}</p>

                    <!-- Presets rápidos -->
                    <div class="flex flex-wrap gap-1.5 mt-2">
                        <button type="button" onclick="document.getElementById('cron-input').value='0 3 * * *'" class="px-2 py-1 rounded bg-white/[0.04] hover:bg-white/[0.08] text-[11px] font-mono text-slate-300 border border-white/[0.06] transition-colors">{preset_daily}</button>
                        <button type="button" onclick="document.getElementById('cron-input').value='0 */12 * * *'" class="px-2 py-1 rounded bg-white/[0.04] hover:bg-white/[0.08] text-[11px] font-mono text-slate-300 border border-white/[0.06] transition-colors">{preset_12h}</button>
                        <button type="button" onclick="document.getElementById('cron-input').value='0 3 * * 0'" class="px-2 py-1 rounded bg-white/[0.04] hover:bg-white/[0.08] text-[11px] font-mono text-slate-300 border border-white/[0.06] transition-colors">{preset_weekly}</button>
                    </div>
                </div>
            </div>

            <!-- Bloco 2: Criptografia -->
            <div class="pt-4 border-t border-white/[0.06] space-y-3">
                <div class="flex items-center gap-2">
                    <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
                    <h4 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{encryption_section}</h4>
                </div>

                <div>
                    <label for="enc-pass" class="block text-xs font-mono text-slate-400 mb-1.5">{encryption_password_label}</label>
                    <div class="relative w-full sm:max-w-md">
                        <input type="password" id="enc-pass" name="encryption_password" value="" placeholder="{enc_pass_ph}" autocomplete="new-password" class="w-full px-3 py-2 pr-10 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                        <button type="button" onclick="const f=document.getElementById('enc-pass');f.type=f.type==='password'?'text':'password';" class="absolute right-2.5 top-2 text-slate-400 hover:text-white text-xs font-mono" title="Toggle visibility">👁️</button>
                    </div>
                    <p class="text-[11px] text-slate-500 font-sans mt-1">O arquivo .zip contendo banco, attachments, sends, chaves e templates será criptografado antes do envio.</p>
                </div>
            </div>

            <!-- Bloco 3: pCloud API & OAuth -->
            <div class="pt-4 border-t border-white/[0.06] space-y-4">
                <div class="flex items-center gap-2">
                    <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
                    <h4 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{pcloud_section}</h4>
                </div>

                {pcloud_status_card}

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                        <label for="pcloud-cid" class="block text-xs font-mono text-slate-400 mb-1.5">{pcloud_client_id_label}</label>
                        <input type="text" id="pcloud-cid" name="pcloud_client_id" value="{pcloud_client_id_val}" placeholder="{pcloud_client_id_ph}" class="w-full px-3 py-2 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                    </div>

                    <div>
                        <label for="pcloud-csec" class="block text-xs font-mono text-slate-400 mb-1.5">{pcloud_client_secret_label}</label>
                        <div class="relative w-full">
                            <input type="password" id="pcloud-csec" name="pcloud_client_secret" value="" placeholder="{pcloud_client_secret_ph}" autocomplete="off" class="w-full px-3 py-2 pr-10 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                            <button type="button" onclick="const f=document.getElementById('pcloud-csec');f.type=f.type==='password'?'text':'password';" class="absolute right-2.5 top-2 text-slate-400 hover:text-white text-xs font-mono" title="Toggle visibility">👁️</button>
                        </div>
                    </div>
                </div>

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                        <label class="block text-xs font-mono text-slate-400 mb-1.5">{pcloud_region_label}</label>
                        <select name="pcloud_region" class="w-full px-3 py-2 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                            <option value="EU" {region_eu_selected}>{pcloud_region_eu}</option>
                            <option value="US" {region_us_selected}>{pcloud_region_us}</option>
                        </select>
                        <p class="text-[11px] text-slate-500 font-sans mt-1">Detectado automaticamente durante o login OAuth.</p>
                    </div>

                    <div>
                        <label class="block text-xs font-mono text-slate-400 mb-1.5">{pcloud_folder_label}</label>
                        <input type="text" name="pcloud_folder" value="{pcloud_folder_val}" placeholder="{pcloud_folder_ph}" class="w-full px-3 py-2 rounded-lg bg-canvas border border-white/[0.08] focus:border-rose-500/60 text-slate-100 text-xs font-mono">
                    </div>
                </div>

                <div class="p-3 rounded-lg bg-white/[0.02] border border-white/[0.06] text-xs font-mono">
                    <div class="flex items-center justify-between gap-2 mb-1">
                        <span class="text-slate-400">{pcloud_callback_url_label}:</span>
                        <button type="button" onclick="navigator.clipboard.writeText('{callback_url}');this.innerText='Copiado!';" class="px-2 py-0.5 rounded bg-white/[0.05] hover:bg-white/[0.1] text-slate-300 border border-white/[0.08] text-[11px] transition-colors">Copiar</button>
                    </div>
                    <div class="text-rose-300 font-semibold break-all select-all">{callback_url}</div>
                    <p class="text-[11px] text-slate-500 font-sans mt-1.5">{pcloud_callback_hint}</p>
                </div>
            </div>

            <!-- Bloco 4: Diagnóstico de Caminhos Locais -->
            <div class="pt-4 border-t border-white/[0.06] space-y-3">
                <div class="flex items-center gap-2">
                    <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
                    <h4 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{local_paths_section}</h4>
                </div>

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs font-mono">
                    <div class="p-3 rounded-lg bg-canvas border border-white/[0.06] flex items-center justify-between gap-2">
                        <span class="text-slate-400 truncate">{data_path_label}</span>
                        {data_path_badge}
                    </div>
                    <div class="p-3 rounded-lg bg-canvas border border-white/[0.06] flex items-center justify-between gap-2">
                        <span class="text-slate-400 truncate">{env_path_label}</span>
                        {env_path_badge}
                    </div>
                </div>
            </div>

            <div class="pt-4 flex justify-end">
                <button type="submit" class="px-5 py-2.5 rounded-lg text-xs font-semibold font-sans transition-all duration-150 flex items-center gap-2 bg-rose-600 hover:bg-rose-500 active:translate-y-px text-white border border-rose-400/30 shadow-md">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>
                    <span>{btn_save_config}</span>
                </button>
            </div>
        </form>
    </div>

    <!-- CARD 3: HISTÓRICO DE BACKUPS -->
    <div class="bg-surface border border-rose-500/20 rounded-xl p-5 sm:p-6 shadow-2xl relative overflow-hidden">
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/50 to-transparent"></div>

        <div class="mb-4 pb-3 border-b border-white/[0.06] flex items-center justify-between">
            <div>
                <h3 class="text-lg font-display font-bold tracking-tight text-white">{history_section}</h3>
            </div>
            <button hx-get="/api/backups/fragment" hx-target="#backups-content" hx-swap="innerHTML" class="p-1.5 rounded-md hover:bg-white/[0.06] text-slate-400 hover:text-white transition-colors" title="Atualizar Histórico">
                <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
            </button>
        </div>

        <div class="overflow-x-auto">
            <table class="w-full text-left border-collapse">
                <thead>
                    <tr class="border-b border-white/[0.08] text-[11px] font-mono uppercase text-slate-400">
                        <th class="py-2.5 px-3">{table_date}</th>
                        <th class="py-2.5 px-3">{table_filename}</th>
                        <th class="py-2.5 px-3">{table_size}</th>
                        <th class="py-2.5 px-3">{table_duration}</th>
                        <th class="py-2.5 px-3">{table_status}</th>
                    </tr>
                </thead>
                <tbody>
                    {history_rows}
                </tbody>
            </table>
        </div>
    </div>

    <script>
    function startPCloudOAuth() {{
        const cidEl = document.getElementById('pcloud-cid');
        const csecEl = document.getElementById('pcloud-csec');
        const cid = cidEl ? cidEl.value.trim() : '';
        const csec = csecEl ? csecEl.value.trim() : '';
        const params = new URLSearchParams();
        if (cid) params.set('client_id', cid);
        if (csec) params.set('client_secret', csec);
        const qs = params.toString();
        window.location.href = qs ? '/api/backups/pcloud/authorize?' + qs : '/api/backups/pcloud/authorize';
    }}
    </script>
</div>"##,
        alert_banner = alert_banner,
        backups_title = i18n.dashboard.backups_title,
        backups_subtitle = i18n.dashboard.backups_subtitle,
        status_badge = status_badge,
        next_run_label = i18n.backups.next_run,
        next_run_val = next_run_str,
        run_btn_disabled = run_btn_disabled,
        run_btn_text = run_btn_text,
        section_config = i18n.backups.section_config,
        enable_cron = i18n.backups.enable_cron,
        cron_checked = cron_checked,
        cron_expression_label = i18n.backups.cron_expression_label,
        cron_val = config.cron_expression,
        cron_hint = i18n.backups.cron_hint,
        preset_daily = i18n.backups.cron_preset_daily,
        preset_12h = i18n.backups.cron_preset_12h,
        preset_weekly = i18n.backups.cron_preset_weekly,
        encryption_section = i18n.backups.encryption_section,
        encryption_password_label = i18n.backups.encryption_password_label,
        enc_pass_ph = enc_pass_ph,
        pcloud_section = i18n.backups.pcloud_section,
        pcloud_status_card = pcloud_status_card,
        pcloud_client_id_label = i18n.backups.pcloud_client_id_label,
        pcloud_client_id_val = client_id_val,
        pcloud_client_id_ph = i18n.backups.pcloud_client_id_placeholder,
        pcloud_client_secret_label = i18n.backups.pcloud_client_secret_label,
        pcloud_client_secret_ph = client_secret_ph,
        pcloud_region_label = i18n.backups.pcloud_region_label,
        pcloud_region_eu = i18n.backups.pcloud_region_eu,
        pcloud_region_us = i18n.backups.pcloud_region_us,
        region_eu_selected = region_eu_selected,
        region_us_selected = region_us_selected,
        pcloud_folder_label = i18n.backups.pcloud_folder_label,
        pcloud_folder_val = config.pcloud_folder,
        pcloud_folder_ph = i18n.backups.pcloud_folder_placeholder,
        pcloud_callback_url_label = i18n.backups.pcloud_callback_url_label,
        pcloud_callback_hint = i18n.backups.pcloud_callback_hint,
        callback_url = callback_url,
        local_paths_section = i18n.backups.local_paths_section,
        data_path_label = i18n.backups.data_path_label,
        data_path_badge = data_path_badge,
        env_path_label = i18n.backups.env_path_label,
        env_path_badge = env_path_badge,
        btn_save_config = i18n.backups.btn_save_config,
        history_section = i18n.backups.history_section,
        table_date = i18n.backups.table_date,
        table_filename = i18n.backups.table_filename,
        table_size = i18n.backups.table_size,
        table_duration = i18n.backups.table_duration,
        table_status = i18n.backups.table_status,
        history_rows = history_rows,
    )
}

async fn dashboard_fragment_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        let mut headers = HeaderMap::new();
        headers.insert("HX-Redirect", "/login".parse().unwrap());
        return (StatusCode::UNAUTHORIZED, headers, i18n.common.session_expired.clone()).into_response();
    }
    Html(render_dashboard_fragment(&state, &i18n, None).await).into_response()
}

async fn container_start_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.start().await.err().map(|e| e.to_localized(&i18n));
    Html(render_dashboard_fragment(&state, &i18n, err).await).into_response()
}

async fn container_stop_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.stop().await.err().map(|e| e.to_localized(&i18n));
    Html(render_dashboard_fragment(&state, &i18n, err).await).into_response()
}

async fn container_restart_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.restart().await.err().map(|e| e.to_localized(&i18n));
    Html(render_dashboard_fragment(&state, &i18n, err).await).into_response()
}

async fn container_update_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.update_image().await.err().map(|e| e.to_localized(&i18n));
    Html(render_dashboard_fragment(&state, &i18n, err).await).into_response()
}

async fn container_rebuild_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    let i18n = resolve_i18n(&jar, &state);
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, i18n.common.unauthorized.clone()).into_response();
    }
    let err = state.container_service.rebuild().await.err().map(|e| e.to_localized(&i18n));
    Html(render_dashboard_fragment(&state, &i18n, err).await).into_response()
}

async fn render_dashboard_fragment(state: &AppState, i18n: &crate::i18n::Translations, action_error: Option<String>) -> String {
    let status_res = state.container_service.status().await;
    let metrics = state.system_service.get_metrics().await;

    let error_banner = match action_error {
        Some(e) => format!(
            r##"<div id="action-error-banner" class="p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs font-mono flex items-center justify-between gap-3 shadow-lg">
                <div class="flex items-center gap-2.5">
                    <span class="px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-400 font-bold shrink-0">FAIL</span>
                    <span><strong>{}</strong> {}</span>
                </div>
                <button type="button" onclick="document.getElementById('action-error-banner').remove()" class="text-rose-400/70 hover:text-rose-200 transition p-1 rounded-md hover:bg-rose-500/20" title="Dismiss">
                    <svg class="w-4 h-4 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
            </div>"##,
            i18n.common.operation_failed,
            e
        ),
        None => String::new(),
    };

    let (status_badge, image_name, uptime_str, container_cpu_val, container_cpu_bar_pct, container_ram_val, container_ram_bar_pct) = match &status_res {
        Ok(status) => {
            let badge = match status.state {
                ContainerState::Running => {
                    format!(
                        r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 flex items-center gap-1.5">
                        <span class="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span> {}
                    </span>"##,
                        i18n.dashboard.status_running
                    )
                }
                ContainerState::Stopped => {
                    format!(
                        r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-rose-500/10 text-rose-400 border border-rose-500/30 flex items-center gap-1.5">
                        <span class="w-1.5 h-1.5 rounded-full bg-rose-400"></span> {}
                    </span>"##,
                        i18n.dashboard.status_stopped
                    )
                }
                ContainerState::Restarting => {
                    format!(
                        r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-amber-500/10 text-amber-400 border border-amber-500/30 flex items-center gap-1.5">
                        <span class="w-1.5 h-1.5 rounded-full bg-amber-400 animate-spin"></span> {}
                    </span>"##,
                        i18n.dashboard.status_restarting
                    )
                }
                _ => {
                    format!(
                        r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-white/[0.05] text-slate-400 border border-white/[0.08]">
                        {}
                    </span>"##,
                        i18n.dashboard.status_unknown
                    )
                }
            };

            let cpu_val = status.cpu_usage_pct.map(|c| format!("{c:.1}%")).unwrap_or_else(|| "0.0%".to_string());
            let cpu_bar = status.cpu_usage_pct.unwrap_or(0.0).clamp(0.0, 100.0);

            let (ram_val, ram_bar) = match (status.memory_used_mb, status.memory_limit_mb) {
                (Some(used), Some(limit)) if limit > 0 && limit < metrics.ram_total_mb => {
                    let pct = (used as f32 / limit as f32) * 100.0;
                    (format!("{used} / {limit} MB ({pct:.1}%)"), pct.clamp(0.0, 100.0))
                }
                (Some(used), _) => {
                    let pct = if metrics.ram_total_mb > 0 {
                        (used as f32 / metrics.ram_total_mb as f32) * 100.0
                    } else {
                        0.0
                    };
                    (format!("{used} MB ({pct:.1}%)"), pct.clamp(0.0, 100.0))
                }
                (None, _) => ("0 MB (0.0%)".to_string(), 0.0),
            };

            (
                badge,
                status.image.clone(),
                status.uptime.clone().unwrap_or_else(|| "N/A".to_string()),
                cpu_val,
                cpu_bar,
                ram_val,
                ram_bar,
            )
        }
        Err(e) => (
            format!(
                r##"<span class="px-2.5 py-1 rounded-md text-xs font-mono font-semibold bg-rose-500/10 text-rose-400 border border-rose-500/30">
                    {} {}
                </span>"##,
                i18n.common.operation_failed,
                e
            ),
            "N/A".to_string(),
            "N/A".to_string(),
            "0.0%".to_string(),
            0.0,
            "0 MB (0.0%)".to_string(),
            0.0,
        ),
    };

    let is_running = matches!(status_res.as_ref().map(|s| &s.state), Ok(ContainerState::Running));

    format!(
        r##"<div class="space-y-6">
    {error_banner}

    <!-- SEÇÃO 1: CONTAINER VAULTWARDEN -->
    <div class="bg-surface border border-rose-500/20 rounded-xl p-5 sm:p-6 shadow-2xl relative overflow-hidden">
        <div class="absolute top-0 left-0 right-0 h-[2px] bg-gradient-to-r from-transparent via-rose-500/50 to-transparent"></div>

        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-5 border-b border-white/[0.06]">
            <div>
                <div class="flex items-center gap-3">
                    <h2 class="text-xl font-display font-bold tracking-tight text-white">{instance_title}</h2>
                    {status_badge}
                </div>
                <p class="text-xs text-slate-400 mt-1 font-mono">{image_prefix} <span class="text-slate-300 font-semibold">{image_name}</span> &bull; {started_prefix} <span class="text-slate-300 font-semibold">{uptime_str}</span></p>
            </div>
            
            <div class="flex flex-wrap items-center gap-1.5 sm:gap-2 p-1 sm:p-1.5 bg-canvas/90 rounded-lg border border-white/[0.06] w-full sm:w-auto">
                <button 
                    hx-post="/api/container/start" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    {disabled_start}
                    class="px-2.5 sm:px-3 py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center flex-1 sm:flex-initial gap-1.5 bg-rose-600 hover:bg-rose-500 active:translate-y-px disabled:opacity-30 disabled:pointer-events-none text-white border border-rose-400/30 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-current" viewBox="0 0 20 20"><path d="M6.3 2.841A1.5 1.5 0 004 4.11v11.78a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z"/></svg>
                    <span>{btn_start}</span>
                </button>
                <button 
                    hx-post="/api/container/stop" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    {disabled_stop}
                    class="px-2.5 sm:px-3 py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center flex-1 sm:flex-initial gap-1.5 bg-surface-elevated hover:bg-rose-950/80 active:translate-y-px disabled:opacity-30 disabled:pointer-events-none text-rose-300 hover:text-white border border-rose-500/30 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-current" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M4.5 4.5A1.5 1.5 0 016 3h8a1.5 1.5 0 011.5 1.5v11a1.5 1.5 0 01-1.5 1.5H6a1.5 1.5 0 01-1.5-1.5v-11z" clip-rule="evenodd"/></svg>
                    <span>{btn_stop}</span>
                </button>
                <button 
                    hx-post="/api/container/restart" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    class="px-2.5 sm:px-3 py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center flex-1 sm:flex-initial gap-1.5 bg-surface-elevated hover:bg-rose-500/15 active:translate-y-px text-slate-200 hover:text-white border border-white/[0.08] hover:border-rose-500/40 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
                    <span>{btn_restart}</span>
                </button>
                <button 
                    hx-post="/api/container/update" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    class="px-2.5 sm:px-3 py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center flex-1 sm:flex-initial gap-1.5 bg-surface-elevated hover:bg-rose-500/15 active:translate-y-px text-slate-200 hover:text-white border border-white/[0.08] hover:border-rose-500/40 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/></svg>
                    <span>{btn_update}</span>
                </button>
                <button 
                    hx-post="/api/container/rebuild" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    hx-confirm="{confirm_rebuild}"
                    class="px-2.5 sm:px-3 py-1.5 rounded-md text-xs font-medium font-sans transition-colors duration-150 flex items-center justify-center flex-1 sm:flex-initial gap-1.5 bg-surface-elevated hover:bg-rose-500/15 active:translate-y-px text-slate-200 hover:text-white border border-white/[0.08] hover:border-rose-500/40 shadow-sm">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/></svg>
                    <span>{btn_rebuild}</span>
                </button>
            </div>
        </div>

        <!-- Consumo Exclusivo do Container -->
        <div class="mt-6 pt-5 border-t border-white/[0.06]">
            <div class="flex items-center gap-2 mb-3">
                <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
                <h3 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{container_section}</h3>
                <div class="flex-1 h-px bg-white/[0.04]"></div>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-6 pt-1">
                <!-- CPU Container -->
                <div class="flex flex-col justify-between">
                    <div class="flex items-center justify-between mb-2">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400">{container_cpu}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20">{container_cpu_val}</span>
                    </div>
                    <div class="w-full bg-surface-elevated h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {container_cpu_bar_pct:.1}%"></div>
                    </div>
                </div>

                <!-- RAM Container -->
                <div class="flex flex-col justify-between">
                    <div class="flex items-center justify-between mb-2">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400">{container_ram}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20">{container_ram_val}</span>
                    </div>
                    <div class="w-full bg-surface-elevated h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {container_ram_bar_pct:.1}%"></div>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <!-- SEÇÃO 2: CONSUMO GERAL DA MÁQUINA (HOST) -->
    <div class="space-y-3">
        <div class="flex items-center gap-2">
            <span class="w-1.5 h-1.5 bg-rose-500 rounded-sm"></span>
            <h3 class="text-xs font-mono uppercase tracking-wider text-slate-300 font-semibold">{host_section}</h3>
            <div class="flex-1 h-px bg-white/[0.04]"></div>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            <!-- Métrica CPU Host -->
            <div class="bg-surface/95 border border-white/[0.08] rounded-xl p-5 shadow-xl hover:border-rose-500/30 transition-colors duration-150 flex flex-col justify-between">
                <div>
                    <div class="flex items-center justify-between mb-3">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400">{cpu_title}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20">{cpu_usage:.1}%</span>
                    </div>
                    <div class="w-full bg-canvas h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {cpu_usage:.1}%"></div>
                    </div>
                </div>
                <div class="mt-4 pt-3 border-t border-white/[0.06] flex items-center justify-between text-[11px] font-mono">
                    <span class="text-slate-500">{host_label}</span>
                    <span class="text-rose-400 font-semibold tabular-nums">{cpu_usage:.1}%</span>
                </div>
            </div>

            <!-- Métrica RAM Host -->
            <div class="bg-surface/95 border border-white/[0.08] rounded-xl p-5 shadow-xl hover:border-rose-500/30 transition-colors duration-150 flex flex-col justify-between">
                <div>
                    <div class="flex items-center justify-between mb-3">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400">{ram_title}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20">{ram_used} / {ram_total} MB</span>
                    </div>
                    <div class="w-full bg-canvas h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {ram_pct:.1}%"></div>
                    </div>
                </div>
                <div class="mt-4 pt-3 border-t border-white/[0.06] flex items-center justify-between text-[11px] font-mono">
                    <span class="text-slate-500">{host_label}</span>
                    <span class="text-rose-400 font-semibold tabular-nums">{ram_pct:.1}% {occupied_suffix}</span>
                </div>
            </div>

            <!-- Métrica Disco Host -->
            <div class="bg-surface/95 border border-white/[0.08] rounded-xl p-5 shadow-xl hover:border-rose-500/30 transition-colors duration-150 flex flex-col justify-between">
                <div>
                    <div class="flex items-center justify-between mb-3">
                        <span class="text-xs font-mono uppercase tracking-wider text-slate-400">{disk_title}</span>
                        <span class="text-xs font-mono font-bold tabular-nums px-2 py-0.5 rounded bg-rose-500/10 text-rose-300 border border-rose-500/20">{disk_used} / {disk_total} GB</span>
                    </div>
                    <div class="w-full bg-canvas h-2 rounded-full overflow-hidden border border-white/[0.04]">
                        <div class="bg-rose-500 h-2 rounded-full transition-[width] duration-500 ease-out" style="width: {disk_pct:.1}%"></div>
                    </div>
                </div>
                <div class="mt-4 pt-3 border-t border-white/[0.06] flex items-center justify-between text-[11px] font-mono">
                    <span class="text-slate-500">{host_label}</span>
                    <span class="text-rose-400 font-semibold tabular-nums">{disk_pct:.1}% {occupied_suffix}</span>
                </div>
            </div>
        </div>
    </div>
</div>"##,
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
        container_section = i18n.metrics.container_section,
        container_cpu = i18n.metrics.container_cpu,
        container_cpu_val = container_cpu_val,
        container_cpu_bar_pct = container_cpu_bar_pct,
        container_ram = i18n.metrics.container_ram,
        container_ram_val = container_ram_val,
        container_ram_bar_pct = container_ram_bar_pct,
        host_section = i18n.metrics.host_section,
        cpu_title = i18n.metrics.cpu_title,
        cpu_usage = metrics.cpu_usage_pct,
        ram_title = i18n.metrics.ram_title,
        ram_used = metrics.ram_used_mb,
        ram_total = metrics.ram_total_mb,
        ram_pct = if metrics.ram_total_mb > 0 { (metrics.ram_used_mb as f32 / metrics.ram_total_mb as f32) * 100.0 } else { 0.0 },
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
                    memory_limit_mb: Some(512),
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
        fn exec_command(&self, _name: &str, _cmd: Vec<String>) -> BoxFuture<'_, Result<String, ContainerError>> {
            Box::pin(async { Ok("success".to_string()) })
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

    use std::sync::Mutex;
    use crate::domain::backup::{BackupConfig, BackupConfigStorePort, BackupError, BackupRecord, PCloudClientPort, PCloudRegion};
    use crate::application::backup_service::BackupService;

    struct MockBackupConfigStore {
        config: Mutex<BackupConfig>,
        history: Mutex<Vec<BackupRecord>>,
    }

    impl MockBackupConfigStore {
        fn new() -> Self {
            Self {
                config: Mutex::new(BackupConfig::default()),
                history: Mutex::new(Vec::new()),
            }
        }
    }

    impl BackupConfigStorePort for MockBackupConfigStore {
        fn load_config(&self) -> BoxFuture<'_, Result<BackupConfig, BackupError>> {
            let cfg = self.config.lock().unwrap().clone();
            Box::pin(async move { Ok(cfg) })
        }
        fn save_config(&self, config: &BackupConfig) -> BoxFuture<'_, Result<(), BackupError>> {
            *self.config.lock().unwrap() = config.clone();
            Box::pin(async move { Ok(()) })
        }
        fn load_history(&self) -> BoxFuture<'_, Result<Vec<BackupRecord>, BackupError>> {
            let hist = self.history.lock().unwrap().clone();
            Box::pin(async move { Ok(hist) })
        }
        fn add_record(&self, record: BackupRecord) -> BoxFuture<'_, Result<(), BackupError>> {
            self.history.lock().unwrap().push(record);
            Box::pin(async move { Ok(()) })
        }
    }

    struct MockPCloudClient;
    impl PCloudClientPort for MockPCloudClient {
        fn upload(
            &self,
            _region: &PCloudRegion,
            _token: &str,
            _folder: &str,
            _filename: &str,
            _data: Vec<u8>,
        ) -> BoxFuture<'_, Result<(), BackupError>> {
            Box::pin(async move { Ok(()) })
        }
    }

    fn create_test_backup_service() -> Arc<BackupService> {
        Arc::new(BackupService::new(
            Arc::new(MockContainerPort),
            Arc::new(MockBackupConfigStore::new()),
            Arc::new(MockPCloudClient),
            "vaultwarden".to_string(),
            "/tmp/test_vw_data",
        ))
    }

    #[tokio::test]
    async fn test_health_check_handler() {
        assert_eq!(health_check().await, "OK");
    }

    #[tokio::test]
    async fn test_challenge_serialization() {
        let auth_service = AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap();
        auth_service.set_registration_token("tok".to_string()).await;
        let (_reg_id, challenge) = auth_service.start_registration("tok").await.unwrap();
        let json = serde_json::to_string_pretty(&challenge).unwrap();
        println!("REG_CHALLENGE:\n{json}");
        assert!(json.contains("\"rp\""));
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
            backup_service: create_test_backup_service(),
            cookie_key: cookie_key.clone(),
            i18n: i18n.clone(),
            rp_origin: "http://localhost:3000".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        };

        let fragment = render_dashboard_fragment(&state, &state.i18n, None).await;
        assert!(fragment.contains("Instância Vaultwarden"));
        assert!(fragment.contains("Consumo do Container"));
        assert!(fragment.contains("Consumo Geral da Máquina (Host)"));
        assert!(fragment.contains("CPU do Container"));
        assert!(fragment.contains("RAM do Container"));
        assert!(fragment.contains("Uso de CPU"));
        assert!(fragment.contains("Memória RAM"));
        assert!(fragment.contains("Armazenamento"));
        assert!(fragment.contains("vaultwarden:test"));
        assert!(fragment.contains("hx-post=\"/api/container/start\""));

        let fragment_err = render_dashboard_fragment(&state, &state.i18n, Some("Falha ao iniciar".to_string())).await;
        assert!(fragment_err.contains("Falha na operação:"));
        assert!(fragment_err.contains("Falha ao iniciar"));

        // Test English fragment
        let en_state = AppState {
            container_service,
            system_service,
            auth_service,
            backup_service: create_test_backup_service(),
            cookie_key,
            i18n: Arc::new(crate::i18n::I18n::load("en")),
            rp_origin: "http://localhost:3000".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        };
        let en_fragment = render_dashboard_fragment(&en_state, &en_state.i18n, None).await;
        assert!(en_fragment.contains("Vaultwarden Instance"));
        assert!(en_fragment.contains("Container Resource Usage"));
        assert!(en_fragment.contains("Host Machine Resource Usage"));
        assert!(en_fragment.contains("Container CPU"));
        assert!(en_fragment.contains("Container RAM"));
        assert!(en_fragment.contains("CPU Usage"));
        assert!(en_fragment.contains("RAM Memory"));
        assert!(en_fragment.contains("Storage"));
    }

    #[tokio::test]
    async fn test_set_language_handler() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let container_service = Arc::new(ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string()));
        let system_service = Arc::new(SystemService::new(Arc::new(MockSystemMonitor)));
        let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
        let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
        let i18n = Arc::new(crate::i18n::I18n::load("pt-BR"));
        let state = AppState {
            container_service,
            system_service,
            auth_service,
            backup_service: create_test_backup_service(),
            cookie_key,
            i18n,
            rp_origin: "http://localhost:3000".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        };

        let router = create_router(state);

        // Switch to English
        let req = Request::builder()
            .uri("/set-lang?lang=en")
            .body(Body::empty())
            .unwrap();
        let res = router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let set_cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
        assert!(set_cookie.contains("komainu_lang="));

        // Switch back to Portuguese
        let req_pt = Request::builder()
            .uri("/set-lang?lang=pt-BR")
            .body(Body::empty())
            .unwrap();
        let res_pt = router.clone().oneshot(req_pt).await.unwrap();
        assert_eq!(res_pt.status(), StatusCode::SEE_OTHER);
        let set_cookie_pt = res_pt.headers().get("set-cookie").unwrap().to_str().unwrap();
        assert!(set_cookie_pt.contains("komainu_lang="));

        // Safe relative redirect
        let req_safe = Request::builder()
            .uri("/set-lang?lang=en&redirect=/dashboard")
            .body(Body::empty())
            .unwrap();
        let res_safe = router.clone().oneshot(req_safe).await.unwrap();
        assert_eq!(res_safe.status(), StatusCode::SEE_OTHER);
        assert_eq!(res_safe.headers().get("location").unwrap().to_str().unwrap(), "/dashboard");

        // Malicious protocol-relative redirect rejected -> falls back to "/"
        let req_evil = Request::builder()
            .uri("/set-lang?lang=en&redirect=//evil.com")
            .body(Body::empty())
            .unwrap();
        let res_evil = router.oneshot(req_evil).await.unwrap();
        assert_eq!(res_evil.status(), StatusCode::SEE_OTHER);
        assert_eq!(res_evil.headers().get("location").unwrap().to_str().unwrap(), "/");
    }

    #[tokio::test]
    async fn test_qr_code_authentication_flow() {
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use tower::ServiceExt;

        let container_service = Arc::new(ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string()));
        let system_service = Arc::new(SystemService::new(Arc::new(MockSystemMonitor)));
        let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
        let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
        let i18n = Arc::new(crate::i18n::I18n::load("pt-BR"));
        let state = AppState {
            container_service,
            system_service,
            auth_service: auth_service.clone(),
            backup_service: create_test_backup_service(),
            cookie_key,
            i18n,
            rp_origin: "http://localhost:3000".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        };

        let router = create_router(state);

        // 1. Iniciar sessão QR
        let req_start = Request::builder()
            .uri("/api/auth/qr_start?origin=http://localhost:3000")
            .body(Body::empty())
            .unwrap();
        let res_start = router.clone().oneshot(req_start).await.unwrap();
        assert_eq!(res_start.status(), StatusCode::OK);
        let body_bytes = to_bytes(res_start.into_body(), usize::MAX).await.unwrap();
        let start_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let qr_id_str = start_json["qr_id"].as_str().unwrap();
        let qr_id: Uuid = qr_id_str.parse().unwrap();
        let qr_url = start_json["qr_url"].as_str().unwrap();
        let svg = start_json["svg"].as_str().unwrap();
        assert!(qr_url.contains("/qr-login?token="));
        assert!(svg.contains("<svg"));

        // 2. Polling enquanto pendente
        let req_poll1 = Request::builder()
            .uri(format!("/api/auth/qr_poll?id={qr_id}"))
            .body(Body::empty())
            .unwrap();
        let res_poll1 = router.clone().oneshot(req_poll1).await.unwrap();
        assert_eq!(res_poll1.status(), StatusCode::OK);
        let poll1_bytes = to_bytes(res_poll1.into_body(), usize::MAX).await.unwrap();
        let poll1_json: serde_json::Value = serde_json::from_slice(&poll1_bytes).unwrap();
        assert_eq!(poll1_json["status"], "pending");

        // 3. Celular acessa a página do QR code
        let req_mobile = Request::builder()
            .uri(format!("/qr-login?token={qr_id}"))
            .body(Body::empty())
            .unwrap();
        let res_mobile = router.clone().oneshot(req_mobile).await.unwrap();
        assert_eq!(res_mobile.status(), StatusCode::OK);

        // 4. Polling agora deve retornar "scanned"
        let req_poll2 = Request::builder()
            .uri(format!("/api/auth/qr_poll?id={qr_id}"))
            .body(Body::empty())
            .unwrap();
        let res_poll2 = router.clone().oneshot(req_poll2).await.unwrap();
        let poll2_bytes = to_bytes(res_poll2.into_body(), usize::MAX).await.unwrap();
        let poll2_json: serde_json::Value = serde_json::from_slice(&poll2_bytes).unwrap();
        assert_eq!(poll2_json["status"], "scanned");

        // 5. Celular aprova o login com biometria
        let now = 1700000000u64;
        assert!(auth_service.approve_qr_session(qr_id, now).await);

        // 6. Polling detecta aprovação e emite cookie de sessão
        let req_poll3 = Request::builder()
            .uri(format!("/api/auth/qr_poll?id={qr_id}"))
            .body(Body::empty())
            .unwrap();
        let res_poll3 = router.clone().oneshot(req_poll3).await.unwrap();
        assert_eq!(res_poll3.status(), StatusCode::OK);
        let cookie_header = res_poll3.headers().get("set-cookie").unwrap().to_str().unwrap();
        assert!(cookie_header.contains("komainu_session="));
        let poll3_bytes = to_bytes(res_poll3.into_body(), usize::MAX).await.unwrap();
        let poll3_json: serde_json::Value = serde_json::from_slice(&poll3_bytes).unwrap();
        assert_eq!(poll3_json["status"], "approved");

        // 7. Próximo polling deve retornar "expired" pois a sessão foi consumida
        let req_poll4 = Request::builder()
            .uri(format!("/api/auth/qr_poll?id={qr_id}"))
            .body(Body::empty())
            .unwrap();
        let res_poll4 = router.clone().oneshot(req_poll4).await.unwrap();
        assert_eq!(res_poll4.status(), StatusCode::OK);
        let poll4_bytes = to_bytes(res_poll4.into_body(), usize::MAX).await.unwrap();
        let poll4_json: serde_json::Value = serde_json::from_slice(&poll4_bytes).unwrap();
        assert_eq!(poll4_json["status"], "expired");
    }

    #[tokio::test]
    async fn test_qr_mobile_error_handling() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let container_service = Arc::new(ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string()));
        let system_service = Arc::new(SystemService::new(Arc::new(MockSystemMonitor)));
        let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
        let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
        let i18n = Arc::new(crate::i18n::I18n::load("pt-BR"));
        let state = AppState {
            container_service,
            system_service,
            auth_service,
            backup_service: create_test_backup_service(),
            cookie_key,
            i18n,
            rp_origin: "http://localhost:3000".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        };

        let router = create_router(state);

        // 1. qr_mobile_start sem parâmetro token -> 400 Bad Request
        let req_no_token = Request::builder()
            .uri("/api/auth/qr_mobile_start")
            .body(Body::empty())
            .unwrap();
        let res_no_token = router.clone().oneshot(req_no_token).await.unwrap();
        assert_eq!(res_no_token.status(), StatusCode::BAD_REQUEST);

        // 2. qr_mobile_start com token inválido -> 400 Bad Request
        let req_invalid = Request::builder()
            .uri("/api/auth/qr_mobile_start?token=not-a-uuid")
            .body(Body::empty())
            .unwrap();
        let res_invalid = router.clone().oneshot(req_invalid).await.unwrap();
        assert_eq!(res_invalid.status(), StatusCode::BAD_REQUEST);

        // 3. qr_poll sem id -> 400 Bad Request
        let req_poll_no_id = Request::builder()
            .uri("/api/auth/qr_poll")
            .body(Body::empty())
            .unwrap();
        let res_poll_no_id = router.oneshot(req_poll_no_id).await.unwrap();
        assert_eq!(res_poll_no_id.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_qr_origin_sanitization() {
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use tower::ServiceExt;

        let container_service = Arc::new(ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string()));
        let system_service = Arc::new(SystemService::new(Arc::new(MockSystemMonitor)));
        let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
        let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
        let i18n = Arc::new(crate::i18n::I18n::load("pt-BR"));
        let state = AppState {
            container_service,
            system_service,
            auth_service,
            backup_service: create_test_backup_service(),
            cookie_key,
            i18n,
            rp_origin: "http://localhost:3000".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        };

        let router = create_router(state);

        // Attempt to pass an arbitrary external origin
        let req_evil = Request::builder()
            .uri("/api/auth/qr_start?origin=https://evil.com")
            .body(Body::empty())
            .unwrap();
        let res_evil = router.oneshot(req_evil).await.unwrap();
        assert_eq!(res_evil.status(), StatusCode::OK);
        let bytes = to_bytes(res_evil.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let qr_url = json["qr_url"].as_str().unwrap();

        // Must reject evil.com and use configured rp_origin
        assert!(!qr_url.contains("evil.com"));
        assert!(qr_url.starts_with("http://localhost:3000/qr-login?token="));
    }

    #[tokio::test]
    async fn test_backups_page_and_navbar() {
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use tower::ServiceExt;

        let container_service = Arc::new(ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string()));
        let system_service = Arc::new(SystemService::new(Arc::new(MockSystemMonitor)));
        let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
        let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
        let i18n = Arc::new(crate::i18n::I18n::load("pt-BR"));
        let state = AppState {
            container_service,
            system_service,
            auth_service,
            backup_service: create_test_backup_service(),
            cookie_key: cookie_key.clone(),
            i18n: i18n.clone(),
            rp_origin: "http://localhost:3000".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        };

        let router = create_router(state);

        // 1. Unauthenticated -> 303/307 Redirect to /login
        let req_unauth = Request::builder()
            .uri("/backups")
            .body(Body::empty())
            .unwrap();
        let res_unauth = router.clone().oneshot(req_unauth).await.unwrap();
        assert_eq!(res_unauth.status(), StatusCode::SEE_OTHER);
        assert_eq!(res_unauth.headers().get("location").unwrap().to_str().unwrap(), "/login");

        // 2. Authenticated -> 200 OK with navbar containing Home and Backups
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let jar = SignedCookieJar::new(cookie_key);
        let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}"));
        cookie.set_path("/");
        let jar = jar.add(cookie);
        let cookie_res = (jar, StatusCode::OK).into_response();
        let set_cookie = cookie_res.headers().get("set-cookie").unwrap().to_str().unwrap();
        let cookie_str = set_cookie.split(';').next().unwrap();

        let req_auth = Request::builder()
            .uri("/backups")
            .header("cookie", cookie_str)
            .body(Body::empty())
            .unwrap();
        let res_auth = router.oneshot(req_auth).await.unwrap();
        assert_eq!(res_auth.status(), StatusCode::OK);
        let bytes = to_bytes(res_auth.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains("Home"));
        assert!(body.contains("Backups"));
        assert!(body.contains("Cópias de Segurança (Backups)"));
        assert!(body.contains("Configuração da Rotina de Backup"));
    }

    #[tokio::test]
    async fn test_backup_api_endpoints() {
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use tower::ServiceExt;

        let container_service = Arc::new(ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string()));
        let system_service = Arc::new(SystemService::new(Arc::new(MockSystemMonitor)));
        let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
        let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
        let i18n = Arc::new(crate::i18n::I18n::load("pt-BR"));
        let state = AppState {
            container_service,
            system_service,
            auth_service,
            backup_service: create_test_backup_service(),
            cookie_key: cookie_key.clone(),
            i18n: i18n.clone(),
            rp_origin: "http://localhost:3000".to_string(),
            pcloud_client_id: String::new(),
            pcloud_client_secret: String::new(),
        };

        let router = create_router(state.clone());

        // Preset an initial pCloud token to simulate existing connected account
        let mut initial_cfg = state.backup_service.get_config().await.unwrap_or_default();
        initial_cfg.pcloud_token = "test_tok".to_string();
        state.backup_service.save_config(&initial_cfg).await.unwrap();

        // 1. Unauthenticated POST /api/backups/config -> 401
        let req_unauth = Request::builder()
            .uri("/api/backups/config")
            .method("POST")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("enabled=on&cron_expression=0+3+*+*+*"))
            .unwrap();
        let res_unauth = router.clone().oneshot(req_unauth).await.unwrap();
        assert_eq!(res_unauth.status(), StatusCode::UNAUTHORIZED);

        // 2. Authenticated POST /api/backups/config -> 200 with success banner
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let jar = SignedCookieJar::new(cookie_key);
        let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}"));
        cookie.set_path("/");
        let jar = jar.add(cookie);
        let cookie_res = (jar, StatusCode::OK).into_response();
        let set_cookie = cookie_res.headers().get("set-cookie").unwrap().to_str().unwrap();
        let cookie_str = set_cookie.split(';').next().unwrap();

        let req_save = Request::builder()
            .uri("/api/backups/config")
            .method("POST")
            .header("cookie", cookie_str)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("enabled=on&cron_expression=0+4+*+*+*&pcloud_region=EU&pcloud_client_id=test_cid&pcloud_client_secret=test_sec&pcloud_folder=12345&encryption_password=strong_password"))
            .unwrap();
        let res_save = router.clone().oneshot(req_save).await.unwrap();
        assert_eq!(res_save.status(), StatusCode::OK);
        let bytes = to_bytes(res_save.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains("Configurações de backup salvas com sucesso!"));
        assert!(body.contains("0 4 * * *"));

        // 3. Save config with empty credentials - existing ones should be preserved
        let req_save_blank = Request::builder()
            .uri("/api/backups/config")
            .method("POST")
            .header("cookie", cookie_str)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("enabled=on&cron_expression=0+5+*+*+*&pcloud_region=EU&pcloud_client_id=&pcloud_client_secret=&pcloud_folder=12345&encryption_password="))
            .unwrap();
        let res_save_blank = router.clone().oneshot(req_save_blank).await.unwrap();
        assert_eq!(res_save_blank.status(), StatusCode::OK);
        let saved_cfg = state.backup_service.get_config().await.unwrap();
        assert_eq!(saved_cfg.encryption_password, "strong_password");
        assert_eq!(saved_cfg.pcloud_client_id, "test_cid");
        assert_eq!(saved_cfg.pcloud_client_secret, "test_sec");
        assert_eq!(saved_cfg.pcloud_token, "test_tok");
        assert_eq!(saved_cfg.cron_expression, "0 5 * * *");

        // 4. Authenticated GET /api/backups/fragment -> 200
        let req_frag = Request::builder()
            .uri("/api/backups/fragment")
            .header("cookie", cookie_str)
            .body(Body::empty())
            .unwrap();
        let res_frag = router.clone().oneshot(req_frag).await.unwrap();
        assert_eq!(res_frag.status(), StatusCode::OK);
        let frag_bytes = to_bytes(res_frag.into_body(), usize::MAX).await.unwrap();
        let frag_body = String::from_utf8(frag_bytes.to_vec()).unwrap();
        assert!(frag_body.contains("Cópias de Segurança (Backups)"));
        assert!(!frag_body.contains("strong_password"));
        assert!(!frag_body.contains("test_sec"));
        assert!(!frag_body.contains("test_tok"));
        assert!(frag_body.contains("••••••••••••"));
        assert!(frag_body.contains("Conectado"));

        // 5. Test OAuth authorize redirect
        let req_auth_oauth = Request::builder()
            .uri("/api/backups/pcloud/authorize")
            .header("cookie", cookie_str)
            .body(Body::empty())
            .unwrap();
        let res_auth_oauth = router.clone().oneshot(req_auth_oauth).await.unwrap();
        assert_eq!(res_auth_oauth.status(), StatusCode::SEE_OTHER);
        let loc = res_auth_oauth.headers().get("location").unwrap().to_str().unwrap();
        assert!(loc.starts_with("https://my.pcloud.com/oauth2/authorize?client_id=test_cid"));
        assert!(loc.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3000%2Fapi%2Fbackups%2Fpcloud%2Fcallback"));

        // 6. Test OAuth disconnect
        let req_disc = Request::builder()
            .uri("/api/backups/pcloud/disconnect")
            .method("POST")
            .header("cookie", cookie_str)
            .body(Body::empty())
            .unwrap();
        let res_disc = router.oneshot(req_disc).await.unwrap();
        assert_eq!(res_disc.status(), StatusCode::OK);
        let disc_bytes = to_bytes(res_disc.into_body(), usize::MAX).await.unwrap();
        let disc_body = String::from_utf8(disc_bytes.to_vec()).unwrap();
        assert!(disc_body.contains(&state.i18n.backups.pcloud_not_connected));
        let updated_cfg = state.backup_service.get_config().await.unwrap();
        assert_eq!(updated_cfg.pcloud_token, "");
    }
}
