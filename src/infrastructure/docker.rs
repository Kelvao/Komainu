use crate::domain::container::{BoxFuture, ContainerError, ContainerPort, ContainerState, ContainerStatus};
use bollard::container::{CreateContainerOptions, RenameContainerOptions, StatsOptions};
use bollard::image::CreateImageOptions;
use bollard::Docker;
use futures_util::StreamExt;

use std::collections::HashMap;
use std::sync::Mutex;

type StatusCache = Mutex<HashMap<String, (std::time::Instant, ContainerStatus)>>;

pub struct DockerAdapter {
    socket_path: String,
    docker: Option<Docker>,
    prev_cpu: Mutex<HashMap<String, (u64, u64, std::time::Instant)>>,
    status_cache: StatusCache,
}

impl DockerAdapter {
    pub fn new(socket_path: &str) -> Self {
        let docker = Docker::connect_with_unix(socket_path, 120, bollard::API_DEFAULT_VERSION).ok();
        Self {
            socket_path: socket_path.to_string(),
            docker,
            prev_cpu: Mutex::new(HashMap::new()),
            status_cache: Mutex::new(HashMap::new()),
        }
    }

    fn invalidate_status(&self, name: &str) {
        if let Ok(mut cache) = self.status_cache.lock() {
            cache.remove(name);
        }
    }

    fn get_docker(&self) -> Result<Docker, ContainerError> {
        if let Some(ref d) = self.docker {
            return Ok(d.clone());
        }
        Docker::connect_with_unix(&self.socket_path, 120, bollard::API_DEFAULT_VERSION)
            .map_err(|_| ContainerError::SocketUnavailable(self.socket_path.clone()))
    }

    async fn pull_image(&self, docker: &Docker, image: &str) -> Result<(), ContainerError> {
        let options = Some(CreateImageOptions {
            from_image: image.to_string(),
            ..Default::default()
        });

        let mut stream = docker.create_image(options, None, None);
        while let Some(item) = stream.next().await {
            if let Err(e) = item {
                return Err(ContainerError::ImagePullFailed(e.to_string()));
            }
        }

        Ok(())
    }
}

impl ContainerPort for DockerAdapter {
    fn get_status(&self, name: &str) -> BoxFuture<'_, Result<ContainerStatus, ContainerError>> {
        let name = name.to_string();
        Box::pin(async move {
            {
                let cache = self.status_cache.lock().unwrap_or_else(|e| e.into_inner());
                if let Some((instant, status)) = cache.get(&name)
                    && instant.elapsed() < std::time::Duration::from_secs(2) {
                        return Ok(status.clone());
                    }
            }

            let docker = self.get_docker()?;
            let inspect = match docker.inspect_container(&name, None).await {
                Ok(res) => res,
                Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => {
                    return Err(ContainerError::NotFound(name));
                }
                Err(e) => return Err(ContainerError::Infrastructure(e.to_string())),
            };

            let state_info = inspect.state.as_ref();
            let status_str = state_info
                .and_then(|s| s.status.as_ref().map(|st| st.to_string()))
                .unwrap_or_default();

            let is_running = status_str.eq_ignore_ascii_case("running");
            let state = match status_str.to_lowercase().as_str() {
                "running" => ContainerState::Running,
                "exited" | "stopped" => ContainerState::Stopped,
                "restarting" => ContainerState::Restarting,
                other => ContainerState::Unknown(other.to_string()),
            };

            let image = inspect
                .config
                .as_ref()
                .and_then(|c| c.image.clone())
                .unwrap_or_else(|| "unknown".to_string());

            let started_at = state_info.and_then(|s| s.started_at.clone());

            let (cpu_usage_pct, memory_used_mb, memory_limit_mb) = if is_running {
                let mut stats_stream = docker.stats(&name, Some(StatsOptions { stream: false, one_shot: true }));
                if let Some(Ok(stats)) = stats_stream.next().await {
                    let mem_used = match stats.memory_stats.stats {
                        Some(bollard::container::MemoryStatsStats::V1(ref v1)) => {
                            let cache = if v1.total_inactive_file > 0 {
                                v1.total_inactive_file
                            } else {
                                v1.cache
                            };
                            stats.memory_stats.usage.unwrap_or(0).saturating_sub(cache)
                        }
                        Some(bollard::container::MemoryStatsStats::V2(ref v2)) => {
                            stats.memory_stats.usage.unwrap_or(0).saturating_sub(v2.inactive_file)
                        }
                        None => stats.memory_stats.usage.unwrap_or(0),
                    };
                    let mem_mb = Some(mem_used / (1024 * 1024));
                    let limit_mb = stats.memory_stats.limit.map(|l| l / (1024 * 1024));

                    let cur_cpu = stats.cpu_stats.cpu_usage.total_usage;
                    let cur_sys = stats.cpu_stats.system_cpu_usage.unwrap_or(0);
                    let online_cpus = stats.cpu_stats.online_cpus.unwrap_or_else(|| {
                        stats.cpu_stats.cpu_usage.percpu_usage.as_ref().map(|p| p.len() as u64).unwrap_or(1)
                    });
                    let now = std::time::Instant::now();

                    let (cpu_delta, sys_delta, time_delta_nanos) = {
                        let mut prev_map = self.prev_cpu.lock().unwrap_or_else(|e| e.into_inner());
                        let deltas = if let Some(&(prev_c, prev_s, prev_t)) = prev_map.get(&name) {
                            (
                                cur_cpu.saturating_sub(prev_c),
                                cur_sys.saturating_sub(prev_s),
                                now.duration_since(prev_t).as_nanos() as u64,
                            )
                        } else {
                            let precpu = stats.precpu_stats.cpu_usage.total_usage;
                            let presys = stats.precpu_stats.system_cpu_usage.unwrap_or(0);
                            (
                                cur_cpu.saturating_sub(precpu),
                                cur_sys.saturating_sub(presys),
                                0,
                            )
                        };
                        // Evita vazamento de memória em execuções prolongadas com containers dinâmicos
                        prev_map.retain(|_, (_, _, t)| now.duration_since(*t).as_secs() < 600);
                        if prev_map.len() >= 20
                            && let Some(oldest) = prev_map.iter().min_by_key(|(_, (_, _, t))| *t).map(|(k, _)| k.clone()) {
                                prev_map.remove(&oldest);
                            }
                        prev_map.insert(name.clone(), (cur_cpu, cur_sys, now));
                        deltas
                    };

                    let cpu_pct = if sys_delta > 0 && cpu_delta > 0 {
                        (cpu_delta as f32 / sys_delta as f32) * (online_cpus as f32) * 100.0
                    } else if time_delta_nanos > 0 && cpu_delta > 0 {
                        (cpu_delta as f32 / time_delta_nanos as f32) * 100.0
                    } else {
                        0.0
                    };
                    let max_cpu = (online_cpus.max(1) as f32) * 100.0;
                    let clamped_cpu = cpu_pct.clamp(0.0, max_cpu);

                    (Some((clamped_cpu * 10.0).round() / 10.0), mem_mb, limit_mb)
                } else {
                    (None, None, None)
                }
            } else {
                (None, None, None)
            };

            let status = ContainerStatus {
                name: name.clone(),
                state,
                image,
                uptime: started_at,
                cpu_usage_pct,
                memory_used_mb,
                memory_limit_mb,
            };

            {
                let mut cache = self.status_cache.lock().unwrap_or_else(|e| e.into_inner());
                cache.insert(name, (std::time::Instant::now(), status.clone()));
            }

            Ok(status)
        })
    }

    fn start(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        self.invalidate_status(name);
        let name = name.to_string();
        Box::pin(async move {
            let docker = self.get_docker()?;
            docker
                .start_container::<String>(&name, None)
                .await
                .map_err(|e| ContainerError::StartFailed(e.to_string()))
        })
    }

    fn stop(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        self.invalidate_status(name);
        let name = name.to_string();
        Box::pin(async move {
            let docker = self.get_docker()?;
            docker
                .stop_container(&name, None)
                .await
                .map_err(|e| ContainerError::StopFailed(e.to_string()))
        })
    }

    fn restart(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        self.invalidate_status(name);
        let name = name.to_string();
        Box::pin(async move {
            let docker = self.get_docker()?;
            docker
                .restart_container(&name, None)
                .await
                .map_err(|e| ContainerError::RestartFailed(e.to_string()))
        })
    }

    fn update_image(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        self.invalidate_status(name);
        let name = name.to_string();
        Box::pin(async move {
            let docker = self.get_docker()?;
            let inspect = docker
                .inspect_container(&name, None)
                .await
                .map_err(|e| ContainerError::Infrastructure(e.to_string()))?;

            let image = inspect
                .config
                .and_then(|c| c.image)
                .unwrap_or_else(|| "vaultwarden/server:latest".to_string());

            self.pull_image(&docker, &image).await
        })
    }

    fn rebuild(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        self.invalidate_status(name);
        let name = name.to_string();
        Box::pin(async move {
            let docker = self.get_docker()?;

            // 1. Inspecionar container atual para guardar configurações antes de remover
            let inspect = docker
                .inspect_container(&name, None)
                .await
                .map_err(|e| ContainerError::Infrastructure(e.to_string()))?;

            let was_running = inspect
                .state
                .as_ref()
                .and_then(|s| s.running)
                .unwrap_or(false);

            let image = inspect
                .config
                .as_ref()
                .and_then(|c| c.image.clone())
                .unwrap_or_else(|| "vaultwarden/server:latest".to_string());

            // 2. Pull da imagem mais recente diretamente sem re-inspecionar o container
            self.pull_image(&docker, &image).await?;

            // 3. Parar container se estiver rodando
            if was_running {
                let _ = docker.stop_container(&name, None).await;
            }

            // 4. Renomear container antigo para backup temporário em vez de destruí-lo prematuramente
            let backup_name = format!("{}_rebuild_bak", name);
            let _ = docker.remove_container(&backup_name, None).await;
            docker
                .rename_container(&name, RenameContainerOptions { name: backup_name.clone() })
                .await
                .map_err(|e| ContainerError::RebuildFailed(format!("Failed to rename container to backup: {e}")))?;

            let networking_config = inspect.network_settings.and_then(|ns| {
                ns.networks.map(|networks| bollard::container::NetworkingConfig {
                    endpoints_config: networks,
                })
            });

            // 5. Recriar com as mesmas configurações de volume, ambiente e rede
            let create_config = bollard::container::Config {
                image: inspect.config.as_ref().and_then(|c| c.image.clone()),
                env: inspect.config.as_ref().and_then(|c| c.env.clone()),
                cmd: inspect.config.as_ref().and_then(|c| c.cmd.clone()),
                entrypoint: inspect.config.as_ref().and_then(|c| c.entrypoint.clone()),
                exposed_ports: inspect.config.as_ref().and_then(|c| c.exposed_ports.clone()),
                volumes: inspect.config.as_ref().and_then(|c| c.volumes.clone()),
                working_dir: inspect.config.as_ref().and_then(|c| c.working_dir.clone()),
                host_config: inspect.host_config,
                networking_config,
                ..Default::default()
            };

            let create_options = Some(CreateContainerOptions {
                name: name.clone(),
                platform: None,
            });

            if let Err(e) = docker.create_container(create_options, create_config).await {
                // Rollback: restaurar nome do backup e reiniciar apenas se estava rodando
                execute_rollback(
                    false,
                    was_running,
                    || async { Ok(()) },
                    || async { docker.rename_container(&backup_name, RenameContainerOptions { name: name.clone() }).await.map_err(|e| e.to_string()) },
                    || async { docker.start_container::<String>(&name, None).await.map_err(|e| e.to_string()) },
                ).await;
                return Err(ContainerError::RebuildFailed(format!("Recreation failed (rolled back): {e}")));
            }

            // 6. Iniciar o novo container
            if let Err(e) = docker.start_container::<String>(&name, None).await {
                // Rollback: remover novo container quebrado, restaurar backup e reiniciar apenas se estava rodando
                execute_rollback(
                    true,
                    was_running,
                    || async { docker.remove_container(&name, None).await.map_err(|e| e.to_string()) },
                    || async { docker.rename_container(&backup_name, RenameContainerOptions { name: name.clone() }).await.map_err(|e| e.to_string()) },
                    || async { docker.start_container::<String>(&name, None).await.map_err(|e| e.to_string()) },
                ).await;
                return Err(ContainerError::StartFailed(format!("Start failed (rolled back): {e}")));
            }

            // 7. Sucesso confirmado: remover backup com segurança
            let _ = docker.remove_container(&backup_name, None).await;

            Ok(())
        })
    }

    fn exec_command(&self, name: &str, cmd: Vec<String>) -> BoxFuture<'_, Result<String, ContainerError>> {
        let name = name.to_string();
        Box::pin(async move {
            let docker = self.get_docker()?;
            let exec_config = bollard::exec::CreateExecOptions {
                cmd: Some(cmd),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            };
            let exec = docker
                .create_exec(&name, exec_config)
                .await
                .map_err(|e| ContainerError::ExecFailed(e.to_string()))?;

            let exec_fut = async {
                let mut output = String::new();
                if let bollard::exec::StartExecResults::Attached { output: mut stream, .. } = docker
                    .start_exec(&exec.id, None)
                    .await
                    .map_err(|e| ContainerError::ExecFailed(e.to_string()))?
                {
                    const MAX_EXEC_OUTPUT_BYTES: usize = 64 * 1024;
                    while let Some(Ok(msg)) = stream.next().await {
                        let msg_str = msg.to_string();
                        if output.len() + msg_str.len() <= MAX_EXEC_OUTPUT_BYTES {
                            output.push_str(&msg_str);
                        } else if output.len() < MAX_EXEC_OUTPUT_BYTES {
                            let remain = MAX_EXEC_OUTPUT_BYTES - output.len();
                            output.push_str(&msg_str[..remain]);
                            output.push_str("... [output truncated]");
                        }
                    }
                }

                let inspect = docker
                    .inspect_exec(&exec.id)
                    .await
                    .map_err(|e| ContainerError::ExecFailed(e.to_string()))?;

                if let Some(exit_code) = inspect.exit_code
                    && exit_code != 0 {
                        return Err(ContainerError::ExecFailed(format!(
                            "Command exited with code {exit_code}: {output}"
                        )));
                    }

                Ok(output)
            };

            const EXEC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
            tokio::time::timeout(EXEC_TIMEOUT, exec_fut)
                .await
                .map_err(|_| ContainerError::ExecFailed("Container command execution timed out (300s limit exceeded)".to_string()))?
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RollbackActions {
    pub removed_new: bool,
    pub renamed_backup: bool,
    pub restarted_original: bool,
}

pub(crate) async fn execute_rollback<FRem, FRen, FStart, Fut1, Fut2, Fut3>(
    cleanup_new: bool,
    was_running: bool,
    mut remove_new: FRem,
    mut rename_backup: FRen,
    mut start_original: FStart,
) -> RollbackActions
where
    FRem: FnMut() -> Fut1,
    FRen: FnMut() -> Fut2,
    FStart: FnMut() -> Fut3,
    Fut1: std::future::Future<Output = Result<(), String>>,
    Fut2: std::future::Future<Output = Result<(), String>>,
    Fut3: std::future::Future<Output = Result<(), String>>,
{
    let mut actions = RollbackActions {
        removed_new: false,
        renamed_backup: false,
        restarted_original: false,
    };
    if cleanup_new {
        let _ = remove_new().await;
        actions.removed_new = true;
    }
    let _ = rename_backup().await;
    actions.renamed_backup = true;
    if was_running {
        let _ = start_original().await;
        actions.restarted_original = true;
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_adapter_new() {
        let adapter = DockerAdapter::new("/var/run/docker.sock");
        assert_eq!(adapter.socket_path, "/var/run/docker.sock");
        let map = adapter.prev_cpu.lock().unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_docker_adapter_prev_cpu_eviction_logic() {
        let adapter = DockerAdapter::new("/var/run/docker.sock");
        let now = std::time::Instant::now();
        {
            let mut map = adapter.prev_cpu.lock().unwrap();
            // Inserir chave expirada (> 600s)
            map.insert("old_container".to_string(), (100, 200, now - std::time::Duration::from_secs(700)));
            // Inserir chave recente
            map.insert("new_container".to_string(), (100, 200, now));

            // Simular lógica de retenção
            map.retain(|_, (_, _, t)| now.duration_since(*t).as_secs() < 600);
            assert_eq!(map.len(), 1);
            assert!(map.contains_key("new_container"));
            assert!(!map.contains_key("old_container"));
        }
    }

    #[tokio::test]
    async fn test_rollback_on_create_failure_when_running() {
        let actions = execute_rollback(
            false,
            true,
            || async { panic!("Should not remove new container") },
            || async { Ok(()) },
            || async { Ok(()) },
        )
        .await;
        assert_eq!(
            actions,
            RollbackActions {
                removed_new: false,
                renamed_backup: true,
                restarted_original: true,
            }
        );
    }

    #[tokio::test]
    async fn test_rollback_on_create_failure_when_stopped() {
        let actions = execute_rollback(
            false,
            false,
            || async { panic!("Should not remove new container") },
            || async { Ok(()) },
            || async { panic!("Should not start container if stopped") },
        )
        .await;
        assert_eq!(
            actions,
            RollbackActions {
                removed_new: false,
                renamed_backup: true,
                restarted_original: false,
            }
        );
    }

    #[tokio::test]
    async fn test_rollback_on_start_failure() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let remove_called = std::sync::Arc::new(AtomicBool::new(false));
        let remove_called_clone = remove_called.clone();
        let actions = execute_rollback(
            true,
            true,
            move || {
                let flag = remove_called_clone.clone();
                async move {
                    flag.store(true, Ordering::SeqCst);
                    Ok(())
                }
            },
            || async { Ok(()) },
            || async { Ok(()) },
        )
        .await;
        assert!(remove_called.load(Ordering::SeqCst));
        assert_eq!(
            actions,
            RollbackActions {
                removed_new: true,
                renamed_backup: true,
                restarted_original: true,
            }
        );
    }
}
