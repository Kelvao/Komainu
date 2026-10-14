use crate::domain::system::{BoxFuture, SystemMetrics, SystemMonitorPort};
use std::sync::Mutex;
use sysinfo::{Disks, System};

pub struct SysinfoAdapter {
    sys: Mutex<System>,
}

impl SysinfoAdapter {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys: Mutex::new(sys),
        }
    }
}

impl Default for SysinfoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemMonitorPort for SysinfoAdapter {
    fn get_metrics(&self) -> BoxFuture<'_, SystemMetrics> {
        Box::pin(async move {
            let (cpu_usage_pct, ram_total_mb, ram_used_mb) = {
                let mut sys = self.sys.lock().unwrap_or_else(|e| e.into_inner());
                sys.refresh_cpu_all();
                sys.refresh_memory();
                (
                    sys.global_cpu_usage(),
                    sys.total_memory() / (1024 * 1024),
                    sys.used_memory() / (1024 * 1024),
                )
            };

            let (disk_used_gb, disk_total_gb) = tokio::task::spawn_blocking(|| {
                let disks = Disks::new_with_refreshed_list();
                let mut disk_total_bytes = 0u64;
                let mut disk_available_bytes = 0u64;

                for disk in &disks {
                    disk_total_bytes += disk.total_space();
                    disk_available_bytes += disk.available_space();
                }

                let disk_total_gb = disk_total_bytes / (1024 * 1024 * 1024);
                let disk_used_gb = (disk_total_bytes.saturating_sub(disk_available_bytes)) / (1024 * 1024 * 1024);
                (disk_used_gb, disk_total_gb)
            })
            .await
            .unwrap_or((0, 0));

            SystemMetrics {
                cpu_usage_pct,
                ram_used_mb,
                ram_total_mb,
                disk_used_gb,
                disk_total_gb,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sysinfo_adapter_get_metrics() {
        let adapter = SysinfoAdapter::new();
        let metrics = adapter.get_metrics().await;
        // Total RAM should be > 0 on any standard Linux environment
        assert!(metrics.ram_total_mb > 0);
    }
}

