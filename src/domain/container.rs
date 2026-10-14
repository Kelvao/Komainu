use std::fmt;
use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerState {
    Running,
    Stopped,
    Restarting,
    NotFound,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStatus {
    pub name: String,
    pub state: ContainerState,
    pub image: String,
    pub uptime: Option<String>,
    pub cpu_usage_pct: Option<f32>,
    pub memory_used_mb: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerError {
    NotFound(String),
    SocketUnavailable(String),
    StartFailed(String),
    StopFailed(String),
    RestartFailed(String),
    ImagePullFailed(String),
    RebuildFailed(String),
    Infrastructure(String),
}

impl fmt::Display for ContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(name) => write!(f, "Container not found: {name}"),
            Self::SocketUnavailable(path) => write!(f, "Docker socket unreachable at: {path}"),
            Self::StartFailed(msg) => write!(f, "Failed to start container: {msg}"),
            Self::StopFailed(msg) => write!(f, "Failed to stop container: {msg}"),
            Self::RestartFailed(msg) => write!(f, "Failed to restart container: {msg}"),
            Self::ImagePullFailed(msg) => write!(f, "Failed to pull image: {msg}"),
            Self::RebuildFailed(msg) => write!(f, "Failed to rebuild container: {msg}"),
            Self::Infrastructure(msg) => write!(f, "Docker error: {msg}"),
        }
    }
}

impl std::error::Error for ContainerError {}

impl ContainerError {
    pub fn to_localized(&self, i18n: &crate::i18n::Translations) -> String {
        match self {
            Self::NotFound(name) => i18n.errors.container_not_found.replace("{name}", name),
            Self::SocketUnavailable(path) => i18n.errors.docker_socket_unavailable.replace("{path}", path),
            Self::StartFailed(msg) => format!("{}: {msg}", i18n.errors.container_start_failed),
            Self::StopFailed(msg) => format!("{}: {msg}", i18n.errors.container_stop_failed),
            Self::RestartFailed(msg) => format!("{}: {msg}", i18n.errors.container_restart_failed),
            Self::ImagePullFailed(msg) => format!("{}: {msg}", i18n.errors.image_pull_failed),
            Self::RebuildFailed(msg) => format!("{}: {msg}", i18n.errors.container_rebuild_failed),
            Self::Infrastructure(msg) => format!("{}: {msg}", i18n.errors.docker_generic),
        }
    }
}

/// Port for container lifecycle management (object-safe)
pub trait ContainerPort: Send + Sync {
    fn get_status(&self, name: &str) -> BoxFuture<'_, Result<ContainerStatus, ContainerError>>;
    fn start(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
    fn stop(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
    fn restart(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
    fn update_image(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
    fn rebuild(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::I18n;

    #[test]
    fn test_container_error_localization() {
        let i18n_pt = I18n::load("pt-BR");
        let i18n_en = I18n::load("en");

        let err = ContainerError::NotFound("vaultwarden".to_string());
        assert_eq!(err.to_localized(&i18n_pt), "Container 'vaultwarden' não foi encontrado no Docker.");
        assert_eq!(err.to_localized(&i18n_en), "Container 'vaultwarden' was not found in Docker.");

        let socket_err = ContainerError::SocketUnavailable("/var/run/docker.sock".to_string());
        assert!(socket_err.to_localized(&i18n_pt).contains("Socket do Docker inacessível"));
        assert!(socket_err.to_localized(&i18n_en).contains("Docker socket unreachable"));
    }
}
