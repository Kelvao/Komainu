use crate::domain::system::{BoxFuture, SystemMetrics, SystemMonitorPort};
use std::fs;
use std::sync::Mutex;
use std::time::Instant;

pub struct SysinfoAdapter {
    cpu_state: Mutex<Option<(u64, u64, Instant)>>,
    disk_cache: Mutex<Option<(Instant, (u64, u64))>>,
}

impl SysinfoAdapter {
    pub fn new() -> Self {
        let initial_cpu = read_cpu_ticks().map(|(w, t)| (w, t, Instant::now()));
        Self {
            cpu_state: Mutex::new(initial_cpu),
            disk_cache: Mutex::new(None),
        }
    }
}

impl Default for SysinfoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn read_cpu_ticks() -> Option<(u64, u64)> {
    use std::io::Read;
    let mut file = fs::File::open("/proc/stat").ok()?;
    let mut buf = [0u8; 1024];
    let n = file.read(&mut buf).ok()?;
    let text = std::str::from_utf8(&buf[..n]).ok()?;
    let first_line = text.lines().next()?;
    if !first_line.starts_with("cpu ") {
        return None;
    }
    let mut parts = [0u64; 10];
    let mut count = 0;
    for token in first_line.split_whitespace().skip(1) {
        if count < parts.len()
            && let Ok(val) = token.parse::<u64>() {
                parts[count] = val;
                count += 1;
            }
    }
    if count < 4 {
        return None;
    }
    // user, nice, system, idle, iowait, irq, softirq, steal
    let idle = parts[3] + parts[4];
    let total: u64 = parts[..count].iter().sum();
    let work = total.saturating_sub(idle);
    Some((work, total))
}

fn read_mem_mb() -> (u64, u64) {
    use std::io::Read;
    let mut file = match fs::File::open("/proc/meminfo") {
        Ok(f) => f,
        Err(_) => return (0, 0),
    };
    let mut buf = [0u8; 2048];
    let n = file.read(&mut buf).unwrap_or(0);
    let text = std::str::from_utf8(&buf[..n]).unwrap_or_default();
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in text.lines() {
        if let Some(val) = line.strip_prefix("MemTotal:") {
            if let Some(num) = val.split_whitespace().next().and_then(|s| s.parse::<u64>().ok()) {
                total_kb = num;
            }
        } else if let Some(val) = line.strip_prefix("MemAvailable:")
            && let Some(num) = val.split_whitespace().next().and_then(|s| s.parse::<u64>().ok()) {
                avail_kb = num;
            }
    }
    let total_mb = total_kb / 1024;
    let used_mb = total_kb.saturating_sub(avail_kb) / 1024;
    (total_mb, used_mb)
}

fn read_disk_space() -> (u64, u64) {
    #[cfg(unix)]
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        let path = std::ffi::CString::new("/").unwrap();
        if libc::statvfs(path.as_ptr(), &mut stat) == 0 {
            let block_size = if stat.f_frsize > 0 {
                stat.f_frsize as u64
            } else {
                stat.f_bsize as u64
            };
            let total_bytes = stat.f_blocks as u64 * block_size;
            let available_bytes = stat.f_bavail as u64 * block_size;
            let total_gb = total_bytes / (1024 * 1024 * 1024);
            let used_gb = total_bytes.saturating_sub(available_bytes) / (1024 * 1024 * 1024);
            return (used_gb, total_gb);
        }
    }
    (0, 0)
}

impl SystemMonitorPort for SysinfoAdapter {
    fn get_metrics(&self) -> BoxFuture<'_, SystemMetrics> {
        Box::pin(async move {
            let cpu_usage_pct = {
                let mut state = self.cpu_state.lock().unwrap_or_else(|e| e.into_inner());
                let now = Instant::now();
                if let Some((work, total)) = read_cpu_ticks() {
                    let pct = if let Some(&(prev_work, prev_total, _)) = state.as_ref() {
                        let work_delta = work.saturating_sub(prev_work);
                        let total_delta = total.saturating_sub(prev_total);
                        if total_delta > 0 {
                            ((work_delta as f32 / total_delta as f32) * 100.0).clamp(0.0, 100.0)
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };
                    *state = Some((work, total, now));
                    pct
                } else {
                    0.0
                }
            };

            let (ram_total_mb, ram_used_mb) = read_mem_mb();

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
                let fresh = tokio::task::spawn_blocking(read_disk_space)
                    .await
                    .unwrap_or((0, 0));

                let mut cache = self.disk_cache.lock().unwrap_or_else(|e| e.into_inner());
                *cache = Some((Instant::now(), fresh));
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
        // Total RAM and disk should be > 0 on standard Linux
        assert!(metrics.ram_total_mb > 0, "Total RAM should be greater than 0");
        assert!(metrics.disk_total_gb > 0, "Total disk should be greater than 0");
    }

    #[test]
    fn test_read_cpu_and_mem() {
        let (total_mb, used_mb) = read_mem_mb();
        assert!(total_mb > 0);
        assert!(used_mb <= total_mb);

        let (used_gb, total_gb) = read_disk_space();
        assert!(total_gb > 0);
        assert!(used_gb <= total_gb);
    }
}
