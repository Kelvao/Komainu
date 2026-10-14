use std::sync::Arc;
use crate::domain::container::{ContainerError, ContainerPort, ContainerStatus};

pub struct ContainerService {
    container_port: Arc<dyn ContainerPort>,
    target_container: String,
}

impl ContainerService {
    pub fn new(container_port: Arc<dyn ContainerPort>, target_container: String) -> Self {
        Self {
            container_port,
            target_container,
        }
    }

    pub async fn status(&self) -> Result<ContainerStatus, ContainerError> {
        self.container_port.get_status(&self.target_container).await
    }

    pub async fn start(&self) -> Result<(), ContainerError> {
        self.container_port.start(&self.target_container).await
    }

    pub async fn stop(&self) -> Result<(), ContainerError> {
        self.container_port.stop(&self.target_container).await
    }

    pub async fn restart(&self) -> Result<(), ContainerError> {
        self.container_port.restart(&self.target_container).await
    }

    pub async fn update_image(&self) -> Result<(), ContainerError> {
        self.container_port.update_image(&self.target_container).await
    }

    pub async fn rebuild(&self) -> Result<(), ContainerError> {
        self.container_port.rebuild(&self.target_container).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::container::{BoxFuture, ContainerState};

    struct MockContainerPort;
    impl ContainerPort for MockContainerPort {
        fn get_status(&self, name: &str) -> BoxFuture<'_, Result<ContainerStatus, ContainerError>> {
            let name = name.to_string();
            Box::pin(async move {
                Ok(ContainerStatus {
                    name,
                    state: ContainerState::Running,
                    image: "vaultwarden:test".to_string(),
                    uptime: Some("Up 1m".to_string()),
                    cpu_usage_pct: Some(1.2),
                    memory_used_mb: Some(48),
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

    #[tokio::test]
    async fn test_container_service_status() {
        let service = ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string());
        let status = service.status().await.expect("Failed to get status");
        assert_eq!(status.name, "vaultwarden");
        assert_eq!(status.state, ContainerState::Running);
        assert_eq!(status.cpu_usage_pct, Some(1.2));
        assert_eq!(status.memory_used_mb, Some(48));
    }

    #[tokio::test]
    async fn test_container_service_rebuild() {
        let service = ContainerService::new(Arc::new(MockContainerPort), "vaultwarden".to_string());
        assert!(service.rebuild().await.is_ok());
    }
}
