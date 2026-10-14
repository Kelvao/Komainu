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

#[derive(Debug, Clone)]
pub enum ContainerError {
    NotFound(String),
    Infrastructure(String),
}

impl fmt::Display for ContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(name) => write!(f, "Container not found: {name}"),
            Self::Infrastructure(msg) => write!(f, "Docker error: {msg}"),
        }
    }
}

impl std::error::Error for ContainerError {}

/// Port for container lifecycle management (object-safe)
pub trait ContainerPort: Send + Sync {
    fn get_status(&self, name: &str) -> BoxFuture<'_, Result<ContainerStatus, ContainerError>>;
    fn start(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
    fn stop(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
    fn restart(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
    fn update_image(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
    fn rebuild(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>>;
}
