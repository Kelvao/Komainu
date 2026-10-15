use super::*;
use super::handlers::*;
use super::session::*;
use super::views::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum_extra::extract::cookie::{Cookie, SignedCookieJar};
use tower::ServiceExt;
use uuid::Uuid;
use crate::domain::auth::{AuthError, CredentialStorePort};
use crate::domain::container::{BoxFuture, ContainerError, ContainerPort, ContainerState, ContainerStatus};
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
                swap_used_mb: 0,
                swap_total_mb: 2048,
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
use crate::domain::backup::{BackupConfig, BackupConfigStorePort, BackupError, BackupRecord, PCloudClientPort, PCloudRegion, PCloudRemoteFile};
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
        _file_path: &std::path::Path,
    ) -> BoxFuture<'_, Result<(), BackupError>> {
        Box::pin(async move { Ok(()) })
    }

    fn list_folder(
        &self,
        _region: &PCloudRegion,
        _token: &str,
        _folder: &str,
    ) -> BoxFuture<'_, Result<Vec<PCloudRemoteFile>, BackupError>> {
        Box::pin(async move {
            Ok(vec![PCloudRemoteFile {
                fileid: 123456,
                name: "vaultwarden_backup_20261014_220000.zip.enc".to_string(),
                size_bytes: 2048,
                modified: "Sat, 14 Oct 2026 22:00:00 +0000".to_string(),
            }])
        })
    }

    fn download_file(
        &self,
        _region: &PCloudRegion,
        _token: &str,
        _fileid: u64,
        dest_path: &std::path::Path,
    ) -> BoxFuture<'_, Result<(), BackupError>> {
        let dest_path = dest_path.to_path_buf();
        Box::pin(async move {
            let mut zip_bytes = Vec::new();
            {
                let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
                zip.start_file("db.sqlite3", zip::write::SimpleFileOptions::default()).unwrap();
                std::io::Write::write_all(&mut zip, b"SQLITE_TEST_DATA").unwrap();
                zip.finish().unwrap();
            }
            let enc = crate::domain::backup::encrypt_aes_256_gcm(&zip_bytes, "strong_password")
                .map_err(|e| BackupError::EncryptionError(e.to_string()))?;
            std::fs::write(&dest_path, enc)
                .map_err(|e| BackupError::IoError(e.to_string()))
        })
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

fn create_test_app_state(auth_service: Arc<AuthService>, i18n_lang: &str) -> AppState {
    let cookie_key = derive_cookie_key("test_secret_key_12345678901234567890");
    AppState {
        container_port: Arc::new(MockContainerPort),
        target_container: "vaultwarden".to_string(),
        system_monitor: Arc::new(MockSystemMonitor),
        auth_service,
        backup_service: create_test_backup_service(),
        cookie_key,
        i18n: Arc::new(crate::i18n::I18n::load(i18n_lang)),
        rp_origin: "http://localhost:3000".to_string(),
        pcloud_client_id: "test_cid".to_string(),
        pcloud_client_secret: "test_csec".to_string(),
    }
}

#[tokio::test]
async fn test_health_check_handler() {
    assert_eq!(health_check().await, "OK");
}

#[tokio::test]
async fn test_webauthn_json_asset_route() {
    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let router = create_router(create_test_app_state(auth_service, "en"));

    let req = Request::builder()
        .uri("/assets/webauthn-json.js")
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "application/javascript; charset=utf-8"
    );
    assert_eq!(
        res.headers().get("cache-control").unwrap(),
        "public, max-age=31536000, immutable"
    );
}

#[tokio::test]
async fn test_theme_css_asset_route() {
    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let router = create_router(create_test_app_state(auth_service, "en"));

    let req = Request::builder()
        .uri("/assets/theme.css")
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "text/css; charset=utf-8"
    );
    assert_eq!(
        res.headers().get("cache-control").unwrap(),
        "public, max-age=31536000, immutable"
    );
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
    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let state = create_test_app_state(auth_service, "pt-BR");

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
    let en_state = create_test_app_state(state.auth_service.clone(), "en");
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

    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let state = create_test_app_state(auth_service, "pt-BR");

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

    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let state = create_test_app_state(auth_service.clone(), "pt-BR");

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

    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let state = create_test_app_state(auth_service, "pt-BR");

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

    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let state = create_test_app_state(auth_service, "pt-BR");

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

    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    auth_service.set_credential_tag("test_tag".to_string());
    let state = create_test_app_state(auth_service, "pt-BR");
    let cookie_key = state.cookie_key.clone();

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
    let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}:test_tag"));
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

    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    auth_service.set_credential_tag("test_tag".to_string());
    let state = create_test_app_state(auth_service, "pt-BR");
    let cookie_key = state.cookie_key.clone();

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

    // 1b. Stale / revoked session cookie with old credential tag -> 401
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let jar_stale = SignedCookieJar::new(cookie_key.clone());
    let mut stale_cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}:old_revoked_tag"));
    stale_cookie.set_path("/");
    let jar_stale = jar_stale.add(stale_cookie);
    let stale_res = (jar_stale, StatusCode::OK).into_response();
    let stale_cookie_str = stale_res.headers().get("set-cookie").unwrap().to_str().unwrap().split(';').next().unwrap();

    let req_stale = Request::builder()
        .uri("/api/backups/config")
        .method("POST")
        .header("cookie", stale_cookie_str)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("enabled=on&cron_expression=0+3+*+*+*"))
        .unwrap();
    let res_stale = router.clone().oneshot(req_stale).await.unwrap();
    assert_eq!(res_stale.status(), StatusCode::UNAUTHORIZED);

    // 2. Authenticated POST /api/backups/config -> 200 with success banner
    let jar = SignedCookieJar::new(cookie_key);
    let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}:test_tag"));
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

    // 5. Test OAuth authorize redirect (GET without params and POST with form body)
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

    let req_auth_post = Request::builder()
        .uri("/api/backups/pcloud/authorize")
        .method("POST")
        .header("cookie", cookie_str)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("client_id=post_cid&client_secret=post_sec"))
        .unwrap();
    let res_auth_post = router.clone().oneshot(req_auth_post).await.unwrap();
    assert_eq!(res_auth_post.status(), StatusCode::SEE_OTHER);
    let loc_post = res_auth_post.headers().get("location").unwrap().to_str().unwrap();
    assert!(loc_post.starts_with("https://my.pcloud.com/oauth2/authorize?client_id=post_cid"));

    // 6. Test OAuth disconnect
    let req_disc = Request::builder()
        .uri("/api/backups/pcloud/disconnect")
        .method("POST")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_disc = router.clone().oneshot(req_disc).await.unwrap();
    assert_eq!(res_disc.status(), StatusCode::OK);
    let disc_bytes = to_bytes(res_disc.into_body(), usize::MAX).await.unwrap();
    let disc_body = String::from_utf8(disc_bytes.to_vec()).unwrap();
    assert!(disc_body.contains(&state.i18n.backups.pcloud_not_connected));
    let updated_cfg = state.backup_service.get_config().await.unwrap();
    assert_eq!(updated_cfg.pcloud_token, "");

    // 7. Unauthenticated OAuth endpoints reject with 303 Redirect or 401
    let req_unauth_auth = Request::builder()
        .uri("/api/backups/pcloud/authorize")
        .body(Body::empty())
        .unwrap();
    let res_unauth_auth = router.clone().oneshot(req_unauth_auth).await.unwrap();
    assert_eq!(res_unauth_auth.status(), StatusCode::SEE_OTHER);
    assert_eq!(res_unauth_auth.headers().get("location").unwrap().to_str().unwrap(), "/login");

    let req_unauth_cb = Request::builder()
        .uri("/api/backups/pcloud/callback?code=abc&state=xyz")
        .body(Body::empty())
        .unwrap();
    let res_unauth_cb = router.clone().oneshot(req_unauth_cb).await.unwrap();
    assert_eq!(res_unauth_cb.status(), StatusCode::SEE_OTHER);
    assert_eq!(res_unauth_cb.headers().get("location").unwrap().to_str().unwrap(), "/login");

    let req_unauth_disc = Request::builder()
        .uri("/api/backups/pcloud/disconnect")
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let res_unauth_disc = router.clone().oneshot(req_unauth_disc).await.unwrap();
    assert_eq!(res_unauth_disc.status(), StatusCode::UNAUTHORIZED);

    // 8. OAuth callback error handling and state validation
    // Provider returned error
    let req_cb_err = Request::builder()
        .uri("/api/backups/pcloud/callback?error=access_denied&error_description=User+denied")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_cb_err = router.clone().oneshot(req_cb_err).await.unwrap();
    assert_eq!(res_cb_err.status(), StatusCode::SEE_OTHER);
    let loc_err = res_cb_err.headers().get("location").unwrap().to_str().unwrap();
    assert!(loc_err.contains("pcloud_error=access_denied"));

    // CSRF: callback with mismatched state
    let req_cb_csrf = Request::builder()
        .uri("/api/backups/pcloud/callback?code=good_code&state=forged_state")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_cb_csrf = router.clone().oneshot(req_cb_csrf).await.unwrap();
    assert_eq!(res_cb_csrf.status(), StatusCode::SEE_OTHER);
    let loc_csrf = res_cb_csrf.headers().get("location").unwrap().to_str().unwrap();
    assert!(loc_csrf.contains("pcloud_error=invalid_oauth_state"));

    // 9. Test unauthenticated GET /api/backups/download -> 401
    let req_dl_unauth = Request::builder()
        .uri("/api/backups/download?fileid=123456&filename=test.zip.enc")
        .body(Body::empty())
        .unwrap();
    let res_dl_unauth = router.clone().oneshot(req_dl_unauth).await.unwrap();
    assert_eq!(res_dl_unauth.status(), StatusCode::UNAUTHORIZED);

    // 10. Test authenticated GET /api/backups/download -> 200 OK with decrypted zip
    // First re-enable pcloud token and set encryption password
    let mut ready_cfg = state.backup_service.get_config().await.unwrap();
    ready_cfg.pcloud_token = "valid_token".to_string();
    ready_cfg.encryption_password = "strong_password".to_string();
    state.backup_service.save_config(&ready_cfg).await.unwrap();

    let req_dl_auth = Request::builder()
        .uri("/api/backups/download?fileid=123456&filename=vaultwarden_backup_20261014_220000.zip.enc")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_dl_auth = router.clone().oneshot(req_dl_auth).await.unwrap();
    assert_eq!(res_dl_auth.status(), StatusCode::OK);
    assert_eq!(
        res_dl_auth.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/zip"
    );
    let cd = res_dl_auth.headers().get("content-disposition").unwrap().to_str().unwrap();
    assert!(cd.contains("attachment; filename=\"vaultwarden_backup_20261014_220000.zip\""));

    let dl_bytes = to_bytes(res_dl_auth.into_body(), usize::MAX).await.unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(dl_bytes)).unwrap();
    let mut db_file = archive.by_name("db.sqlite3").unwrap();
    let mut db_content = Vec::new();
    std::io::Read::read_to_end(&mut db_file, &mut db_content).unwrap();
    assert_eq!(db_content, b"SQLITE_TEST_DATA");

    // 11. Test authenticated GET /api/backups/fragment lists remote files and download button
    let req_frag_remote = Request::builder()
        .uri("/api/backups/fragment")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_frag_remote = router.clone().oneshot(req_frag_remote).await.unwrap();
    assert_eq!(res_frag_remote.status(), StatusCode::OK);
    let frag_bytes = to_bytes(res_frag_remote.into_body(), usize::MAX).await.unwrap();
    let frag_html = String::from_utf8(frag_bytes.to_vec()).unwrap();
    assert!(frag_html.contains("vaultwarden_backup_20261014_220000.zip.enc"));
    assert!(frag_html.contains("/api/backups/download?fileid=123456"));
    assert!(frag_html.contains("Baixar Descriptografado"));

    // 12. Test fileid=0 returns 400 Bad Request
    let req_dl_zero = Request::builder()
        .uri("/api/backups/download?fileid=0")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_dl_zero = router.clone().oneshot(req_dl_zero).await.unwrap();
    assert_eq!(res_dl_zero.status(), StatusCode::BAD_REQUEST);

    // 13. Test filename fallback when query filename produces hidden file
    let req_dl_hidden = Request::builder()
        .uri("/api/backups/download?fileid=123456&filename=.enc")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_dl_hidden = router.oneshot(req_dl_hidden).await.unwrap();
    assert_eq!(res_dl_hidden.status(), StatusCode::OK);
    let cd_hidden = res_dl_hidden.headers().get("content-disposition").unwrap().to_str().unwrap();
    assert!(cd_hidden.contains("attachment; filename=\"backup_123456.zip\""));
}

#[tokio::test]
async fn test_logout_invalidates_session_server_side() {
    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    auth_service.set_credential_tag("active_tag".to_string());
    let state = create_test_app_state(auth_service.clone(), "pt-BR");
    let cookie_key = state.cookie_key.clone();
    let router = create_router(state);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let jar = SignedCookieJar::new(cookie_key);
    let mut cookie = Cookie::new(SESSION_COOKIE, format!("admin:{now}:active_tag"));
    cookie.set_path("/");
    let jar = jar.add(cookie);
    let cookie_res = (jar, StatusCode::OK).into_response();
    let cookie_str = cookie_res.headers().get("set-cookie").unwrap().to_str().unwrap().split(';').next().unwrap();

    // 1. Session is valid initially
    let req_before = Request::builder()
        .uri("/backups")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_before = router.clone().oneshot(req_before).await.unwrap();
    assert_eq!(res_before.status(), StatusCode::OK);

    // 2. Perform POST /logout
    let req_logout = Request::builder()
        .uri("/logout")
        .method("POST")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_logout = router.clone().oneshot(req_logout).await.unwrap();
    assert_eq!(res_logout.status(), StatusCode::SEE_OTHER);

    // 3. Captured cookie is now rejected because server invalidated the session tag
    let req_after = Request::builder()
        .uri("/backups")
        .header("cookie", cookie_str)
        .body(Body::empty())
        .unwrap();
    let res_after = router.oneshot(req_after).await.unwrap();
    assert_eq!(res_after.status(), StatusCode::SEE_OTHER);
    assert_eq!(res_after.headers().get("location").unwrap().to_str().unwrap(), "/login");
}

#[tokio::test]
async fn test_security_headers_middleware() {
    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let state = create_test_app_state(auth_service, "pt-BR");
    let router = create_router(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let headers = res.headers();
    assert_eq!(headers.get("x-frame-options").unwrap().to_str().unwrap(), "DENY");
    assert_eq!(headers.get("x-content-type-options").unwrap().to_str().unwrap(), "nosniff");
    assert_eq!(headers.get("referrer-policy").unwrap().to_str().unwrap(), "strict-origin-when-cross-origin");
    let csp = headers.get("content-security-policy").unwrap().to_str().unwrap();
    assert!(csp.contains("frame-ancestors 'none'"));
    assert!(csp.contains("default-src 'self'"));
}

#[tokio::test]
async fn test_rate_limiter_blocks_excessive_requests() {
    let limiter = crate::infrastructure::http::rate_limit::RateLimiter::new(3, std::time::Duration::from_secs(60));
    assert!(limiter.check("1.2.3.4"));
    assert!(limiter.check("1.2.3.4"));
    assert!(limiter.check("1.2.3.4"));
    assert!(!limiter.check("1.2.3.4")); // 4th request blocked
    assert!(limiter.check("5.6.7.8")); // different IP allowed
}

#[tokio::test]
async fn test_csrf_origin_validation() {
    let auth_service = Arc::new(AuthService::new("localhost", "http://localhost:3000", Arc::new(MockCredentialStore)).unwrap());
    let state = create_test_app_state(auth_service, "pt-BR");
    let router = create_router(state);

    // 1. Untrusted origin -> 403 Forbidden
    let req_evil = Request::builder()
        .uri("/api/container/stop")
        .method("POST")
        .header("origin", "https://evil-attacker.com")
        .body(Body::empty())
        .unwrap();
    let res_evil = router.clone().oneshot(req_evil).await.unwrap();
    assert_eq!(res_evil.status(), StatusCode::FORBIDDEN);

    // 2. Untrusted referer -> 403 Forbidden
    let req_evil_ref = Request::builder()
        .uri("/api/container/stop")
        .method("POST")
        .header("referer", "https://evil-attacker.com/malicious-page")
        .body(Body::empty())
        .unwrap();
    let res_evil_ref = router.clone().oneshot(req_evil_ref).await.unwrap();
    assert_eq!(res_evil_ref.status(), StatusCode::FORBIDDEN);

    // 3. Trusted origin -> not blocked by CSRF middleware (will proceed to auth check)
    let req_trusted = Request::builder()
        .uri("/api/container/stop")
        .method("POST")
        .header("origin", "http://localhost:3000")
        .body(Body::empty())
        .unwrap();
    let res_trusted = router.oneshot(req_trusted).await.unwrap();
    assert_ne!(res_trusted.status(), StatusCode::FORBIDDEN);
}




