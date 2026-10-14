use crate::domain::container::{BoxFuture, ContainerError, ContainerPort, ContainerState, ContainerStatus};
use bollard::container::{CreateContainerOptions, RenameContainerOptions, StatsOptions};
use bollard::image::CreateImageOptions;
use bollard::Docker;
use futures_util::StreamExt;

use std::collections::HashMap;
use std::sync::Mutex;

pub struct DockerAdapter {
    socket_path: String,
    docker: Option<Docker>,
    prev_cpu: Mutex<HashMap<String, (u64, u64)>>,
}

impl DockerAdapter {
    pub fn new(socket_path: &str) -> Self {
        let docker = Docker::connect_with_unix(socket_path, 120, bollard::API_DEFAULT_VERSION).ok();
        Self {
            socket_path: socket_path.to_string(),
            docker,
            prev_cpu: Mutex::new(HashMap::new()),
        }
    }

    fn get_docker(&self) -> Result<Docker, ContainerError> {
        if let Some(ref d) = self.docker {
            return Ok(d.clone());
        }
        Docker::connect_with_unix(&self.socket_path, 120, bollard::API_DEFAULT_VERSION)
            .map_err(|_| ContainerError::SocketUnavailable(self.socket_path.clone()))
    }
}

impl ContainerPort for DockerAdapter {
    fn get_status(&self, name: &str) -> BoxFuture<'_, Result<ContainerStatus, ContainerError>> {
        let name = name.to_string();
        Box::pin(async move {
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

            let (cpu_usage_pct, memory_used_mb) = if is_running {
                let mut stats_stream = docker.stats(&name, Some(StatsOptions { stream: false, one_shot: true }));
                if let Some(Ok(stats)) = stats_stream.next().await {
                    let mem_mb = stats.memory_stats.usage.map(|u| u / (1024 * 1024));

                    let cur_cpu = stats.cpu_stats.cpu_usage.total_usage;
                    let cur_sys = stats.cpu_stats.system_cpu_usage.unwrap_or(0);
                    let online_cpus = stats.cpu_stats.online_cpus.unwrap_or(1);

                    let (cpu_delta, sys_delta) = {
                        let mut prev_map = self.prev_cpu.lock().unwrap_or_else(|e| e.into_inner());
                        let deltas = if let Some(&(prev_c, prev_s)) = prev_map.get(&name) {
                            (cur_cpu.saturating_sub(prev_c), cur_sys.saturating_sub(prev_s))
                        } else {
                            let precpu = stats.precpu_stats.cpu_usage.total_usage;
                            let presys = stats.precpu_stats.system_cpu_usage.unwrap_or(0);
                            (cur_cpu.saturating_sub(precpu), cur_sys.saturating_sub(presys))
                        };
                        prev_map.insert(name.clone(), (cur_cpu, cur_sys));
                        deltas
                    };

                    let cpu_pct = if sys_delta > 0 && cpu_delta > 0 {
                        (cpu_delta as f32 / sys_delta as f32) * (online_cpus as f32) * 100.0
                    } else {
                        0.0
                    };

                    (Some(cpu_pct), mem_mb)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            Ok(ContainerStatus {
                name,
                state,
                image,
                uptime: started_at,
                cpu_usage_pct,
                memory_used_mb,
            })
        })
    }

    fn start(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
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

            let options = Some(CreateImageOptions {
                from_image: image,
                ..Default::default()
            });

            let mut stream = docker.create_image(options, None, None);
            while let Some(item) = stream.next().await {
                if let Err(e) = item {
                    return Err(ContainerError::ImagePullFailed(e.to_string()));
                }
            }

            Ok(())
        })
    }

    fn rebuild(&self, name: &str) -> BoxFuture<'_, Result<(), ContainerError>> {
        let name = name.to_string();
        Box::pin(async move {
            let docker = self.get_docker()?;

            // 1. Inspecionar container atual para guardar configurações antes de remover
            let inspect = docker
                .inspect_container(&name, None)
                .await
                .map_err(|e| ContainerError::Infrastructure(e.to_string()))?;

            // 2. Pull da imagem mais recente
            self.update_image(&name).await?;

            // 3. Parar container se estiver rodando
            let _ = docker.stop_container(&name, None).await;

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
                // Rollback: restaurar nome do backup e reiniciar
                let _ = docker.rename_container(&backup_name, RenameContainerOptions { name: name.clone() }).await;
                let _ = docker.start_container::<String>(&name, None).await;
                return Err(ContainerError::RebuildFailed(format!("Recreation failed (rolled back): {e}")));
            }

            // 6. Iniciar o novo container
            if let Err(e) = docker.start_container::<String>(&name, None).await {
                // Rollback: remover novo container quebrado, restaurar backup e reiniciar
                let _ = docker.remove_container(&name, None).await;
                let _ = docker.rename_container(&backup_name, RenameContainerOptions { name: name.clone() }).await;
                let _ = docker.start_container::<String>(&name, None).await;
                return Err(ContainerError::StartFailed(format!("Start failed (rolled back): {e}")));
            }

            // 7. Sucesso confirmado: remover backup com segurança
            let _ = docker.remove_container(&backup_name, None).await;

            Ok(())
        })
    }
}
