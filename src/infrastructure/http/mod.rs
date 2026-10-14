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

#[derive(Clone)]
pub struct AppState {
    pub container_service: Arc<ContainerService>,
    pub system_service: Arc<SystemService>,
    pub auth_service: Arc<AuthService>,
    pub cookie_key: Key,
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
        .with_state(state)
}

fn is_authenticated(jar: &SignedCookieJar) -> bool {
    jar.get(SESSION_COOKIE)
        .map(|c| c.value() == "admin")
        .unwrap_or(false)
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

    let has_admin = state.auth_service.has_admin().await;
    let warning_box = if !has_admin {
        r##"<div class="mb-6 p-4 rounded-xl bg-amber-500/10 border border-amber-500/20 text-amber-400 text-sm">
            <strong>Atenção:</strong> Nenhum administrador registrado. Execute <code>./komainu --register</code> no terminal para gerar a chave de acesso.
        </div>"##
    } else {
        ""
    };

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="pt-BR" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Login - Komainu</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script src="https://unpkg.com/@github/webauthn-json@2.1.1/dist/browser-global/webauthn-json.browser-global.min.js"></script>
</head>
<body class="bg-slate-950 text-slate-100 min-h-screen flex items-center justify-center p-4">
    <div class="max-w-md w-full bg-slate-900 border border-slate-800 rounded-2xl p-8 shadow-2xl">
        <div class="flex items-center gap-3 mb-6">
            <div class="w-10 h-10 rounded-xl bg-blue-600 flex items-center justify-center font-black text-2xl shadow-lg shadow-blue-500/20">
                狛
            </div>
            <div>
                <h1 class="text-xl font-bold tracking-tight">Komainu</h1>
                <p class="text-xs text-slate-400">Autenticação por Passkey</p>
            </div>
        </div>

        {warning_box}

        <p class="text-sm text-slate-300 mb-6">
            Acesso administrativo exclusivo protegido por WebAuthn (Passkeys). Utilize sua chave física ou biometria.
        </p>

        <div id="error-msg" class="hidden mb-4 p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 text-sm"></div>

        <button 
            id="login-btn"
            onclick="startLogin()"
            class="w-full py-3 px-4 rounded-xl text-sm font-semibold transition shadow-lg flex items-center justify-center gap-2 bg-blue-600 hover:bg-blue-500 active:scale-95 text-white">
            <svg class="w-5 h-5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"/></svg>
            Entrar com Passkey
        </button>
    </div>

    <script>
    async function startLogin() {{
        const btn = document.getElementById('login-btn');
        const errDiv = document.getElementById('error-msg');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = 'Aguardando Passkey...';

        try {{
            const startRes = await fetch('/api/auth/login_start');
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || 'Erro ao iniciar login');
            }}
            const data = await startRes.json();
            const credential = await webauthnJSON.get({{ publicKey: data.challenge }});

            const finishRes = await fetch('/api/auth/login_finish', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ auth_id: data.auth_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || 'Falha ao autenticar Passkey');
            }}

            window.location.href = '/';
        }} catch (err) {{
            console.error(err);
            errDiv.innerText = err.message || 'Erro durante a autenticação.';
            errDiv.classList.remove('hidden');
            btn.disabled = false;
            btn.innerText = 'Entrar com Passkey';
        }}
    }}
    </script>
</body>
</html>"##,
        warning_box = warning_box
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
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
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
            let mut cookie = Cookie::new(SESSION_COOKIE, "admin");
            cookie.set_path("/");
            cookie.set_http_only(true);
            cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);

            let jar = jar.add(cookie);
            (jar, "Autenticado com sucesso").into_response()
        }
        Err(e) => (StatusCode::UNAUTHORIZED, e.to_string()).into_response(),
    }
}

async fn logout_handler(jar: SignedCookieJar) -> Response {
    let jar = jar.remove(Cookie::from(SESSION_COOKIE));
    (jar, Redirect::to("/login")).into_response()
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
    let token = match query.token {
        Some(t) => t,
        None => return (StatusCode::FORBIDDEN, "Token ausente").into_response(),
    };

    if !state.auth_service.is_valid_reg_token(&token).await {
        return (StatusCode::FORBIDDEN, "Token de registro inválido ou expirado.").into_response();
    }

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="pt-BR" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Registrar Passkey - Komainu</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script src="https://unpkg.com/@github/webauthn-json@2.1.1/dist/browser-global/webauthn-json.browser-global.min.js"></script>
</head>
<body class="bg-slate-950 text-slate-100 min-h-screen flex items-center justify-center p-4">
    <div class="max-w-md w-full bg-slate-900 border border-slate-800 rounded-2xl p-8 shadow-2xl">
        <div class="flex items-center gap-3 mb-6">
            <div class="w-10 h-10 rounded-xl bg-emerald-600 flex items-center justify-center font-black text-2xl shadow-lg shadow-emerald-500/20">
                狛
            </div>
            <div>
                <h1 class="text-xl font-bold tracking-tight">Cadastro de Administrador</h1>
                <p class="text-xs text-slate-400">Sessão temporária autorizada via terminal</p>
            </div>
        </div>

        <p class="text-sm text-slate-300 mb-6">
            Cadastre a Passkey do seu dispositivo para assumir o controle do Komainu. Este link é de uso único.
        </p>

        <div id="error-msg" class="hidden mb-4 p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 text-sm"></div>

        <button 
            id="reg-btn"
            onclick="startRegistration('{token}')"
            class="w-full py-3 px-4 rounded-xl text-sm font-semibold transition shadow-lg flex items-center justify-center gap-2 bg-emerald-600 hover:bg-emerald-500 active:scale-95 text-white">
            <svg class="w-5 h-5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
            Criar Passkey do Administrador
        </button>
    </div>

    <script>
    async function startRegistration(token) {{
        const btn = document.getElementById('reg-btn');
        const errDiv = document.getElementById('error-msg');
        errDiv.classList.add('hidden');
        btn.disabled = true;
        btn.innerText = 'Criando credencial no dispositivo...';

        try {{
            const startRes = await fetch(`/api/auth/register_start?token=${{encodeURIComponent(token)}}`);
            if (!startRes.ok) {{
                const err = await startRes.text();
                throw new Error(err || 'Falha ao iniciar registro');
            }}
            const data = await startRes.json();
            const credential = await webauthnJSON.create({{ publicKey: data.challenge }});

            const finishRes = await fetch(`/api/auth/register_finish?token=${{encodeURIComponent(token)}}`, {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ reg_id: data.reg_id, credential }})
            }});

            if (!finishRes.ok) {{
                const err = await finishRes.text();
                throw new Error(err || 'Falha ao salvar Passkey');
            }}

            alert('Passkey cadastrada com sucesso! Redirecionando para o login.');
            window.location.href = '/login';
        }} catch (err) {{
            console.error(err);
            errDiv.innerText = err.message || 'Erro durante o cadastro.';
            errDiv.classList.remove('hidden');
            btn.disabled = false;
            btn.innerText = 'Criar Passkey do Administrador';
        }}
    }}
    </script>
</body>
</html>"##,
        token = token
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
        None => return (StatusCode::FORBIDDEN, "Token ausente").into_response(),
    };

    match state.auth_service.start_registration(&token).await {
        Ok((reg_id, challenge)) => Json(RegisterStartResponse { reg_id, challenge }).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
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
        None => return (StatusCode::FORBIDDEN, "Token ausente").into_response(),
    };

    if !state.auth_service.is_valid_reg_token(&token).await {
        return (StatusCode::FORBIDDEN, "Token inválido").into_response();
    }

    match state
        .auth_service
        .finish_registration(payload.reg_id, &payload.credential)
        .await
    {
        Ok(_) => (StatusCode::OK, "Passkey cadastrada com sucesso").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ==========================================
// ROTAS DO DASHBOARD (PROTEGIDAS POR SESSÃO)
// ==========================================

async fn dashboard_page(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return Redirect::to("/login").into_response();
    }

    let fragment = render_dashboard_fragment(&state).await;
    let html = format!(
        r##"<!DOCTYPE html>
<html lang="pt-BR" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Komainu - Vaultwarden Manager</title>
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
                    <h1 class="text-lg font-bold tracking-tight">Komainu</h1>
                    <p class="text-xs text-slate-400">Vaultwarden Docker Manager</p>
                </div>
            </div>
            <div class="flex items-center gap-3">
                <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                    <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                    Monitoramento Ativo
                </span>
                <a href="/logout" class="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 transition">
                    Sair
                </a>
            </div>
        </div>
    </header>

    <main class="flex-1 max-w-6xl w-full mx-auto px-4 py-8">
        <div id="dashboard-content" hx-get="/api/dashboard-fragment" hx-trigger="every 5s" hx-swap="innerHTML">
            {fragment}
        </div>
    </main>

    <footer class="border-t border-slate-800/80 py-4 text-center text-xs text-slate-500">
        Komainu Monolith &bull; Rust + HTMX + Tailwind CSS
    </footer>
</body>
</html>"##,
        fragment = fragment
    );
    Html(html).into_response()
}

async fn dashboard_fragment_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        let mut headers = HeaderMap::new();
        headers.insert("HX-Redirect", "/login".parse().unwrap());
        return (StatusCode::UNAUTHORIZED, headers, "Sessão expirada").into_response();
    }
    Html(render_dashboard_fragment(&state).await).into_response()
}

async fn container_start_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, "Não autorizado").into_response();
    }
    let _ = state.container_service.start().await;
    Html(render_dashboard_fragment(&state).await).into_response()
}

async fn container_stop_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, "Não autorizado").into_response();
    }
    let _ = state.container_service.stop().await;
    Html(render_dashboard_fragment(&state).await).into_response()
}

async fn container_restart_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, "Não autorizado").into_response();
    }
    let _ = state.container_service.restart().await;
    Html(render_dashboard_fragment(&state).await).into_response()
}

async fn container_update_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, "Não autorizado").into_response();
    }
    let _ = state.container_service.update_image().await;
    Html(render_dashboard_fragment(&state).await).into_response()
}

async fn container_rebuild_handler(jar: SignedCookieJar, State(state): State<AppState>) -> Response {
    if !is_authenticated(&jar) {
        return (StatusCode::UNAUTHORIZED, "Não autorizado").into_response();
    }
    let _ = state.container_service.rebuild().await;
    Html(render_dashboard_fragment(&state).await).into_response()
}

async fn render_dashboard_fragment(state: &AppState) -> String {
    let status_res = state.container_service.status().await;
    let metrics = state.system_service.get_metrics().await;

    let (status_badge, image_name, uptime_str, container_cpu_str, container_ram_str) = match &status_res {
        Ok(status) => {
            let badge = match status.state {
                ContainerState::Running => {
                    r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 flex items-center gap-1.5">
                        <span class="w-2 h-2 rounded-full bg-emerald-400"></span> Rodando
                    </span>"##
                }
                ContainerState::Stopped => {
                    r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-rose-500/20 text-rose-400 border border-rose-500/30 flex items-center gap-1.5">
                        <span class="w-2 h-2 rounded-full bg-rose-400"></span> Parado
                    </span>"##
                }
                ContainerState::Restarting => {
                    r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-amber-500/20 text-amber-400 border border-amber-500/30 flex items-center gap-1.5">
                        <span class="w-2 h-2 rounded-full bg-amber-400 animate-spin"></span> Reiniciando
                    </span>"##
                }
                _ => {
                    r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-slate-700 text-slate-300">
                        Desconhecido
                    </span>"##
                }
            };
            (
                badge.to_string(),
                status.image.clone(),
                status.uptime.clone().unwrap_or_else(|| "N/A".to_string()),
                status.cpu_usage_pct.map(|c| format!("{c:.1}%")).unwrap_or_else(|| "N/A".to_string()),
                status.memory_used_mb.map(|m| format!("{m} MB")).unwrap_or_else(|| "N/A".to_string()),
            )
        }
        Err(e) => (
            format!(
                r##"<span class="px-3 py-1 rounded-full text-xs font-semibold bg-rose-500/20 text-rose-400 border border-rose-500/30">
                    Erro: {}
                </span>"##,
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
    <!-- Card do Container -->
    <div class="lg:col-span-3 bg-slate-900/60 border border-slate-800 rounded-2xl p-6 shadow-xl backdrop-blur">
        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-slate-800">
            <div>
                <div class="flex items-center gap-3">
                    <h2 class="text-xl font-bold tracking-tight">Instância Vaultwarden</h2>
                    {status_badge}
                </div>
                <p class="text-sm text-slate-400 mt-1 font-mono text-xs">Imagem: {image_name} &bull; Início: {uptime_str}</p>
            </div>
            
            <div class="flex flex-wrap items-center gap-2">
                <button 
                    hx-post="/api/container/start" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    {disabled_start}
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-emerald-600 hover:bg-emerald-500 active:scale-95 disabled:opacity-40 disabled:cursor-not-allowed text-white">
                    <svg class="w-3.5 h-3.5 fill-current" viewBox="0 0 20 20"><path d="M6.3 2.841A1.5 1.5 0 004 4.11v11.78a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z"/></svg>
                    Iniciar
                </button>
                <button 
                    hx-post="/api/container/stop" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    {disabled_stop}
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-rose-600 hover:bg-rose-500 active:scale-95 disabled:opacity-40 disabled:cursor-not-allowed text-white">
                    <svg class="w-3.5 h-3.5 fill-current" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M4.5 4.5A1.5 1.5 0 016 3h8a1.5 1.5 0 011.5 1.5v11a1.5 1.5 0 01-1.5 1.5H6a1.5 1.5 0 01-1.5-1.5v-11z" clip-rule="evenodd"/></svg>
                    Parar
                </button>
                <button 
                    hx-post="/api/container/restart" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-amber-600 hover:bg-amber-500 active:scale-95 text-white">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/></svg>
                    Reiniciar
                </button>
                <button 
                    hx-post="/api/container/update" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-sky-600 hover:bg-sky-500 active:scale-95 text-white">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/></svg>
                    Atualizar Imagem
                </button>
                <button 
                    hx-post="/api/container/rebuild" 
                    hx-target="#dashboard-content" 
                    hx-swap="innerHTML"
                    hx-confirm="Deseja atualizar a imagem e recriar o container Vaultwarden preservando os volumes?"
                    class="px-3.5 py-2 rounded-xl text-xs font-medium transition shadow-sm flex items-center gap-1.5 bg-indigo-600 hover:bg-indigo-500 active:scale-95 text-white">
                    <svg class="w-3.5 h-3.5 fill-none stroke-current stroke-2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"/></svg>
                    Reconstruir
                </button>
            </div>
        </div>
    </div>

    <!-- Métrica CPU -->
    <div class="bg-slate-900/60 border border-slate-800 rounded-2xl p-6 shadow-xl backdrop-blur flex flex-col justify-between">
        <div>
            <div class="flex items-center justify-between mb-4">
                <span class="text-sm font-medium text-slate-400">Uso de CPU</span>
                <span class="text-xs font-mono font-semibold px-2 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/20">{cpu_usage:.1}%</span>
            </div>
            <div class="w-full bg-slate-800 h-2.5 rounded-full overflow-hidden">
                <div class="bg-blue-500 h-2.5 rounded-full transition-all duration-500" style="width: {cpu_usage:.1}%"></div>
            </div>
        </div>
        <div class="mt-4 pt-3 border-t border-slate-800/60 flex items-center justify-between text-xs">
            <span class="text-slate-500">Container Vaultwarden</span>
            <span class="font-mono font-semibold text-blue-400">{container_cpu_str}</span>
        </div>
    </div>

    <!-- Métrica RAM -->
    <div class="bg-slate-900/60 border border-slate-800 rounded-2xl p-6 shadow-xl backdrop-blur flex flex-col justify-between">
        <div>
            <div class="flex items-center justify-between mb-4">
                <span class="text-sm font-medium text-slate-400">Memória RAM</span>
                <span class="text-xs font-mono font-semibold px-2 py-0.5 rounded bg-purple-500/10 text-purple-400 border border-purple-500/20">{ram_used} / {ram_total} MB</span>
            </div>
            <div class="w-full bg-slate-800 h-2.5 rounded-full overflow-hidden">
                <div class="bg-purple-500 h-2.5 rounded-full transition-all duration-500" style="width: {ram_pct:.1}%"></div>
            </div>
        </div>
        <div class="mt-4 pt-3 border-t border-slate-800/60 flex items-center justify-between text-xs">
            <span class="text-slate-500">Container Vaultwarden</span>
            <span class="font-mono font-semibold text-purple-400">{container_ram_str}</span>
        </div>
    </div>

    <!-- Métrica Disco -->
    <div class="bg-slate-900/60 border border-slate-800 rounded-2xl p-6 shadow-xl backdrop-blur flex flex-col justify-between">
        <div>
            <div class="flex items-center justify-between mb-4">
                <span class="text-sm font-medium text-slate-400">Armazenamento</span>
                <span class="text-xs font-mono font-semibold px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">{disk_used} / {disk_total} GB</span>
            </div>
            <div class="w-full bg-slate-800 h-2.5 rounded-full overflow-hidden">
                <div class="bg-emerald-500 h-2.5 rounded-full transition-all duration-500" style="width: {disk_pct:.1}%"></div>
            </div>
        </div>
        <div class="mt-4 pt-3 border-t border-slate-800/60 flex items-center justify-between text-xs">
            <span class="text-slate-500">Servidor Host</span>
            <span class="font-mono font-semibold text-emerald-400">{disk_pct:.1}% ocupado</span>
        </div>
    </div>
</div>"##,
        status_badge = status_badge,
        image_name = image_name,
        uptime_str = uptime_str,
        disabled_start = if is_running { "disabled" } else { "" },
        disabled_stop = if !is_running { "disabled" } else { "" },
        cpu_usage = metrics.cpu_usage_pct,
        container_cpu_str = container_cpu_str,
        ram_used = metrics.ram_used_mb,
        ram_total = metrics.ram_total_mb,
        ram_pct = if metrics.ram_total_mb > 0 { (metrics.ram_used_mb as f32 / metrics.ram_total_mb as f32) * 100.0 } else { 0.0 },
        container_ram_str = container_ram_str,
        disk_used = metrics.disk_used_gb,
        disk_total = metrics.disk_total_gb,
        disk_pct = if metrics.disk_total_gb > 0 { (metrics.disk_used_gb as f32 / metrics.disk_total_gb as f32) * 100.0 } else { 0.0 },
    )
}

pub async fn run_server(port: u16, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
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
        let state = AppState {
            container_service,
            system_service,
            auth_service,
            cookie_key,
        };

        let fragment = render_dashboard_fragment(&state).await;
        assert!(fragment.contains("Instância Vaultwarden"));
        assert!(fragment.contains("Uso de CPU"));
        assert!(fragment.contains("Memória RAM"));
        assert!(fragment.contains("Armazenamento"));
        assert!(fragment.contains("vaultwarden:test"));
    }
}
