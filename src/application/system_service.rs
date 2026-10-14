use std::sync::Arc;
use crate::domain::system::{SystemMetrics, SystemMonitorPort};

pub struct SystemService {
    monitor: Arc<dyn SystemMonitorPort>,
}

impl SystemService {
    pub fn new(monitor: Arc<dyn SystemMonitorPort>) -> Self {
        Self { monitor }
    }

    pub async fn get_metrics(&self) -> SystemMetrics {
        self.monitor.get_metrics().await
    }
}

