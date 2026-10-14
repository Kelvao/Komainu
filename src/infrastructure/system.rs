use crate::domain::system::{BoxFuture, SystemMetrics, SystemMonitorPort};
use std::sync::Mutex;
use sysinfo::{Disks, System};

pub struct SysinfoAdapter {
    sys: Mutex<System>,
    disk_cache: Mutex<Option<(std::time::Instant, (u64, u64))>>,
}

impl SysinfoAdapter {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys: Mutex::new(sys),
            disk_cache: Mutex::new(None),
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

            let cached_disk = {
                let cache = self.disk_cache.lock().unwrap_or_else(|e| e.into_inner());
                if let Some((instant, metrics)) = *cache {
                    if instant.elapsed() < std::time::Duration::from_secs(30) {
                        Some(metrics)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            let (disk_used_gb, disk_total_gb) = if let Some(metrics) = cached_disk {
                metrics
            } else {
                let fresh = tokio::task::spawn_blocking(|| {
                    let disks = Disks::new_with_refreshed_list();
                    // Prioritize root filesystem "/" to reflect the primary host storage accurately
                    let root_disk = disks.iter().find(|d| d.mount_point() == std::path::Path::new("/"));
                    let (total_bytes, available_bytes) = if let Some(root) = root_disk {
                        (root.total_space(), root.available_space())
                    } else {
                        // Fallback: deduplicate by device name to prevent counting Docker overlay mounts repeatedly
                        let mut seen = std::collections::HashSet::new();
                        let mut total = 0u64;
                        let mut available = 0u64;
                        for disk in &disks {
                            let dev = disk.name().to_string_lossy().to_string();
                            if !dev.is_empty() && seen.insert(dev) {
                                total += disk.total_space();
                                available += disk.available_space();
                            }
                        }
                        (total, available)
                    };

                    let disk_total_gb = total_bytes / (1024 * 1024 * 1024);
                    let disk_used_gb = total_bytes.saturating_sub(available_bytes) / (1024 * 1024 * 1024);
                    (disk_used_gb, disk_total_gb)
                })
                .await
                .unwrap_or((0, 0));

                let mut cache = self.disk_cache.lock().unwrap_or_else(|e| e.into_inner());
                *cache = Some((std::time::Instant::now(), fresh));
                fresh
            };

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

