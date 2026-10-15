use std::net::SocketAddr;
use std::sync::Arc;
use axum_extra::extract::cookie::Key;
use tracing::info;

use crate::application::auth_service::AuthService;
use crate::domain::container::ContainerPort;
use crate::domain::system::SystemMonitorPort;

pub mod handlers;
pub mod rate_limit;
pub mod routes;
pub mod session;
pub mod theme;
pub mod views;

#[cfg(test)]
mod tests;

pub use routes::create_router;
pub use session::derive_cookie_key;

#[derive(Clone)]
pub struct AppState {
    pub container_port: Arc<dyn ContainerPort>,
    pub target_container: String,
    pub system_monitor: Arc<dyn SystemMonitorPort>,
    pub auth_service: Arc<AuthService>,
    pub backup_service: Arc<crate::application::backup_service::BackupService>,
    pub cookie_key: Key,
    pub i18n: Arc<crate::i18n::Translations>,
    pub rp_origin: String,
    pub pcloud_client_id: String,
    pub pcloud_client_secret: String,
}

impl axum::extract::FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
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
