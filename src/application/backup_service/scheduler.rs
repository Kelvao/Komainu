use chrono::{Local, Timelike};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, info, warn};

use super::service::BackupService;

pub fn start_cron_loop(service: Arc<BackupService>) {
    tokio::spawn(async move {
        info!("Backup cron background worker started.");
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            let config = match service.config_store.load_config().await {
                Ok(c) => c,
                Err(e) => {
                    warn!("Could not read backup configuration in cron loop: {e}");
                    continue;
                }
            };

            if !config.enabled {
                continue;
            }

            // If schedule format is valid 5 or 6/7 field expression
            let schedule = match cron::Schedule::from_str(&config.cron_expression) {
                Ok(s) => s,
                Err(_) => {
                    // In case expression has 5 fields, try converting to 6-field cron by prepending "0 "
                    let normalized = format!("0 {}", config.cron_expression.trim());
                    match cron::Schedule::from_str(&normalized) {
                        Ok(s) => s,
                        Err(e) => {
                            warn!("Invalid cron expression '{}': {e}", config.cron_expression);
                            continue;
                        }
                    }
                }
            };

            let now_local = Local::now();
            let current_minute = now_local.timestamp() / 60;

            // Check if current minute should trigger using host local time
            let start_of_this_min = now_local
                .with_second(0)
                .and_then(|t| t.with_nanosecond(0))
                .unwrap_or(now_local);
            let mut should_run = false;

            let window_start = start_of_this_min
                .checked_sub_signed(chrono::Duration::seconds(1))
                .unwrap_or(start_of_this_min);

            // Look up scheduled events in this minute window
            if let Some(next) = schedule.after(&window_start).next() {
                let next_minute = next.timestamp() / 60;
                if next_minute == current_minute {
                    let mut last_min = service.last_run_minute.lock().await;
                    if *last_min != Some(current_minute) {
                        *last_min = Some(current_minute);
                        should_run = true;
                    }
                }
            }

            if should_run {
                info!(
                    "Triggering scheduled cron backup (minute={})...",
                    current_minute
                );
                let svc = service.clone();
                tokio::spawn(async move {
                    if let Err(e) = svc.execute_backup().await {
                        error!("Scheduled cron backup failed: {e}");
                    }
                });
            }
        }
    });
}

